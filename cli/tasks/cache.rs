//! Disposable, content-addressed operation results. Consumers define complete
//! input keys and validate typed values; this module checks storage integrity.
//! Cache files never choose output destinations and are never published directly.
use anyhow::{Context, Error, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_ENTRY: u64 = 128 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CacheKey(String);

impl CacheKey {
    pub fn new(operation: &str, inputs: &impl Serialize) -> Result<Self, Error> {
        Ok(Self(hash(&serde_json::to_vec(&(operation, inputs))?)))
    }
}

pub(super) struct Artifact<T> {
    pub value: T,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct Header {
    key: String,
    value: serde_json::Value,
    checksum: String,
}

pub(super) struct ArtifactCache {
    project_root: PathBuf,
}

impl ArtifactCache {
    pub fn new(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_owned(),
        }
    }

    fn directory(&self, create: bool) -> Result<Option<PathBuf>, Error> {
        let mut path = self.project_root.canonicalize()?;
        for component in [".berlin", "cache", "artifacts-v1"] {
            path.push(component);
            if create
                && let Err(error) = fs::create_dir(&path)
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(error.into());
            }
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                _ => bail!(
                    "cache directory is not a real directory: {}",
                    path.display()
                ),
            }
        }
        Ok(Some(path))
    }

    /// A corrupt or unavailable entry is a miss, not a failed build.
    pub fn load<T: DeserializeOwned>(&self, key: &CacheKey) -> Option<Artifact<T>> {
        match self.read(key) {
            Ok(value) => value,
            Err(error) => {
                log::warn!("Ignoring unusable operation cache: {error:#}");
                None
            }
        }
    }

    fn read<T: DeserializeOwned>(&self, key: &CacheKey) -> Result<Option<Artifact<T>>, Error> {
        let Some(directory) = self.directory(false)? else {
            return Ok(None);
        };
        let path = directory.join(format!("{}.bin", key.0));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_ENTRY => {}
            _ => bail!("invalid cache entry: {}", path.display()),
        }
        let mut data = Vec::new();
        fs::File::open(path)?
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut data)?;
        if data.len() as u64 > MAX_ENTRY {
            bail!("cache entry exceeds size limit");
        }
        let split = data
            .iter()
            .position(|byte| *byte == b'\n')
            .context("cache header missing")?;
        let header: Header = serde_json::from_slice(&data[..split])?;
        let bytes = &data[split + 1..];
        if header.key != key.0 || header.checksum != checksum(&header.key, &header.value, bytes)? {
            bail!("cache checksum mismatch");
        }
        Ok(Some(Artifact {
            value: serde_json::from_value(header.value)?,
            bytes: bytes.to_vec(),
        }))
    }

    pub fn store<T: Serialize>(&self, key: &CacheKey, artifact: &Artifact<T>) {
        if let Err(error) = self.write(key, artifact) {
            log::warn!("Unable to save operation cache: {error:#}");
        }
    }

    fn write<T: Serialize>(&self, key: &CacheKey, artifact: &Artifact<T>) -> Result<(), Error> {
        let value = serde_json::to_value(&artifact.value)?;
        let header = serde_json::to_vec(&Header {
            key: key.0.clone(),
            checksum: checksum(&key.0, &value, &artifact.bytes)?,
            value,
        })?;
        if (header.len() + 1 + artifact.bytes.len()) as u64 > MAX_ENTRY {
            bail!("artifact exceeds cache size limit");
        }
        let directory = self.directory(true)?.context("missing cache directory")?;
        let path = directory.join(format!("{}.bin", key.0));
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() => bail!("refusing non-file cache destination"),
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&header)?;
        temporary.write_all(b"\n")?;
        temporary.write_all(&artifact.bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    }
}

fn checksum(key: &str, value: &serde_json::Value, bytes: &[u8]) -> Result<String, Error> {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&(key, value))?);
    digest.update(bytes);
    Ok(hex(&digest.finalize()))
}

pub(super) fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(super) fn file_hash(path: &Path) -> Result<String, Error> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex(&digest.finalize()))
}

/// One identity for this running compiler, shared by all operation caches.
/// Includes local, uncommitted implementation changes in development builds.
pub(super) fn implementation_fingerprint() -> Result<&'static str, Error> {
    static FINGERPRINT: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    FINGERPRINT
        .get_or_init(|| {
            std::env::current_exe()
                .map_err(Error::from)
                .and_then(|path| file_hash(&path))
                .map_err(|error| error.to_string())
        })
        .as_deref()
        .map_err(|error| anyhow::anyhow!(error.clone()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_cover_operation_and_inputs_and_entries_are_disposable() {
        let root = tempfile::tempdir().unwrap();
        let cache = ArtifactCache::new(root.path());
        let key = CacheKey::new("compile/v1", &("source", "dependency", "options")).unwrap();
        assert!(cache.load::<String>(&key).is_none());
        assert!(!root.path().join(".berlin").exists());
        cache.store(
            &key,
            &Artifact {
                value: "document".to_owned(),
                bytes: b"pdf".to_vec(),
            },
        );
        let result = cache.load::<String>(&key).unwrap();
        assert_eq!(result.value, "document");
        assert_eq!(result.bytes, b"pdf");
        for (operation, inputs) in [
            ("compile/v2", ("source", "dependency", "options")),
            ("compile/v1", ("edited", "dependency", "options")),
            ("compile/v1", ("source", "edited", "options")),
            ("compile/v1", ("source", "dependency", "edited")),
        ] {
            assert!(
                cache
                    .load::<String>(&CacheKey::new(operation, &inputs).unwrap())
                    .is_none()
            );
        }
        let path = cache
            .directory(false)
            .unwrap()
            .unwrap()
            .join(format!("{}.bin", key.0));
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(cache.load::<String>(&key).is_none());
        cache.store(
            &key,
            &Artifact {
                value: "repaired",
                bytes: b"pdf".to_vec(),
            },
        );
        assert_eq!(cache.load::<String>(&key).unwrap().value, "repaired");
        fs::write(path, b"partial").unwrap();
        assert!(cache.load::<String>(&key).is_none());
    }

    #[test]
    #[cfg(unix)]
    fn never_reads_or_writes_through_cache_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(".berlin")).unwrap();
        let cache = ArtifactCache::new(root.path());
        let key = CacheKey::new("test", &1).unwrap();
        assert!(cache.load::<String>(&key).is_none());
        cache.store(
            &key,
            &Artifact {
                value: "test",
                bytes: vec![],
            },
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
