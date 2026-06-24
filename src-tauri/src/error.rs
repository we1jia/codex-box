// src-tauri/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Config not found at {0}")]
    ConfigNotFound(String),

    #[error("Backup directory error: {0}")]
    BackupDir(String),

    #[error("Atomic write failed: {0}")]
    AtomicWrite(String),

    #[error("Invalid UTF-8 in {0}")]
    InvalidUtf8(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
