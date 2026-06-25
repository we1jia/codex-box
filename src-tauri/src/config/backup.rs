// src-tauri/src/config/backup.rs
use crate::config::model::{BackupReason, BackupRecord};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn create_backup(
    config_path: &Path,
    backup_dir: &Path,
    reason: BackupReason,
) -> AppResult<BackupRecord> {
    create_backup_with_extension(config_path, backup_dir, reason, "toml")
}

/// 创建备份并按 extension 选择后缀（用于 JSON 备份、纯文本备份等场景）
pub fn create_backup_with_extension(
    config_path: &Path,
    backup_dir: &Path,
    reason: BackupReason,
    extension: &str,
) -> AppResult<BackupRecord> {
    if !config_path.exists() {
        return Err(AppError::ConfigNotFound(config_path.display().to_string()));
    }

    let bytes = std::fs::read(config_path)?;
    let size_bytes = bytes.len() as u64;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let content_hash = format!("{:x}", hasher.finalize());

    std::fs::create_dir_all(backup_dir)
        .map_err(|e| AppError::BackupDir(format!("{}: {}", backup_dir.display(), e)))?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let short_hash = &content_hash[..8];
    let filename = format!("codex-box-{}-{}.{}", timestamp, short_hash, extension);
    let dest = backup_dir.join(&filename);

    std::fs::write(&dest, &bytes)
        .map_err(|e| AppError::BackupDir(format!("write failed: {}", e)))?;

    let id = format!("{}-{}", timestamp, short_hash);
    let created_at = Utc::now().to_rfc3339();

    Ok(BackupRecord {
        id,
        created_at,
        file_path: dest.display().to_string(),
        reason,
        content_hash,
        size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use tempfile::NamedTempFile;

    #[test]
    fn create_backup_copies_file_to_backup_dir() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"original config").unwrap();
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().to_path_buf();

        let rec = create_backup(src.path(), &backup_dir, BackupReason::Manual).unwrap();

        assert!(rec.file_path.contains("codex-box"));
        assert!(rec.file_path.ends_with(".toml"));
        assert!(std::path::Path::new(&rec.file_path).exists());
        let copied = std::fs::read_to_string(&rec.file_path).unwrap();
        assert_eq!(copied, "original config");
    }

    #[test]
    fn create_backup_records_sha256_and_size() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"abc").unwrap();
        let dir = tempdir().unwrap();
        let rec = create_backup(src.path(), dir.path(), BackupReason::PreWrite).unwrap();
        assert_eq!(rec.size_bytes, 3);
        assert_eq!(
            rec.content_hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn create_backup_missing_source_errors() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("missing.toml");
        let result = create_backup(&src, dir.path(), BackupReason::Manual);
        assert!(result.is_err());
    }

    #[test]
    fn create_backup_creates_dir_if_missing() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"x").unwrap();
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("nested").join("backups");

        let rec = create_backup(src.path(), &backup_dir, BackupReason::Manual).unwrap();
        assert!(backup_dir.exists());
        assert!(std::path::Path::new(&rec.file_path).exists());
    }

    #[test]
    fn create_backup_with_extension_uses_custom_suffix() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"{}").unwrap();
        let dir = tempdir().unwrap();
        let rec =
            create_backup_with_extension(src.path(), dir.path(), BackupReason::PreWrite, "json")
                .unwrap();
        assert!(rec.file_path.ends_with(".json"));
        assert!(std::path::Path::new(&rec.file_path).exists());
    }
}
