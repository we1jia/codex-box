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
    pub backend_model: Option<String>,
    pub backend_provider: Option<String>,
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

    let (providers, raw_providers_text, providers_err) = read_providers(&providers_path);
    let (catalog, raw_catalog_text, catalog_err) = read_catalog(&catalog_path);

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
        valid: parse_errors.is_empty(),
        parse_errors,
    })
}

/// 读取 ~/.opencodex/providers.json
/// 严格策略: 明文 api_key 全部拒绝入库,记入 parse_errors 提示用户修复
fn read_providers(path: &Path) -> (Vec<ProviderRoute>, String, Option<String>) {
    if !path.exists() {
        let raw = "[]".to_string();
        return (Vec::new(), raw, None);
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return (Vec::new(), String::new(), Some(format!("read failed: {e}"))),
    };
    if raw.trim().is_empty() {
        return (Vec::new(), raw, None);
    }
    // 先解析为通用 Value,再做 envelope/明文 secret 清理
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return (Vec::new(), raw, Some(format!("parse failed: {e}")));
        }
    };

    let array_value = if let Some(obj) = value.as_object() {
        obj.get("providers")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]))
    } else {
        value
    };
    let arr = match array_value.as_array() {
        Some(a) => a,
        None => {
            return (
                Vec::new(),
                raw,
                Some("expected an array (or {providers: [...]})".to_string()),
            );
        }
    };

    let mut providers = Vec::new();
    let mut inline_secret_count = 0usize;
    for entry in arr {
        // 从 AITabby 原始 entry 抽出字段
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let base_url = entry
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let wire_api = entry
            .get("wire_api")
            .and_then(|v| v.as_str())
            .unwrap_or("chat")
            .to_string();
        let http_headers: BTreeMap<String, String> = entry
            .get("http_headers")
            .and_then(|v| v.as_object())
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        // api_key 字段: AITabby 支持 "$ENV_VAR"; Codex Box 也兼容 "${ENV_VAR}"。
        // 其他形态按明文 secret 处理,不会进入内存模型。
        let raw_api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        let api_key_ref = if raw_api_key.is_empty() {
            None
        } else if (raw_api_key.starts_with("${") && raw_api_key.ends_with('}'))
            || raw_api_key.starts_with('$')
        {
            Some(raw_api_key.to_string())
        } else {
            // 明文 secret, 拒绝入库
            inline_secret_count += 1;
            None
        };

        if name.is_empty() {
            continue;
        }
        providers.push(ProviderRoute {
            name,
            base_url,
            wire_api,
            api_key_ref,
            http_headers,
            enabled: true,
            note: None,
            extra: BTreeMap::new(),
        });
    }

    let err = if inline_secret_count > 0 {
        Some(format!(
            "{} provider 含明文 api_key,已拒绝入库。请改为 ${{ENV_VAR}} 引用或在 Codex Box Provider Routes 页编辑。",
            inline_secret_count
        ))
    } else {
        None
    };

    (providers, raw, err)
}

