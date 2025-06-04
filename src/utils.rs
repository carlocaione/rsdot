use anyhow::Result;
use std::path::PathBuf;

pub fn validate_file(file: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(file);

    if !path.exists() {
        return Err(format!("File '{}' does not exist", file));
    }

    if path.is_absolute() {
        return Err(
            "Use only paths relative to the configuration directory, not absolute".to_string(),
        );
    }

    Ok(path)
}