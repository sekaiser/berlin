use crate::args::ConfigFlag;
use crate::args::Flags;
use crate::util::fs::canonicalize_path;
use crate::util::path::specifier_to_file_path;

use files::ModuleSpecifier;
use libs::anyhow::anyhow;
use libs::anyhow::bail;
use libs::anyhow::Context;
use libs::anyhow::Error;
use libs::log;
use libs::toml;
use libs::toml::Value;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

fn parse_site_config(
    site_config: &HashMap<String, Value>,
    maybe_specifier: Option<ModuleSpecifier>,
) -> Result<SiteConfig, Error> {
    let mut filtered: HashMap<String, Value> = HashMap::new();

    for (key, value) in site_config.iter() {
        let key = key.as_str();
        filtered.insert(key.to_string(), value.to_owned());
    }
    let s = toml::ser::to_string(&filtered).unwrap();
    let value = toml::from_str(&s)?;

    Ok(value)
}

fn parse_profiles_config(
    profiles_config: &HashMap<String, Value>,
    maybe_specifier: Option<ModuleSpecifier>,
) -> Result<ProfilesConfig, Error> {
    let mut filtered: HashMap<String, Value> = HashMap::new();

    for (key, value) in profiles_config.iter() {
        let key = key.as_str();
        filtered.insert(key.to_string(), value.to_owned());
    }
    let s = toml::ser::to_string(&filtered).unwrap();
    let value = toml::from_str(&s)?;

    Ok(value)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SiteConfig {
    pub url: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProfilesConfig {
    pub linkedin: Option<String>,
    pub github: Option<String>,
    pub twitter: Option<String>,
    pub mastodon: Option<String>,
}

impl ProfilesConfig {
    pub fn empty() -> Self {
        Self {
            linkedin: None,
            github: None,
            twitter: None,
            mastodon: None,
        }
    }
}

impl SiteConfig {
    pub fn empty() -> SiteConfig {
        SiteConfig {
            url: None,
            description: None,
            title: None,
            author: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub specifier: ModuleSpecifier,
    pub toml: ConfigFileToml,
}

impl ConfigFile {
    pub fn empty() -> ConfigFile {
        ConfigFile {
            specifier: ModuleSpecifier::parse("foo:bar").unwrap(),
            toml: ConfigFileToml {
                site: None,
                profiles: None,
            },
        }
    }

    pub fn discover(flags: &Flags) -> Result<Option<ConfigFile>, Error> {
        match &flags.config_flag {
            ConfigFlag::Path(config_path) => Ok(Some(ConfigFile::read(config_path)?)),
            ConfigFlag::Discover => {
                if let Some(config_path_args) = flags.config_path_args() {
                    let mut checked = HashSet::new();
                    for f in config_path_args {
                        if let Some(cf) = Self::discover_from(&f, &mut checked)? {
                            return Ok(Some(cf));
                        }
                    }
                    // From CWD walk up to root looking for deno.json or deno.jsonc
                    let cwd = std::env::current_dir()?;
                    Self::discover_from(&cwd, &mut checked)
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn discover_from(
        start: &Path,
        checked: &mut HashSet<PathBuf>,
    ) -> Result<Option<ConfigFile>, Error> {
        /// Filenames that Berlin will recognize when discovering config.
        const CONFIG_FILE_NAMES: [&str; 1] = ["berlin.toml"];

        for ancestor in start.ancestors() {
            if checked.insert(ancestor.to_path_buf()) {
                for config_filename in CONFIG_FILE_NAMES {
                    let f = ancestor.join(config_filename);
                    match ConfigFile::read(&f) {
                        Ok(cf) => {
                            log::debug!("Config file found at '{}'", f.display());
                            return Ok(Some(cf));
                        }
                        Err(e) => {
                            if let Some(ioerr) = e.downcast_ref::<std::io::Error>() {
                                use std::io::ErrorKind::*;
                                match ioerr.kind() {
                                    InvalidInput | PermissionDenied | NotFound => {
                                        // ok keep going
                                    }
                                    _ => {
                                        return Err(e); // Unknown error. Stop.
                                    }
                                }
                            } else {
                                return Err(e); // Parse error or something else. Stop.
                            }
                        }
                    }
                }
            }
        }
        // No config file found.
        Ok(None)
    }

    pub fn read(path_ref: impl AsRef<Path>) -> Result<Self, Error> {
        let path = Path::new(path_ref.as_ref());
        let config_file = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path_ref)
        };

        // perf: Check if the config file exists before canonicalizing path.
        if !config_file.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Could not find the config file: {}",
                    config_file.to_string_lossy()
                ),
            )
            .into());
        }

        let config_path = canonicalize_path(&config_file).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Could not find the config file: {}",
                    config_file.to_string_lossy()
                ),
            )
        })?;
        let config_specifier = ModuleSpecifier::from_file_path(&config_path).map_err(|_| {
            anyhow!(
                "Could not convert path to specifier. Path: {}",
                config_path.display()
            )
        })?;
        Self::from_specifier(&config_specifier)
    }

    pub fn from_specifier(specifier: &ModuleSpecifier) -> Result<Self, Error> {
        let config_path = specifier_to_file_path(specifier)?;
        let config_text = match std::fs::read_to_string(config_path) {
            Ok(text) => text,
            Err(err) => bail!(
                "Error reading config file {}: {}",
                specifier,
                err.to_string()
            ),
        };
        Self::new(&config_text, specifier)
    }

    pub fn new(text: &str, specifier: &ModuleSpecifier) -> Result<Self, Error> {
        let toml = match toml::from_str::<ConfigFileToml>(text) {
            Ok(toml) => toml,
            Err(e) => {
                return Err(anyhow!(
                    "Unable to parse config file TOML {} because of {}",
                    specifier,
                    e.to_string()
                ))
            }
        };

        Ok(Self {
            specifier: specifier.to_owned(),
            toml,
        })
    }

    /// Parse `siteOptions` and return a serde `Value`.
    /// The result also contains any options that were ignored.
    pub fn to_site_config(&self) -> Result<SiteConfig, Error> {
        if let Some(site_config) = self.toml.site.clone() {
            let s = toml::ser::to_string(&site_config).unwrap();
            let config: HashMap<String, Value> =
                toml::from_str(&s).context("site config should be an object")?;
            parse_site_config(&config, Some(self.specifier.to_owned()))
        } else {
            Ok(SiteConfig::empty())
        }
    }

    pub fn to_profiles_config(&self) -> Result<ProfilesConfig, Error> {
        if let Some(profiles_config) = self.toml.profiles.clone() {
            let s = toml::ser::to_string(&profiles_config).unwrap();
            let config: HashMap<String, Value> =
                toml::from_str(&s).context("profiles config should be an object")?;
            parse_profiles_config(&config, Some(self.specifier.to_owned()))
        } else {
            Ok(ProfilesConfig::empty())
        }
    }
}

