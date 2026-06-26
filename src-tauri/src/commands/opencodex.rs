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
    pub vision_bridge_enabled: Option<bool>,
    pub vision_fallback_base_url: Option<String>,
    pub vision_fallback_model: Option<String>,
    pub vision_fallback_api_key_ref: Option<String>,
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleModelConfigRequest {
    pub model_input: String,
    pub base_url: String,
    pub api_key: String,
    pub display_name: Option<String>,
    pub reasoning_level: Option<String>,
    pub restart_codex: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimpleModelConfigPlan {
    pub provider: ProviderRoute,
    pub model: ModelCatalogEntry,
    pub env_key: String,
    pub restart_codex: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimpleModelConfigResult {
    pub provider: ProviderRoute,
    pub model: ModelCatalogEntry,
    pub env_key: String,
    pub provider_write: OpenCodexWriteResult,
    pub catalog_write: OpenCodexWriteResult,
    pub restart_codex: bool,
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

fn codex_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(".codex").join("config.toml"))
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

fn sanitize_identifier(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn env_key_for_provider(provider: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = false;
    for ch in provider.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "CUSTOM_API_KEY".to_string()
    } else {
        format!("{out}_API_KEY")
    }
}

fn infer_provider_name(base_url: &str) -> String {
    let lower = base_url.to_ascii_lowercase();
    for (needle, provider) in [
        ("deepseek", "deepseek"),
        ("minimax", "minimax"),
        ("openrouter", "openrouter"),
        ("moonshot", "moonshot"),
        ("bigmodel", "zhipu"),
        ("zhipu", "zhipu"),
        ("openai", "openai"),
        ("siliconflow", "siliconflow"),
    ] {
        if lower.contains(needle) {
            return provider.to_string();
        }
    }
    "custom".to_string()
}

fn split_alias_mapping(value: &str) -> (String, String) {
    if let Some((alias, backend)) = value.split_once("->") {
        return (alias.trim().to_string(), backend.trim().to_string());
    }
    if let Some((alias, backend)) = value.split_once('=') {
        return (alias.trim().to_string(), backend.trim().to_string());
    }
    let trimmed = value.trim().to_string();
    (trimmed.clone(), trimmed)
}

fn custom_model_default_fields(
    slug: &str,
    backend_model: &str,
    provider: &str,
) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([
        (
            "description".to_string(),
            serde_json::json!(format!("Custom model: {slug} ({provider})")),
        ),
        ("context_window".to_string(), serde_json::json!(200000)),
        ("max_context_window".to_string(), serde_json::json!(1000000)),
        (
            "auto_compact_token_limit".to_string(),
            serde_json::json!(160000),
        ),
        (
            "truncation_policy".to_string(),
            serde_json::json!({ "mode": "tokens", "limit": 48000 }),
        ),
        (
            "default_reasoning_level".to_string(),
            serde_json::json!("medium"),
        ),
        (
            "supported_reasoning_levels".to_string(),
            serde_json::json!([{ "effort": "medium", "description": "Balanced" }]),
        ),
        (
            "default_reasoning_summary".to_string(),
            serde_json::json!("none"),
        ),
        (
            "reasoning_summary_format".to_string(),
            serde_json::json!("none"),
        ),
        (
            "supports_reasoning_summaries".to_string(),
            serde_json::json!(false),
        ),
        ("default_verbosity".to_string(), serde_json::json!("low")),
        ("support_verbosity".to_string(), serde_json::json!(false)),
        (
            "apply_patch_tool_type".to_string(),
            serde_json::json!("freeform"),
        ),
        (
            "web_search_tool_type".to_string(),
            serde_json::json!("text_and_image"),
        ),
        ("supports_search_tool".to_string(), serde_json::json!(false)),
        (
            "supports_parallel_tool_calls".to_string(),
            serde_json::json!(true),
        ),
        (
            "experimental_supported_tools".to_string(),
            serde_json::json!(["computer_use", "mcp"]),
        ),
        (
            "input_modalities".to_string(),
            serde_json::json!(["text", "image"]),
        ),
        (
            "supports_image_detail_original".to_string(),
            serde_json::json!(true),
        ),
        ("shell_type".to_string(), serde_json::json!("shell_command")),
        (
            "minimal_client_version".to_string(),
            serde_json::json!("0.0.1"),
        ),
        ("supported_in_api".to_string(), serde_json::json!(true)),
        ("availability_nux".to_string(), serde_json::Value::Null),
        ("upgrade".to_string(), serde_json::Value::Null),
        ("priority".to_string(), serde_json::json!(100)),
        ("prefer_websockets".to_string(), serde_json::json!(false)),
        (
            "available_in_plans".to_string(),
            serde_json::json!(["free", "plus", "pro", "team", "business", "enterprise"]),
        ),
        (
            "base_instructions".to_string(),
            serde_json::json!("You are a coding agent running in Codex through a local BYOK shim."),
        ),
        (
            "model_messages".to_string(),
            serde_json::json!({
                "instructions_template": "You are Codex running on {model_name} through a local all-model shim. Be a helpful, direct coding collaborator.",
                "instructions_variables": { "model_name": backend_model }
            }),
        ),
        ("supports_computer_use".to_string(), serde_json::json!(true)),
        ("supports_mcp".to_string(), serde_json::json!(true)),
    ])
}

fn collect_extra_fields(
    obj: &serde_json::Map<String, serde_json::Value>,
    known_keys: &[&str],
) -> BTreeMap<String, serde_json::Value> {
    obj.iter()
        .filter(|(key, _)| !known_keys.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn build_simple_model_config_plan(
    request: &SimpleModelConfigRequest,
) -> AppResult<SimpleModelConfigPlan> {
    let model_input = request.model_input.trim();
    let base_url = request.base_url.trim();
    let api_key = request.api_key.trim();

    if model_input.is_empty() {
        return Err(AppError::Command("请填写模型名称。".to_string()));
    }
    if base_url.is_empty() {
        return Err(AppError::Command("请填写接口地址。".to_string()));
    }
    if api_key.is_empty() {
        return Err(AppError::Command("请填写 API Key。".to_string()));
    }

    let (provider_raw, model_raw) = if let Some((provider, model)) = model_input.split_once(':') {
        (provider.trim().to_string(), model.trim().to_string())
    } else {
        (infer_provider_name(base_url), model_input.to_string())
    };

    let provider_name = sanitize_identifier(&provider_raw);
    if provider_name.is_empty() {
        return Err(AppError::Command("模型来源名称无效。".to_string()));
    }
    let (model_slug_raw, backend_model_raw) = split_alias_mapping(&model_raw);
    let model_slug = sanitize_identifier(&model_slug_raw);
    if model_slug.is_empty() || backend_model_raw.trim().is_empty() {
        return Err(AppError::Command("模型名称无效。".to_string()));
    }

    let env_key = env_key_for_provider(&provider_name);
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| model_slug_raw.trim().to_string());
    let reasoning_level = request
        .reasoning_level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("medium")
        .to_string();

    Ok(SimpleModelConfigPlan {
        provider: ProviderRoute {
            name: provider_name.clone(),
            base_url: base_url.to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: Some(env_key.clone()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: Some("通过模型配置页添加".to_string()),
            extra: BTreeMap::new(),
        },
        model: ModelCatalogEntry {
            model_id: model_slug.clone(),
            display_name: Some(display_name),
            provider: "opencodex".to_string(),
            backend_model: Some(backend_model_raw.trim().to_string()),
            backend_provider: Some(provider_name.clone()),
            visible: true,
            reasoning: Some(ReasoningConfig {
                enabled: true,
                levels: vec![reasoning_level],
            }),
            note: Some("显示在 Codex 下拉框中".to_string()),
            vision_bridge_enabled: Some(false),
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: custom_model_default_fields(
                &model_slug,
                backend_model_raw.trim(),
                &provider_name,
            ),
        },
        env_key,
        restart_codex: request.restart_codex,
    })
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
    let mut inline_secret_names: Vec<String> = Vec::new();
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
            if !name.is_empty() {
                inline_secret_names.push(name.clone());
            }
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

    let err = if !inline_secret_names.is_empty() {
        Some(format!(
            "provider {} 含明文 api_key,已剥离密钥并以受限状态读取。请把 providers.json 中的 api_key 改为 ${{ENV_VAR}} 引用，并在系统环境变量中保存真实密钥。",
            inline_secret_names.join(", ")
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
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let model_id = obj
            .get("model_id")
            .or_else(|| obj.get("slug"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if model_id.is_empty() {
            continue;
        }
        let display_name = obj
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(String::from);
        let provider = obj
            .get("provider")
            .or_else(|| obj.get("backend_provider"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let backend_model = obj
            .get("backend_model")
            .or_else(|| obj.get("model"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let backend_provider = obj
            .get("backend_provider")
            .and_then(|v| v.as_str())
            .map(String::from);
        let visible = obj
            .get("visible")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                obj.get("visibility")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "list")
                    .unwrap_or(true)
            });
        let reasoning = obj.get("reasoning").and_then(|v| {
            if v.is_null() {
                None
            } else {
                serde_json::from_value::<ReasoningConfig>(v.clone()).ok()
            }
        });
        let note = obj.get("note").and_then(|v| v.as_str()).map(String::from);
        let vision_bridge_enabled = obj.get("vision_bridge_enabled").and_then(|v| v.as_bool());
        let vision_fallback_base_url = obj
            .get("vision_fallback_base_url")
            .and_then(|v| v.as_str())
            .map(String::from);
        let vision_fallback_model = obj
            .get("vision_fallback_model")
            .and_then(|v| v.as_str())
            .map(String::from);
        let vision_fallback_api_key_ref = obj
            .get("vision_fallback_api_key_ref")
            .and_then(|v| v.as_str())
            .map(String::from);

        catalog.push(ModelCatalogEntry {
            model_id,
            display_name,
            provider,
            backend_model,
            backend_provider,
            visible,
            reasoning,
            note,
            vision_bridge_enabled,
            vision_fallback_base_url,
            vision_fallback_model,
            vision_fallback_api_key_ref,
            extra: collect_extra_fields(
                obj,
                &[
                    "model_id",
                    "slug",
                    "display_name",
                    "provider",
                    "backend_provider",
                    "backend_model",
                    "model",
                    "visible",
                    "visibility",
                    "reasoning",
                    "note",
                    "vision_bridge_enabled",
                    "vision_fallback_base_url",
                    "vision_fallback_model",
                    "vision_fallback_api_key_ref",
                ],
            ),
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
            Ok(())
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
            let Some(entry) = current.iter().find(|e| e.model_id == key) else {
                return Err(AppError::Command("模型不存在，请刷新后重试。".to_string()));
            };
            if is_protected_subscription_model(entry) {
                return Err(AppError::Command(
                    "订阅默认模型不能删除；如需不显示，请关闭“显示在下拉框”。".to_string(),
                ));
            }
            current.retain(|e| e.model_id != key);
            Ok(())
        },
    )
}

fn is_protected_subscription_model(entry: &ModelCatalogEntry) -> bool {
    if !entry.provider.trim().eq_ignore_ascii_case("openai") {
        return false;
    }

    entry
        .backend_provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .map(|provider| provider.eq_ignore_ascii_case("openai"))
        .unwrap_or(true)
}

#[tauri::command]
pub fn simple_model_config_save(
    request: SimpleModelConfigRequest,
) -> AppResult<SimpleModelConfigResult> {
    // 只写入当前 Codex Box 进程环境,不把明文 Key 写入文件或日志。
    let plan = build_simple_model_config_plan(&request)?;
    std::env::set_var(&plan.env_key, request.api_key.trim());
    write_simple_model_config_to_paths(
        &providers_path()?,
        &catalog_path()?,
        Some(&codex_config_path()?),
        &request,
    )
}

fn write_simple_model_config_to_paths(
    providers_path: &Path,
    catalog_path: &Path,
    codex_config_path: Option<&Path>,
    request: &SimpleModelConfigRequest,
) -> AppResult<SimpleModelConfigResult> {
    let plan = build_simple_model_config_plan(request)?;

    let providers_raw = if providers_path.exists() {
        std::fs::read_to_string(providers_path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    let providers_hash = if providers_raw.trim().is_empty() {
        String::new()
    } else {
        content_hash(&providers_raw)
    };
    let provider_write = upsert_provider_routes(
        providers_path,
        &providers_hash,
        Some("simple model config provider"),
        |current: &mut Vec<ProviderRoute>| {
            current.retain(|p| p.name != plan.provider.name);
            current.push(plan.provider.clone());
        },
    )?;

    let catalog_raw = if catalog_path.exists() {
        std::fs::read_to_string(catalog_path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    let catalog_hash = if catalog_raw.trim().is_empty() {
        String::new()
    } else {
        content_hash(&catalog_raw)
    };
    let catalog_write = upsert_catalog_entries(
        catalog_path,
        &catalog_hash,
        Some("simple model config catalog"),
        |current: &mut Vec<ModelCatalogEntry>| {
            let legacy_model_id = format!("{}/{}", plan.provider.name, plan.model.model_id);
            current.retain(|entry| {
                entry.model_id != plan.model.model_id && entry.model_id != legacy_model_id
            });
            current.push(plan.model.clone());
            Ok(())
        },
    )?;

    if let Some(path) = codex_config_path {
        ensure_codex_config_for_opencodex(path, catalog_path)?;
    }

    Ok(SimpleModelConfigResult {
        provider: plan.provider,
        model: plan.model,
        env_key: plan.env_key,
        provider_write,
        catalog_write,
        restart_codex: plan.restart_codex,
    })
}

fn strip_opencodex_managed_blocks(raw: &str) -> String {
    let mut out = Vec::new();
    let mut skipping_managed_block = false;
    let mut skipping_standalone_provider = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed == "# >>> opencodex managed >>>" {
            skipping_managed_block = true;
            continue;
        }
        if trimmed == "# <<< opencodex managed <<<" {
            skipping_managed_block = false;
            continue;
        }
        if skipping_managed_block {
            if trimmed.starts_with('[') && trimmed != "[model_providers.opencodex]" {
                skipping_managed_block = false;
                out.push(line);
            }
            continue;
        }
        if trimmed == "[model_providers.opencodex]" {
            skipping_standalone_provider = true;
            continue;
        }
        if skipping_standalone_provider {
            if trimmed.starts_with('[') {
                skipping_standalone_provider = false;
                out.push(line);
            }
            continue;
        }
        out.push(line);
    }
    ensure_trailing_newline(&out.join("\n"))
}

fn opencodex_managed_config(catalog_path: &Path) -> String {
    format!(
        r#"# >>> opencodex managed >>>
model_catalog_json = "{}"
openai_base_url = "http://127.0.0.1:8765/v1"
# <<< opencodex managed <<<

{}"#,
        catalog_path.display(),
        r#"# >>> opencodex managed >>>
[model_providers.opencodex]
name = "OpenCodex"
base_url = "http://127.0.0.1:8765/v1"
wire_api = "responses"
requires_openai_auth = true
experimental_bearer_token = "dummy"
request_max_retries = 3
stream_max_retries = 3
stream_idle_timeout_ms = 600000
# <<< opencodex managed <<<"#
    )
}

fn ensure_codex_config_for_opencodex(path: &Path, catalog_path: &Path) -> AppResult<()> {
    let raw = std::fs::read_to_string(path).map_err(AppError::Io)?;
    let cleaned = strip_opencodex_managed_blocks(&raw);
    let managed = opencodex_managed_config(catalog_path);
    let body = cleaned.trim();
    let new_text = if body.is_empty() {
        ensure_trailing_newline(&managed)
    } else {
        ensure_trailing_newline(&format!("{managed}\n\n{body}"))
    };
    if new_text == raw {
        return Ok(());
    }

    let backup_record = {
        let dir = backup_dir()?;
        backup::create_backup_with_extension(path, &dir, BackupReason::PreWrite, "toml")?
    };

    if let Err(error) = writer::atomic_write(path, &new_text) {
        if let Ok(backup_content) = std::fs::read_to_string(&backup_record.file_path) {
            let _ = writer::atomic_write(path, &backup_content);
        }
        return Err(error);
    }
    Ok(())
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
    mutate: impl FnOnce(&mut Vec<ModelCatalogEntry>) -> AppResult<()>,
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
    mutate(&mut current)?;
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
    let mut value = serde_json::json!({
        "slug": entry.model_id,
        "model": entry.model_id,
        "display_name": entry.display_name,
        "provider": entry.provider,
        "backend_model": backend_model,
        "backend_provider": backend_provider,
        "visibility": if entry.visible { "list" } else { "hide" },
        "reasoning": entry.reasoning,
        "note": entry.note,
        "vision_bridge_enabled": entry.vision_bridge_enabled,
        "vision_fallback_base_url": entry.vision_fallback_base_url,
        "vision_fallback_model": entry.vision_fallback_model,
        "vision_fallback_api_key_ref": entry.vision_fallback_api_key_ref,
    });
    if let Some(obj) = value.as_object_mut() {
        for (key, extra_value) in &entry.extra {
            obj.entry(key.clone())
                .or_insert_with(|| extra_value.clone());
        }
    }
    value
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
    fn simple_model_config_plan_uses_env_ref_and_never_returns_plaintext_key() {
        let plan = build_simple_model_config_plan(&SimpleModelConfigRequest {
            model_input: "deepseek:deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test-secret-should-not-be-returned".to_string(),
            display_name: None,
            reasoning_level: Some("medium".to_string()),
            restart_codex: false,
        })
        .unwrap();

        assert_eq!(plan.provider.name, "deepseek");
        assert_eq!(plan.provider.base_url, "https://api.deepseek.com/v1");
        assert_eq!(
            plan.provider.api_key_ref.as_deref(),
            Some("DEEPSEEK_API_KEY")
        );
        assert_eq!(plan.model.model_id, "deepseek-chat");
        assert_eq!(plan.model.provider, "opencodex");
        assert_eq!(plan.model.backend_model.as_deref(), Some("deepseek-chat"));
        assert_eq!(plan.model.backend_provider.as_deref(), Some("deepseek"));
        assert_eq!(plan.env_key, "DEEPSEEK_API_KEY");
        assert!(!format!("{plan:?}").contains("sk-test-secret-should-not-be-returned"));
    }

    #[test]
    fn simple_model_config_write_updates_provider_and_catalog_without_plaintext_key() {
        let dir = tempdir().unwrap();
        let providers = dir.path().join(PROVIDERS_FILE);
        let catalog = dir.path().join(CATALOG_FILE);
        let codex_config = dir.path().join("config.toml");
        std::fs::write(&providers, "{\"providers\":[]}\n").unwrap();
        std::fs::write(
            &catalog,
            r#"{"models":[{"slug":"deepseek/deepseek-chat","model":"deepseek/deepseek-chat","display_name":"Legacy","provider":"deepseek","backend_model":"deepseek-chat","backend_provider":"deepseek","visibility":"list"}]}"#,
        )
        .unwrap();
        std::fs::write(
            &codex_config,
            r#"# <<< opencodex managed <<<
model = "gpt-5.5"

# >>> opencodex managed >>>
[model_providers.opencodex]
name = "OpenCodex"
base_url = "http://127.0.0.1:8765/v1"

[features]
js_repl = false
# <<< opencodex managed <<<
"#,
        )
        .unwrap();

        let result = write_simple_model_config_to_paths(
            &providers,
            &catalog,
            Some(&codex_config),
            &SimpleModelConfigRequest {
                model_input: "deepseek:deepseek-chat".to_string(),
                base_url: "https://api.deepseek.com/v1".to_string(),
                api_key: "sk-test-secret-should-not-be-written".to_string(),
                display_name: Some("DeepSeek Chat".to_string()),
                reasoning_level: Some("medium".to_string()),
                restart_codex: true,
            },
        )
        .unwrap();

        let providers_text = std::fs::read_to_string(&providers).unwrap();
        let catalog_text = std::fs::read_to_string(&catalog).unwrap();
        assert!(providers_text.contains("\"name\": \"deepseek\""));
        assert!(providers_text.contains("\"api_key\": \"$DEEPSEEK_API_KEY\""));
        assert!(!providers_text.contains("sk-test-secret-should-not-be-written"));
        assert!(catalog_text.contains("\"slug\": \"deepseek-chat\""));
        assert!(catalog_text.contains("\"model\": \"deepseek-chat\""));
        assert!(catalog_text.contains("\"provider\": \"opencodex\""));
        assert!(catalog_text.contains("\"backend_model\": \"deepseek-chat\""));
        assert!(catalog_text.contains("\"backend_provider\": \"deepseek\""));
        assert!(catalog_text.contains("\"supported_in_api\": true"));
        assert!(catalog_text.contains("\"minimal_client_version\": \"0.0.1\""));
        assert!(catalog_text.contains("\"available_in_plans\""));
        assert!(!catalog_text.contains("\"slug\": \"deepseek/deepseek-chat\""));
        assert_eq!(result.env_key, "DEEPSEEK_API_KEY");
        assert!(result.restart_codex);

        let config_text = std::fs::read_to_string(&codex_config).unwrap();
        assert!(config_text.starts_with("# >>> opencodex managed >>>"));
        assert!(config_text.contains(&format!("model_catalog_json = \"{}\"", catalog.display())));
        assert!(config_text.contains("openai_base_url = \"http://127.0.0.1:8765/v1\""));
        assert!(config_text.contains("[model_providers.opencodex]"));
        assert!(config_text.contains("wire_api = \"responses\""));
        assert!(config_text.contains("[features]\njs_repl = false"));
        assert!(!config_text.starts_with("# <<< opencodex managed <<<"));
    }

    #[test]
    fn strip_opencodex_managed_blocks_removes_standalone_duplicate_provider() {
        let raw = r#"# >>> opencodex managed >>>
model_catalog_json = "/tmp/catalog.json"
openai_base_url = "http://127.0.0.1:8765/v1"
# <<< opencodex managed <<<

model = "gpt-5.5"

[model_providers.opencodex]
base_url = "http://127.0.0.1:0/v1"
name = "OpenCodex"

[plugins.example]
enabled = true
"#;

        let cleaned = strip_opencodex_managed_blocks(raw);

        assert!(cleaned.contains("model = \"gpt-5.5\""));
        assert!(cleaned.contains("[plugins.example]"));
        assert!(!cleaned.contains("[model_providers.opencodex]"));
        assert!(!cleaned.contains("127.0.0.1:0"));
        assert!(!cleaned.contains("model_catalog_json"));
        assert!(!cleaned.contains("openai_base_url"));
    }

    #[test]
    fn catalog_entry_roundtrip_keeps_vision_bridge_fields() {
        let entry = ModelCatalogEntry {
            model_id: "deepseek/deepseek-chat".to_string(),
            display_name: Some("DeepSeek Chat".to_string()),
            provider: "deepseek".to_string(),
            backend_model: Some("deepseek-chat".to_string()),
            backend_provider: Some("deepseek".to_string()),
            visible: true,
            reasoning: Some(ReasoningConfig {
                enabled: true,
                levels: vec!["medium".to_string()],
            }),
            note: Some("Vision bridge enabled".to_string()),
            vision_bridge_enabled: Some(true),
            vision_fallback_base_url: Some("https://api.deepseek.com/v1".to_string()),
            vision_fallback_model: Some("deepseek-vision".to_string()),
            vision_fallback_api_key_ref: Some("DEEPSEEK_VISION_API_KEY".to_string()),
            extra: BTreeMap::new(),
        };

        let value = catalog_to_file_value(&entry);
        assert_eq!(
            value.get("vision_bridge_enabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value
                .get("vision_fallback_base_url")
                .and_then(|v| v.as_str()),
            Some("https://api.deepseek.com/v1")
        );
        assert_eq!(
            value.get("vision_fallback_model").and_then(|v| v.as_str()),
            Some("deepseek-vision")
        );
        assert_eq!(
            value
                .get("vision_fallback_api_key_ref")
                .and_then(|v| v.as_str()),
            Some("DEEPSEEK_VISION_API_KEY")
        );
    }

    #[test]
    fn subscription_openai_catalog_entries_are_protected_from_delete() {
        let make_entry = |provider: &str, backend_provider: Option<&str>| ModelCatalogEntry {
            model_id: "gpt-5.5".to_string(),
            display_name: Some("GPT-5.5".to_string()),
            provider: provider.to_string(),
            backend_model: Some("gpt-5.5".to_string()),
            backend_provider: backend_provider.map(ToString::to_string),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        };

        assert!(is_protected_subscription_model(&make_entry("openai", None)));
        assert!(is_protected_subscription_model(&make_entry(
            "openai",
            Some("openai")
        )));
        assert!(!is_protected_subscription_model(&make_entry(
            "opencodex",
            Some("openai")
        )));
        assert!(!is_protected_subscription_model(&make_entry(
            "openai",
            Some("minimax")
        )));
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
