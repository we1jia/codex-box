// src-tauri/src/config/writer.rs
use crate::error::{AppError, AppResult};
use std::path::Path;

pub fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::AtomicWrite(format!("no parent for {}", path.display())))?;

    if !parent.exists() {
        return Err(AppError::AtomicWrite(format!(
            "parent dir does not exist: {}",
            parent.display()
        )));
    }

    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::AtomicWrite(format!("no file_name for {}", path.display())))?;
    let tmp = parent.join(format!("{}.tmp", file_name.to_string_lossy()));

    std::fs::write(&tmp, content)
        .map_err(|e| AppError::AtomicWrite(format!("write tmp {}: {}", tmp.display(), e)))?;

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::AtomicWrite(format!(
            "rename {} -> {}: {}",
            tmp.display(),
            path.display(),
            e
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_file_with_content() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        atomic_write(&target, "hello").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        fs::write(&target, "old").unwrap();
        atomic_write(&target, "new").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn atomic_write_does_not_leave_tmp_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        atomic_write(&target, "x").unwrap();
        let tmp = dir.path().join("out.toml.tmp");
        assert!(!tmp.exists(), "tmp file leaked");
    }

    #[test]
    fn atomic_write_preserves_target_on_invalid_path() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("nonexistent_subdir").join("out.toml");
        let result = atomic_write(&target, "x");
        assert!(result.is_err());
    }
}