/// 读取 ~/.opencodex/custom_model_catalog.json
fn read_catalog(path: &Path) -> (Vec<ModelCatalogEntry>, String, Option<String>) {
    if !path.exists() {
        let raw = "[]".to_string();
        return (Vec::new(), raw, None);
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return (Vec::new(), String::new(), Some(format!("read failed: {e}"))),
    };
    if raw.trim().is_empty() {
        return (Vec::new(), raw, None);
    }
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => return (Vec::new(), raw, Some(format!("parse failed: {e}"))),
    };
    let array_value = if let Some(obj) = value.as_object() {
        obj.get("models")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]))
    } else {
        value
    };
    let arr = match array_value.as_array() {
        Some(a) => a,
        None => {
            return (
                Vec::new(),
                raw,
                Some("expected an array (or {models: [...]})".to_string()),
            );
        }
    };

    let mut catalog = Vec::new();
    for entry in arr {
        let model_id = entry
            .get("model_id")
            .or_else(|| entry.get("slug"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if model_id.is_empty() {
            continue;
        }
        let display_name = entry
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(String::from);
        let provider = entry
            .get("provider")
            .or_else(|| entry.get("backend_provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let backend_model = entry
            .get("backend_model")
            .or_else(|| entry.get("model"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let backend_provider = entry
            .get("backend_provider")
            .and_then(|v| v.as_str())
            .map(String::from);
        let visible = entry
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                entry
                    .get("visibility")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "list")
                    .unwrap_or(true)
            });
        let reasoning = entry.get("reasoning").and_then(|v| {
            if v.is_null() {
                None
            } else {
                serde_json::from_value::<ReasoningConfig>(v.clone()).ok()
            }
        });
        let note = entry.get("note").and_then(|v| v.as_str()).map(String::from);

        catalog.push(ModelCatalogEntry {
            model_id,
            display_name,
            provider,
            backend_model,
            backend_provider,
            visible,
            reasoning,
            note,
            extra: BTreeMap::new(),
        });
    }

    (catalog, raw, None)
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
    upsert_provider_routes(
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
pub fn provider_route_delete(request: OpenCodexDeleteRequest) -> AppResult<OpenCodexWriteResult> {
    let key = request.key.clone();
    if key.trim().is_empty() {
        return Err(AppError::Command("provider name 不能为空".to_string()));
    }
    upsert_provider_routes(
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
    upsert_catalog_entries(
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
pub fn catalog_entry_delete(request: OpenCodexDeleteRequest) -> AppResult<OpenCodexWriteResult> {
    let key = request.key.clone();
    if key.trim().is_empty() {
        return Err(AppError::Command("model_id 不能为空".to_string()));
    }
    upsert_catalog_entries(
        &catalog_path()?,
        &request.expected_hash,
        request.note.as_deref(),
        |current: &mut Vec<ModelCatalogEntry>| {
            current.retain(|e| e.model_id != key);
        },
    )
}

fn upsert_provider_routes(
    path: &Path,
    expected_hash: &str,
    _note: Option<&str>,
    mutate: impl FnOnce(&mut Vec<ProviderRoute>),
) -> AppResult<OpenCodexWriteResult> {
    let raw = if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    if !raw.trim().is_empty() {
        let actual_hash = content_hash(&raw);
        if actual_hash != expected_hash {
            return Err(AppError::Command(
                "配置文件已变化，请重新读取后再写入。".to_string(),
            ));
        }
    }

    let (mut current, _, err) = read_providers(path);
    if let Some(message) = err {
        if message.starts_with("parse failed") || message.starts_with("expected") {
            return Err(AppError::Command(format!(
                "refusing to overwrite unparseable file: {message}"
            )));
        }
    }
    mutate(&mut current);
    let providers: Vec<serde_json::Value> = current.iter().map(provider_to_file_value).collect();
    write_json_envelope(path, "providers", providers)
}

fn upsert_catalog_entries(
    path: &Path,
    expected_hash: &str,
    _note: Option<&str>,
    mutate: impl FnOnce(&mut Vec<ModelCatalogEntry>),
) -> AppResult<OpenCodexWriteResult> {
    let raw = if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    if !raw.trim().is_empty() {
        let actual_hash = content_hash(&raw);
        if actual_hash != expected_hash {
            return Err(AppError::Command(
                "配置文件已变化，请重新读取后再写入。".to_string(),
            ));
        }
    }

    let (mut current, _, err) = read_catalog(path);
    if let Some(message) = err {
        return Err(AppError::Command(format!(
            "refusing to overwrite unparseable file: {message}"
        )));
    }
    mutate(&mut current);
    let models: Vec<serde_json::Value> = current.iter().map(catalog_to_file_value).collect();
    write_json_envelope(path, "models", models)
}

fn provider_to_file_value(route: &ProviderRoute) -> serde_json::Value {
    let api_key = route
        .api_key_ref
        .as_deref()
        .map(to_aitabby_env_ref)
        .unwrap_or_default();
    serde_json::json!({
        "name": route.name,
        "base_url": route.base_url,
        "api_key": api_key,
        "wire_api": route.wire_api,
        "http_headers": route.http_headers,
    })
}

fn catalog_to_file_value(entry: &ModelCatalogEntry) -> serde_json::Value {
    let backend_provider = entry
        .backend_provider
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&entry.provider);
    let backend_model = entry
        .backend_model
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&entry.model_id);
    serde_json::json!({
        "slug": entry.model_id,
        "model": entry.model_id,
        "display_name": entry.display_name,
        "provider": entry.provider,
        "backend_model": backend_model,
        "backend_provider": backend_provider,
        "visibility": if entry.visible { "list" } else { "hide" },
        "reasoning": entry.reasoning,
        "note": entry.note,
    })
}

fn to_aitabby_env_ref(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') {
        format!("${}", &trimmed[2..trimmed.len() - 1])
    } else if trimmed.starts_with('$') {
        trimmed.to_string()
    } else {
        format!("${trimmed}")
    }
}

fn write_json_envelope(
    path: &Path,
    key: &str,
    items: Vec<serde_json::Value>,
) -> AppResult<OpenCodexWriteResult> {
    let envelope = serde_json::json!({ key: items });
    let new_text = serde_json::to_string_pretty(&envelope)
        .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?;
    let new_text = ensure_trailing_newline(&new_text);

    let backup_record = if path.exists() {
        let dir = backup_dir()?;
        Some(backup::create_backup_with_extension(
            path,
            &dir,
            BackupReason::PreWrite,
            "json",
        )?)
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

    Ok(OpenCodexWriteResult {
        file_path: path.display().to_string(),
        backup_id: backup_record.map(|r| r.id).unwrap_or_default(),
        new_hash: content_hash(&new_text),
    })
}

/// 支持 envelope schema 的 upsert
///   envelope_key = None  -> 顶层直接是 Vec<T>
///   envelope_key = Some(k) -> 顶层是 { k: Vec<T> }
fn upsert_with_envelope<T>(
    path: &Path,
    expected_hash: &str,
    _note: Option<&str>,
    envelope_key: Option<&str>,
    mutate: impl FnOnce(&mut T),
) -> AppResult<OpenCodexWriteResult>
where
    T: Default + serde::de::DeserializeOwned + Serialize,
{
    // 读 raw, 支持 envelope / 顶层数组两种格式
    let raw = if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)?
    } else {
        String::new()
    };

    if !raw.trim().is_empty() {
        let actual_hash = content_hash(&raw);
        if actual_hash != expected_hash {
            return Err(AppError::Command(
                "配置文件已变化，请重新读取后再写入。".to_string(),
            ));
        }
    }

    // 解析 vec
    let mut current: T = if raw.trim().is_empty() {
        T::default()
    } else {
        match extract_vec::<T>(&raw, envelope_key) {
            Ok(v) => v,
            Err(e) => {
                return Err(AppError::Command(format!(
                    "refusing to overwrite unparseable file: {e}"
                )));
            }
        }
    };

    mutate(&mut current);

    // 序列化: envelope 时包一层
    let new_text = match envelope_key {
        Some(k) => {
            let inner = serde_json::to_value(&current)
                .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?;
            let envelope = serde_json::json!({ k: inner });
            serde_json::to_string_pretty(&envelope)
                .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?
        }
        None => serde_json::to_string_pretty(&current)
            .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?,
    };
    let new_text = ensure_trailing_newline(&new_text);

    let backup_record = if path.exists() {
        let dir = backup_dir()?;
        Some(backup::create_backup_with_extension(
            path,
            &dir,
            BackupReason::PreWrite,
            "json",
        )?)
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

/// 从 raw JSON 中抽取 vec, 支持 envelope / 顶层数组两种格式
fn extract_vec<T>(raw: &str, envelope_key: Option<&str>) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("parse failed: {e}"))?;
    if let Some(key) = envelope_key {
        if let Some(obj) = value.as_object() {
            if let Some(inner) = obj.get(key) {
                return serde_json::from_value::<T>(inner.clone())
                    .map_err(|e| format!("parse failed: {e}"));
            }
        }
        // envelope_key 指定了但 raw 不是 envelope, 返回默认值
        return serde_json::from_value::<T>(serde_json::Value::Array(vec![]))
            .map_err(|e| format!("parse failed: {e}"));
    }
    serde_json::from_value::<T>(value).map_err(|e| format!("parse failed: {e}"))
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
        assert!(looks_like_secret(
            route.api_key_ref.as_deref().unwrap_or("")
        ));

        route.api_key_ref = Some("OPENROUTER_API_KEY".to_string());
        assert!(!looks_like_secret(
            route.api_key_ref.as_deref().unwrap_or("")
        ));
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

    #[test]
    fn extract_vec_handles_envelope_schema() {
        let raw = r#"{"providers":[{"name":"x","baseUrl":"https://x","wireApi":"chat","apiKeyRef":null,"httpHeaders":{},"enabled":true,"note":null}]}"#;
        let v: Vec<ProviderRoute> = extract_vec(raw, Some("providers")).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "x");
    }

    #[test]
    fn extract_vec_handles_top_level_array() {
        let raw = r#"[{"name":"y","baseUrl":"https://y","wireApi":"chat","apiKeyRef":null,"httpHeaders":{},"enabled":true,"note":null}]"#;
        let v: Vec<ProviderRoute> = extract_vec(raw, None).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "y");
    }

    #[test]
    fn read_providers_rejects_inline_secrets_but_keeps_valid() {
        // read_providers 走 JSON Value 自定义解析, 直接读 "base_url"/"api_key" 这些 AITabby 原生 key
        let raw = r#"{
            "providers": [
                { "name": "good", "base_url": "https://a", "api_key": "${ENV_KEY}" },
                { "name": "bad", "base_url": "https://b", "api_key": "sk-cp-bagtpizhVEyY6MvdK8q4ZFfXEN00GwJo5lbSI2cCb99rH4XlzNtrT9rALNayi6IbA80MYxgT2myt3NkCyYFUBtoyNYc9ZSVWKUHbRZwNccf_fnGhX_hLOXg" }
            ]
        }"#;
        let dir = tempdir().unwrap();
        let file = dir.path().join(PROVIDERS_FILE);
        std::fs::write(&file, raw).unwrap();
        let (providers, _raw, err) = read_providers(&file);
        assert_eq!(providers.len(), 2);
        assert!(providers
            .iter()
            .any(|p| p.name == "good" && p.api_key_ref.as_deref() == Some("${ENV_KEY}")));
        // bad provider: 入库但 api_key_ref = None(明文 secret 被剥离)
        let bad = providers.iter().find(|p| p.name == "bad").unwrap();
        assert!(bad.api_key_ref.is_none());
        assert!(err.is_some(), "应记录明文 secret 拒绝警告");
        assert!(err.unwrap().contains("明文"));
    }

    #[test]
    fn upsert_with_envelope_writes_envelope_schema() {
        let dir = tempdir().unwrap();
        let file = dir.path().join(PROVIDERS_FILE);
        std::fs::write(
            &file,
            r#"{"providers":[{"name":"old","baseUrl":"https://old","wireApi":"chat","apiKeyRef":null,"httpHeaders":{},"enabled":true,"note":null}]}"#,
        )
        .unwrap();
        let raw_before = std::fs::read_to_string(&file).unwrap();
        let hash_before = content_hash(&raw_before);

        let route = ProviderRoute {
            name: "new".to_string(),
            base_url: "https://new".to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: Some("NEW_KEY".to_string()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            extra: BTreeMap::new(),
        };
        let result = upsert_with_envelope::<Vec<ProviderRoute>>(
            &file,
            &hash_before,
            Some("test"),
            Some("providers"),
            |current: &mut Vec<ProviderRoute>| {
                current.retain(|p| p.name != "new");
                current.push(route.clone());
            },
        )
        .unwrap();
        assert!(!result.backup_id.is_empty());

        let after = std::fs::read_to_string(&file).unwrap();
        assert!(after.starts_with("{"));
        assert!(after.contains("\"providers\""));
        assert!(after.contains("\"old\""));
        assert!(after.contains("\"new\""));
    }
}