/// A structure for managing the configuration of Berlin
#[derive(Clone, Debug, Deserialize)]
pub struct ConfigFileToml {
    pub site: Option<Value>,
    pub profiles: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let config_text = r#"
            [site]
            url = "http://localhost:8081"
            title = "My Test Blog"
            description = "A generated blog for testing purposes."
        "#;

        let config_dir = ModuleSpecifier::parse("file:///berlin/").unwrap();
        let config_specifier = config_dir.join("berlin.toml").unwrap();
        let config_file = ConfigFile::new(config_text, &config_specifier).unwrap();
        let site_config = config_file.to_site_config().expect("error parsing");
        let SiteConfig {
            url,
            title,
            description,
            ..
        } = site_config;
        assert_eq!(url, Some("http://localhost:8081".to_string()));
        assert_eq!(title, Some("My Test Blog".to_string()));
        assert_eq!(
            description,
            Some("A generated blog for testing purposes.".to_string())
        );
    }

    #[test]
    fn test_parse_config_with_empty_file() {
        let config_text = "";
        let config_specifier = ModuleSpecifier::parse("file:///berlin/berlin.toml").unwrap();
        let config_file = ConfigFile::new(&config_text, &config_specifier).unwrap();
        let site_config = config_file.to_site_config();
        assert!(site_config.is_ok());
    }

    #[test]
    fn test_config_with_invalid_file() {
        let config_text = r#"{}"#;
        let config_specifier = ModuleSpecifier::parse("file:///berlin/berlin.toml").unwrap();
        let config_file = ConfigFile::new(&config_text, &config_specifier).unwrap();
        let site_config = config_file.to_site_config();
        assert!(site_config.is_err());
    }

    #[test]
    fn test_parse_config_with_commented_file() {
        let config_text = r#"#{"foo":"bar"}"#;
        let config_specifier = ModuleSpecifier::parse("file:///berlin/berlin.toml").unwrap();
        let config_file = ConfigFile::new(config_text, &config_specifier).unwrap();
        let site_config = config_file.to_site_config();
        assert!(site_config.is_ok());
    }

    #[test]
    fn test_discover_from_success() {
        let testdata = test_util::testdata_path();
        let toml_file = testdata.join("site_config/berlin.toml");
        let mut checked = HashSet::new();
        let config_file = ConfigFile::discover_from(&toml_file, &mut checked)
            .unwrap()
            .unwrap();

        assert!(checked.contains(toml_file.parent().unwrap()));
        assert!(!checked.contains(&testdata));
        assert!(config_file.toml.site.is_some());
    }

    #[test]
    fn test_discover_from_malformed() {
        let testdata = test_util::testdata_path();
        let d = testdata.join("malformed_config/");
        let mut checked = HashSet::new();
        let err = ConfigFile::discover_from(&d, &mut checked).unwrap_err();
        assert!(err.to_string().contains("Unable to parse config file"));
    }
}
