// src-tauri/src/config/loader.rs
use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::path::Path;

/// 读取 config 文件原始文本
pub fn read_raw(path: &Path) -> AppResult<String> {
    if !path.exists() {
        return Err(AppError::ConfigNotFound(path.display().to_string()));
    }
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| AppError::InvalidUtf8(path.display().to_string()))
}

/// 文件元信息
pub struct Metadata {
    pub size_bytes: u64,
    pub content_hash: String,
}

pub fn metadata(path: &Path) -> AppResult<Metadata> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());
    Ok(Metadata {
        size_bytes: bytes.len() as u64,
        content_hash: hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn read_raw_returns_content() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello world").unwrap();
        let content = read_raw(f.path()).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn read_raw_missing_file_errors() {
        let path = Path::new("/tmp/codex-box-nonexistent-zzzz.toml");
        let _ = std::fs::remove_file(path);
        assert!(matches!(
            read_raw(path),
            Err(AppError::ConfigNotFound(_))
        ));
    }

    #[test]
    fn metadata_returns_size_and_hash() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"abc").unwrap();
        let m = metadata(f.path()).unwrap();
        assert_eq!(m.size_bytes, 3);
        assert_eq!(
            m.content_hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
