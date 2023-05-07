use std::path::PathBuf;

pub fn root_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR")))
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn tests_path() -> PathBuf {
    root_path().join("cli").join("tests")
}

pub fn testdata_path() -> PathBuf {
    tests_path().join("testdata")
}
