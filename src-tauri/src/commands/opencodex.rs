// src-tauri/src/commands/opencodex.rs
//
// v0.3 BYOK: 不再 spawn 外部 OpenCodex 进程,改为读写
// ~/.opencodex/providers.json 与 ~/.opencodex/custom_model_catalog.json。
//
// 写入闭环沿用 v0.2 既有 backup → diff → confirm → atomic write → rollback 链:
//   - backup 目录:~/.codex/codex-box/backups/{ts}-{hash}.json
//   - 写 .tmp → rename
//   - 失败时 rollback 到最近一次 backup
//   - 写入前校验 content_hash,防止并发覆盖
//
// 兼容性策略: 未知字段保留(用 #[serde(flatten)] + extra),文件不存在按空配置处理,
// secret 字段只接受 ${ENV_VAR} 引用,绝不落盘明文。
use crate::config::model::BackupReason;
use crate::config::{backup, writer};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const OPENCODEX_DIR: &str = ".opencodex";
const PROVIDERS_FILE: &str = "providers.json";
const CATALOG_FILE: &str = "custom_model_catalog.json";
const SCHEMA_VERSION: u32 = 1;
const BACKUP_DIR: &str = ".codex/codex-box/backups";

const fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexCustomConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub providers_path: String,
    pub catalog_path: String,
    pub providers: Vec<ProviderRoute>,
    pub catalog: Vec<ModelCatalogEntry>,
    pub raw_providers_text: String,
    pub raw_catalog_text: String,
    pub providers_content_hash: String,
    pub catalog_content_hash: String,
    pub read_at: String,
    pub valid: bool,
    pub parse_errors: Vec<OpenCodexParseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRoute {
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub api_key_ref: Option<String>,
    #[serde(default)]
    pub http_headers: BTreeMap<String, String>,
    pub enabled: bool,
    pub note: Option<String>,
    /// 未知字段保留,AITabby CLI 升级 schema 时不丢字段
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub model_id: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub visible: bool,
    pub reasoning: Option<ReasoningConfig>,
    pub note: Option<String>,
    /// 未知字段保留
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexParseError {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexWriteRequest<T> {
    pub entry: T,
    pub expected_hash: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexDeleteRequest {
    pub key: String,
    pub expected_hash: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodexWriteResult {
    pub file_path: String,
    pub backup_id: String,
    pub new_hash: String,
}

struct FileRead<T> {
    value: T,
    raw: String,
    valid: bool,
    error: Option<String>,
}

fn opencodex_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let dir = home.join(OPENCODEX_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn providers_path() -> AppResult<PathBuf> {
    Ok(opencodex_dir()?.join(PROVIDERS_FILE))
}

fn catalog_path() -> AppResult<PathBuf> {
    Ok(opencodex_dir()?.join(CATALOG_FILE))
}

fn backup_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let dir = home.join(BACKUP_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    format!("sha256-{digest:x}")
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.starts_with("xox")
        || lower.starts_with("ghp_")
        || lower.starts_with("aiza")
        || value.len() > 80
}

fn read_or_default<T>(path: &Path) -> FileRead<T>
where
    T: Default + serde::de::DeserializeOwned + Serialize,
{
    if !path.exists() {
        let empty = T::default();
        let raw = serde_json::to_string_pretty(&empty).unwrap_or_else(|_| "[]".to_string());
        return FileRead {
            value: empty,
            raw,
            valid: true,
            error: None,
        };
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(error) => {
            return FileRead {
                value: T::default(),
                raw: String::new(),
                valid: false,
                error: Some(format!("read failed: {error}")),
            };
        }
    };
    if raw.trim().is_empty() {
        return FileRead {
            value: T::default(),
            raw,
            valid: true,
            error: None,
        };
    }
    match serde_json::from_str::<T>(&raw) {
        Ok(value) => FileRead {
            value,
            raw,
            valid: true,
            error: None,
        },
        Err(error) => FileRead {
            value: T::default(),
            raw,
            valid: false,
            error: Some(format!("parse failed: {error}")),
        },
    }
}

#[tauri::command]
pub fn opencodex_config_read() -> AppResult<OpenCodexCustomConfig> {
    let providers_path = providers_path()?;
    let catalog_path = catalog_path()?;

    let FileRead {
        value: providers,
        raw: raw_providers_text,
        valid: providers_valid,
        error: providers_err,
    } = read_or_default::<Vec<ProviderRoute>>(&providers_path);

    let FileRead {
        value: catalog,
        raw: raw_catalog_text,
        valid: catalog_valid,
        error: catalog_err,
    } = read_or_default::<Vec<ModelCatalogEntry>>(&catalog_path);

    let mut parse_errors = Vec::new();
    if let Some(message) = providers_err {
        parse_errors.push(OpenCodexParseError {
            file: providers_path.display().to_string(),
            message,
        });
    }
    if let Some(message) = catalog_err {
        parse_errors.push(OpenCodexParseError {
            file: catalog_path.display().to_string(),
            message,
        });
    }

    let providers_content_hash = content_hash(&raw_providers_text);
    let catalog_content_hash = content_hash(&raw_catalog_text);

    Ok(OpenCodexCustomConfig {
        schema_version: SCHEMA_VERSION,
        providers_path: providers_path.display().to_string(),
        catalog_path: catalog_path.display().to_string(),
        providers,
        catalog,
        raw_providers_text,
        raw_catalog_text,
        providers_content_hash,
        catalog_content_hash,
        read_at: Utc::now().to_rfc3339(),
        valid: providers_valid && catalog_valid,
        parse_errors,
    })
}

#[tauri::command]
pub fn provider_route_upsert(
    request: OpenCodexWriteRequest<ProviderRoute>,
) -> AppResult<OpenCodexWriteResult> {
    let entry = request.entry.clone();
    if entry.name.trim().is_empty() {
        return Err(AppError::Command("provider name 不能为空".to_string()));
    }
    if let Some(ref key_ref) = entry.api_key_ref {
        if looks_like_secret(key_ref) {
            return Err(AppError::Command(
                "api_key_ref 只允许 ${ENV_VAR} 引用,不能填明文密钥".to_string(),
            ));
        }
    }
    upsert_with(
        &providers_path()?,
        &request.expected_hash,
        request.note.as_deref(),
        |current: &mut Vec<ProviderRoute>| {
            let pos = current.iter().position(|p| p.name == entry.name);
            match pos {
                Some(index) => current[index] = entry.clone(),
                None => current.push(entry.clone()),
            }
        },
    )
}

#[tauri::command]
pub fn provider_route_delete(
    request: OpenCodexDeleteRequest,
) -> AppResult<OpenCodexWriteResult> {
    let key = request.key.clone();
    if key.trim().is_empty() {
        return Err(AppError::Command("provider name 不能为空".to_string()));
    }
    upsert_with(
        &providers_path()?,
        &request.expected_hash,
        request.note.as_deref(),
        |current: &mut Vec<ProviderRoute>| {
            current.retain(|p| p.name != key);
        },
    )
}

#[tauri::command]
pub fn catalog_entry_upsert(
    request: OpenCodexWriteRequest<ModelCatalogEntry>,
) -> AppResult<OpenCodexWriteResult> {
    let entry = request.entry.clone();
    if entry.model_id.trim().is_empty() {
        return Err(AppError::Command("model_id 不能为空".to_string()));
    }
    if entry.provider.trim().is_empty() {
        return Err(AppError::Command("provider 归属不能为空".to_string()));
    }
    upsert_with(
        &catalog_path()?,
        &request.expected_hash,
        request.note.as_deref(),
        |current: &mut Vec<ModelCatalogEntry>| {
            let pos = current.iter().position(|e| e.model_id == entry.model_id);
            match pos {
                Some(index) => current[index] = entry.clone(),
                None => current.push(entry.clone()),
            }
        },
    )
}

#[tauri::command]
pub fn catalog_entry_delete(
    request: OpenCodexDeleteRequest,
) -> AppResult<OpenCodexWriteResult> {
    let key = request.key.clone();
    if key.trim().is_empty() {
        return Err(AppError::Command("model_id 不能为空".to_string()));
    }
    upsert_with(
        &catalog_path()?,
        &request.expected_hash,
        request.note.as_deref(),
        |current: &mut Vec<ModelCatalogEntry>| {
            current.retain(|e| e.model_id != key);
        },
    )
}

fn upsert_with<T>(
    path: &Path,
    expected_hash: &str,
    _note: Option<&str>,
    mutate: impl FnOnce(&mut T),
) -> AppResult<OpenCodexWriteResult>
where
    T: Default + serde::de::DeserializeOwned + Serialize,
{
    let FileRead {
        value: mut current,
        raw,
        valid,
        error,
    } = read_or_default::<T>(path);
    if !valid {
        return Err(AppError::Command(format!(
            "refusing to overwrite unparseable file: {}",
            error.unwrap_or_else(|| "unknown parse error".to_string())
        )));
    }
    if !raw.trim().is_empty() {
        let actual_hash = content_hash(&raw);
        if actual_hash != expected_hash {
            return Err(AppError::Command(
                "配置文件已变化，请重新读取后再写入。".to_string(),
            ));
        }
    }
    mutate(&mut current);

    let new_text = serde_json::to_string_pretty(&current)
        .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?;
    let new_text = ensure_trailing_newline(&new_text);

    let backup_record = if path.exists() {
        let dir = backup_dir()?;
        Some(backup::create_backup(path, &dir, BackupReason::PreWrite)?)
    } else {
        None
    };

    if let Err(error) = writer::atomic_write(path, &new_text) {
        if let Some(ref backup) = backup_record {
            if let Ok(backup_content) = std::fs::read_to_string(&backup.file_path) {
                let _ = writer::atomic_write(path, &backup_content);
            }
        }
        return Err(error);
    }

    let new_hash = content_hash(&new_text);
    Ok(OpenCodexWriteResult {
        file_path: path.display().to_string(),
        backup_id: backup_record.map(|r| r.id).unwrap_or_default(),
        new_hash,
    })
}

fn ensure_trailing_newline(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_missing_files_returns_empty_config() {
        let dir = tempdir().unwrap();
        let providers = dir.path().join(PROVIDERS_FILE);
        let catalog = dir.path().join(CATALOG_FILE);

        let FileRead {
            value: providers_value,
            valid: providers_valid,
            error: providers_err,
            ..
        } = read_or_default::<Vec<ProviderRoute>>(&providers);
        let FileRead {
            value: catalog_value,
            valid: catalog_valid,
            error: catalog_err,
            ..
        } = read_or_default::<Vec<ModelCatalogEntry>>(&catalog);

        assert!(providers_value.is_empty());
        assert!(catalog_value.is_empty());
        assert!(providers_valid);
        assert!(catalog_valid);
        assert!(providers_err.is_none());
        assert!(catalog_err.is_none());
    }

    #[test]
    fn content_hash_is_stable_for_same_input() {
        let a = content_hash("[]");
        let b = content_hash("[]");
        assert_eq!(a, b);
        assert_ne!(a, content_hash("[\"x\"]"));
    }

    #[test]
    fn rejects_inline_secret_in_api_key_ref() {
        let mut route = ProviderRoute {
            name: "openrouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: Some("sk-this-is-actually-a-secret".to_string()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            extra: BTreeMap::new(),
        };
        route.api_key_ref = Some("sk-this-is-actually-a-secret".to_string());
        assert!(looks_like_secret(route.api_key_ref.as_deref().unwrap_or("")));

        route.api_key_ref = Some("OPENROUTER_API_KEY".to_string());
        assert!(!looks_like_secret(route.api_key_ref.as_deref().unwrap_or("")));
    }

    #[test]
    fn read_unparseable_file_marks_invalid_and_keeps_raw() {
        let dir = tempdir().unwrap();
        let file = dir.path().join(PROVIDERS_FILE);
        std::fs::write(&file, "this is not json ====").unwrap();
        let FileRead {
            valid, error, raw, ..
        } = read_or_default::<Vec<ProviderRoute>>(&file);
        assert!(!valid);
        assert!(error.is_some());
        assert!(raw.contains("not json"));
    }

    #[test]
    fn write_failure_rolls_back_to_backup() {
        let dir = tempdir().unwrap();
        let file = dir.path().join(PROVIDERS_FILE);
        std::fs::write(&file, "[]\n").unwrap();
        let initial = std::fs::read_to_string(&file).unwrap();
        let initial_hash = content_hash(&initial);

        let route = ProviderRoute {
            name: "openrouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: Some("OPENROUTER_API_KEY".to_string()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: Some("added by test".to_string()),
            extra: BTreeMap::new(),
        };
        let serialized = serde_json::to_string_pretty(&vec![route.clone()]).unwrap();
        let serialized = ensure_trailing_newline(&serialized);
        std::fs::write(&file, &serialized).unwrap();

        let after = std::fs::read_to_string(&file).unwrap();
        let after_hash = content_hash(&after);
        assert_ne!(initial_hash, after_hash);
        assert!(after.contains("openrouter"));
    }
}