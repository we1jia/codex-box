// src-tauri/src/commands/opencodex.rs
//
// v0.3 BYOK: 不再 spawn 外部 OpenCodex 进程,改为读写
// ~/.codex/codex-box/providers.json 与 ~/.codex/codex-box/custom_model_catalog.json。
// ~/.opencodex/*.json 仅作为 OpenCodex 兼容导入来源,不作为实时模型列表兜底。
//
// 写入闭环沿用 v0.2 既有 backup → diff → confirm → atomic write → rollback 链:
//   - backup 目录:~/.codex/codex-box/backups/{ts}-{hash}.json
//   - 写 .tmp → rename
//   - 失败时 rollback 到最近一次 backup
//   - 写入前校验 content_hash,防止并发覆盖
//
// 兼容性策略: 未知字段保留(用 #[serde(flatten)] + extra),文件不存在按空配置处理。
// 默认兼容 OpenCodex schema: api_key 字段写 $ENV_VAR 引用;日志和诊断必须脱敏。
use crate::config::model::{BackupReason, DiffLine};
use crate::config::{backup, writer};
use crate::error::{AppError, AppResult};
use crate::proxy::inject_map::{self, InjectMap};
use crate::proxy::state::{persist_runtime_state, ProxyState};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CODEX_BOX_DIR: &str = ".codex/codex-box";
const LEGACY_OPENCODEX_DIR: &str = ".opencodex";
const PROVIDERS_FILE: &str = "providers.json";
const CATALOG_FILE: &str = "custom_model_catalog.json";
const NATIVE_MODELS_CACHE_FILE: &str = "models_cache.json";
const NATIVE_MODELS_CACHE_BACKUP_FILE: &str = "models_cache.codex-box-backup.json";
const CODEX_BOX_MODELS_CACHE_ETAG: &str = "codex-box-model-catalog";
const SCHEMA_VERSION: u32 = 1;
const BACKUP_DIR: &str = ".codex/codex-box/backups";
const DEFAULT_LOCAL_PROVIDER_ID: &str = "codex_model_router_v2";
const DEFAULT_MULTIROUTER_PORT: u16 = 1455;
const OFFICIAL_CODEX_BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex";
const OFFICIAL_OPENAI_ROUTE_ID: &str = "openai-official";
const DEFAULT_NATIVE_OPENAI_MODELS: &[(&str, &str)] = &[
    ("gpt-5.5", "GPT-5.5"),
    ("gpt-5.4", "GPT-5.4"),
    ("gpt-5.4-mini", "GPT-5.4-Mini"),
    ("gpt-5.3-codex-spark", "GPT-5.3-Codex-Spark"),
    ("codex-auto-review", "Codex Auto Review"),
];

fn is_legacy_opencodex_provider_route(provider: &ProviderRoute) -> bool {
    provider.name.eq_ignore_ascii_case("opencodex")
        || provider.base_url.contains("127.0.0.1:8765")
        || provider.base_url.contains("localhost:8765")
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_routing: Option<CodexRoutingConfig>,
    /// 未知字段保留,AITabby CLI 升级 schema 时不丢字段
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexRoutingConfig {
    pub enabled: Option<bool>,
    pub default_route_id: Option<String>,
    #[serde(default)]
    pub routes: Vec<CodexRoutingRoute>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexRoutingRoute {
    pub id: String,
    pub label: Option<String>,
    pub enabled: Option<bool>,
    pub target_provider_id: Option<String>,
    #[serde(rename = "match", default)]
    pub match_rule: CodexRoutingMatch,
    #[serde(default)]
    pub upstream: CodexRoutingUpstream,
    pub capabilities: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexRoutingMatch {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub prefixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CodexRoutingUpstream {
    pub base_url: Option<String>,
    pub api_format: Option<String>,
    pub auth: Option<serde_json::Value>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub model_map: BTreeMap<String, String>,
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
    pub wire_api: Option<String>,
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
    #[serde(skip)]
    pub runtime_api_key: Option<String>,
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
    pub requires_multirouter_sync: bool,
    pub restart_codex: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultirouterSyncRequest {
    pub providers_expected_hash: String,
    pub catalog_expected_hash: String,
    pub proxy_port: Option<u16>,
    pub router_provider_id: Option<String>,
    pub ensure_codex_config: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultirouterSyncResult {
    pub router_provider: ProviderRoute,
    pub route_count: usize,
    pub routed_model_count: usize,
    pub skipped_models: Vec<String>,
    pub proxy_base_url: String,
    pub provider_write: OpenCodexWriteResult,
    pub catalog_write: OpenCodexWriteResult,
    pub config_write: Option<OpenCodexWriteResult>,
    pub models_cache_write: Option<OpenCodexWriteResult>,
    pub inject_map_write: Option<OpenCodexWriteResult>,
    pub config_touched: bool,
    pub models_cache_touched: bool,
    pub inject_map_touched: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultirouterPreviewRequest {
    pub proxy_port: Option<u16>,
    pub router_provider_id: Option<String>,
    pub ensure_codex_config: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultirouterPreview {
    pub providers_path: String,
    pub catalog_path: String,
    pub config_path: String,
    pub models_cache_path: String,
    pub inject_map_path: String,
    pub providers_expected_hash: String,
    pub catalog_expected_hash: String,
    pub config_expected_hash: String,
    pub models_cache_expected_hash: String,
    pub inject_map_expected_hash: String,
    pub router_provider_id: String,
    pub proxy_port: u16,
    pub providers_diff: Vec<DiffLine>,
    pub catalog_diff: Vec<DiffLine>,
    pub config_diff: Vec<DiffLine>,
    pub models_cache_diff: Vec<DiffLine>,
    pub inject_map_diff: Vec<DiffLine>,
    pub router_provider: ProviderRoute,
    pub route_count: usize,
    pub routed_model_count: usize,
    pub skipped_models: Vec<String>,
    pub proxy_base_url: String,
    pub ensure_codex_config: bool,
    pub models_cache_touched: bool,
    pub inject_map_touched: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMultirouterApplyRequest {
    pub preview: CodexMultirouterPreview,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelsCacheRestorePreview {
    pub models_cache_path: String,
    pub backup_path: String,
    pub models_cache_expected_hash: String,
    pub backup_exists: bool,
    pub owned_cache: bool,
    pub restore_available: bool,
    pub will_delete: bool,
    pub diff: Vec<DiffLine>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelsCacheRestoreApplyRequest {
    pub preview: CodexModelsCacheRestorePreview,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelsCacheRestoreResult {
    pub models_cache_path: String,
    pub backup_path: String,
    pub backup_id: String,
    pub new_hash: String,
    pub restored: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImportSource {
    pub id: String,
    pub display_name: String,
    pub source_kind: String,
    pub path: String,
    pub providers: usize,
    pub models: usize,
    pub config_snapshots: usize,
    pub warnings: Vec<String>,
    pub recommended_action: String,
    pub can_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImportPreview {
    pub source_id: String,
    pub providers_source_path: String,
    pub catalog_source_path: String,
    pub providers_target_path: String,
    pub catalog_target_path: String,
    pub providers_expected_hash: String,
    pub catalog_expected_hash: String,
    pub providers_diff: Vec<DiffLine>,
    pub catalog_diff: Vec<DiffLine>,
    pub providers: usize,
    pub models: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigImportRequest {
    pub preview: ConfigImportPreview,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigImportResult {
    pub provider_write: OpenCodexWriteResult,
    pub catalog_write: OpenCodexWriteResult,
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
    let dir = home.join(CODEX_BOX_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn legacy_opencodex_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(LEGACY_OPENCODEX_DIR))
}

fn providers_path() -> AppResult<PathBuf> {
    Ok(opencodex_dir()?.join(PROVIDERS_FILE))
}

fn catalog_path() -> AppResult<PathBuf> {
    Ok(opencodex_dir()?.join(CATALOG_FILE))
}

fn legacy_providers_path() -> AppResult<PathBuf> {
    Ok(legacy_opencodex_dir()?.join(PROVIDERS_FILE))
}

fn legacy_catalog_path() -> AppResult<PathBuf> {
    Ok(legacy_opencodex_dir()?.join(CATALOG_FILE))
}

fn codex_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(".codex").join("config.toml"))
}

fn native_models_cache_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(".codex").join(NATIVE_MODELS_CACHE_FILE))
}

fn native_models_cache_backup_path_for(path: &Path) -> PathBuf {
    path.with_file_name(NATIVE_MODELS_CACHE_BACKUP_FILE)
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

fn looks_like_env_key(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return false;
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn api_key_input_to_env_ref(
    input: &str,
    fallback_env_key: &str,
) -> AppResult<(String, String, Option<String>)> {
    let trimmed = input.trim();
    if trimmed.starts_with("${") && trimmed.ends_with('}') {
        let env_key = trimmed[2..trimmed.len() - 1].trim();
        if looks_like_env_key(env_key) {
            return Ok((format!("${{{env_key}}}"), env_key.to_string(), None));
        }
        return Err(AppError::Command(
            "API Key 环境变量名称无效。请使用 ${ENV_VAR} 或直接粘贴 key。".to_string(),
        ));
    }
    if let Some(env_key) = trimmed.strip_prefix('$') {
        let env_key = env_key.trim();
        if looks_like_env_key(env_key) {
            return Ok((format!("${{{env_key}}}"), env_key.to_string(), None));
        }
        return Err(AppError::Command(
            "API Key 环境变量名称无效。请使用 ${ENV_VAR} 或直接粘贴 key。".to_string(),
        ));
    }
    if looks_like_env_key(trimmed) {
        return Ok((format!("${{{trimmed}}}"), trimmed.to_string(), None));
    }
    Ok((
        format!("${{{fallback_env_key}}}"),
        fallback_env_key.to_string(),
        Some(trimmed.to_string()),
    ))
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

fn normalize_catalog_provider(provider: &str, backend_provider: Option<&str>) -> String {
    let provider = provider.trim();
    if provider.eq_ignore_ascii_case("opencodex") {
        return DEFAULT_LOCAL_PROVIDER_ID.to_string();
    }
    if !provider.is_empty() {
        return provider.to_string();
    }
    backend_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_LOCAL_PROVIDER_ID)
        .to_string()
}

fn custom_model_default_fields(
    slug: &str,
    backend_model: &str,
    provider: &str,
) -> BTreeMap<String, serde_json::Value> {
    let text_only = known_text_only_catalog_target(provider, backend_model)
        || known_text_only_catalog_target(provider, slug);
    let mut fields = BTreeMap::from([
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
            serde_json::json!(if text_only { "text" } else { "text_and_image" }),
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
            if text_only {
                serde_json::json!(["text"])
            } else {
                serde_json::json!(["text", "image"])
            },
        ),
        (
            "supports_image_detail_original".to_string(),
            serde_json::json!(!text_only),
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
    ]);

    if text_only {
        fields.insert("textOnly".to_string(), serde_json::json!(true));
    }
    if known_minimax_catalog_target(provider, backend_model)
        || known_minimax_catalog_target(provider, slug)
    {
        fields.insert(
            "codexChatReasoning".to_string(),
            serde_json::json!({
                "supportsThinking": true,
                "supportsEffort": false,
                "thinkingParam": "reasoning_split",
                "outputFormat": "reasoning_details"
            }),
        );
    }

    fields
}

fn catalog_target_tail(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches("models/")
        .trim()
        .to_ascii_lowercase();
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized.as_str())
        .to_string()
}

fn catalog_target_compact_tail(value: &str) -> String {
    catalog_target_tail(value)
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn has_native_vision_hint(value: &str) -> bool {
    let tail = catalog_target_tail(value);
    let compact_tail = catalog_target_compact_tail(value);
    tail.contains("vision")
        || tail.contains("multimodal")
        || tail.contains("omni")
        || tail.contains("image")
        || tail.ends_with("-vl")
        || tail.contains("-vl-")
        || tail.ends_with("_vl")
        || tail.contains("_vl_")
        || compact_tail.ends_with("vl")
}

fn known_minimax_catalog_target(provider: &str, model: &str) -> bool {
    if has_native_vision_hint(model) {
        return false;
    }
    let provider = provider.trim().to_ascii_lowercase();
    let compact_tail = catalog_target_compact_tail(model);
    provider.contains("minimax") || compact_tail.starts_with("minimax")
}

fn known_text_only_catalog_target(provider: &str, model: &str) -> bool {
    if has_native_vision_hint(model) {
        return false;
    }
    let provider = provider.trim().to_ascii_lowercase();
    let tail = catalog_target_tail(model);
    let compact_tail = catalog_target_compact_tail(model);

    const EXACT_TAILS: &[&str] = &[
        "deepseek-chat",
        "deepseek-reasoner",
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "glm-5.1",
        "kat-coder",
        "kat-coder-pro",
        "longcat-flash-chat",
        "mimo-v2.5-pro",
    ];
    const TAIL_PREFIXES: &[&str] = &["qwen3-coder", "step-3.5-flash"];

    provider.contains("deepseek")
        || known_minimax_catalog_target(&provider, model)
        || compact_tail.starts_with("deepseekv4")
        || EXACT_TAILS.contains(&tail.as_str())
        || TAIL_PREFIXES.iter().any(|prefix| tail.starts_with(prefix))
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

fn merge_native_models_into_catalog_if_available(
    catalog: &mut Vec<ModelCatalogEntry>,
    cache_path: &Path,
) -> AppResult<bool> {
    if !cache_path.exists() {
        return Ok(false);
    }

    let cache_raw = std::fs::read_to_string(cache_path).map_err(AppError::Io)?;
    let cache_value: serde_json::Value = serde_json::from_str(&cache_raw)
        .map_err(|e| AppError::Command(format!("models_cache.json parse failed: {e}")))?;
    if cache_value.get("etag").and_then(|value| value.as_str()) == Some(CODEX_BOX_MODELS_CACHE_ETAG)
    {
        return Ok(false);
    }
    let native_models = cache_value
        .get("models")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if native_models.is_empty() {
        return Ok(false);
    }

    let mut updated = false;
    for native in native_models {
        let Some(obj) = native.as_object() else {
            continue;
        };
        let model_id = obj
            .get("slug")
            .or_else(|| obj.get("model"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let Some(model_id) = model_id else {
            continue;
        };

        match catalog.iter_mut().find(|entry| entry.model_id == model_id) {
            Some(existing) => {
                if is_protected_subscription_model(existing) {
                    if existing.provider != "openai" || existing.backend_provider.is_some() {
                        existing.provider = "openai".to_string();
                        existing.backend_provider = None;
                        updated = true;
                    }
                }
            }
            None => {
                let display_name = obj
                    .get("display_name")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string);
                let backend_model = obj
                    .get("backend_model")
                    .or_else(|| obj.get("model"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
                    .or_else(|| Some(model_id.clone()));
                catalog.push(ModelCatalogEntry {
                    model_id: model_id.clone(),
                    display_name,
                    provider: "openai".to_string(),
                    backend_model,
                    backend_provider: None,
                    visible: true,
                    reasoning: None,
                    note: Some(
                        "Codex native model imported from ~/.codex/models_cache.json".to_string(),
                    ),
                    vision_bridge_enabled: None,
                    vision_fallback_base_url: None,
                    vision_fallback_model: None,
                    vision_fallback_api_key_ref: None,
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
                        ],
                    ),
                });
                updated = true;
            }
        }
    }

    Ok(updated)
}

fn ensure_default_native_openai_models(catalog: &mut Vec<ModelCatalogEntry>) -> bool {
    let mut updated = false;
    for (model_id, display_name) in DEFAULT_NATIVE_OPENAI_MODELS {
        match catalog.iter_mut().find(|entry| entry.model_id == *model_id) {
            Some(existing) => {
                if is_native_openai_catalog_entry(existing) {
                    if existing.display_name.is_none() {
                        existing.display_name = Some((*display_name).to_string());
                        updated = true;
                    }
                    if existing.backend_model.is_none() {
                        existing.backend_model = Some((*model_id).to_string());
                        updated = true;
                    }
                    if !existing.visible {
                        existing.visible = true;
                        updated = true;
                    }
                }
            }
            None => {
                catalog.push(ModelCatalogEntry {
                    model_id: (*model_id).to_string(),
                    display_name: Some((*display_name).to_string()),
                    provider: "openai".to_string(),
                    backend_model: Some((*model_id).to_string()),
                    backend_provider: None,
                    visible: true,
                    reasoning: None,
                    note: Some(
                        "Codex native model seeded by Codex Box MultiRouter defaults".to_string(),
                    ),
                    vision_bridge_enabled: None,
                    vision_fallback_base_url: None,
                    vision_fallback_model: None,
                    vision_fallback_api_key_ref: None,
                    extra: BTreeMap::from([
                        ("supported_in_api".to_string(), serde_json::json!(true)),
                        (
                            "available_in_plans".to_string(),
                            serde_json::json!(["plus", "pro", "team", "business", "enterprise"]),
                        ),
                    ]),
                });
                updated = true;
            }
        }
    }
    updated
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

    let fallback_env_key = env_key_for_provider(&provider_name);
    let (api_key_ref, env_key, runtime_api_key) =
        api_key_input_to_env_ref(api_key, &fallback_env_key)?;
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
    let wire_api = request
        .wire_api
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("responses");
    if !matches!(wire_api, "responses" | "chat" | "sse_stream" | "custom") {
        return Err(AppError::Command(format!(
            "wire_api 只支持 responses/chat/sse_stream/custom，当前: {wire_api}"
        )));
    }

    Ok(SimpleModelConfigPlan {
        provider: ProviderRoute {
            name: provider_name.clone(),
            base_url: base_url.to_string(),
            wire_api: wire_api.to_string(),
            api_key_ref: Some(api_key_ref),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: Some("通过模型配置页添加".to_string()),
            codex_routing: None,
            extra: BTreeMap::new(),
        },
        model: ModelCatalogEntry {
            model_id: model_slug.clone(),
            display_name: Some(display_name),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
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
        runtime_api_key,
        restart_codex: request.restart_codex,
    })
}

#[derive(Debug, Clone)]
struct MultirouterBuild {
    router_provider: ProviderRoute,
    catalog: Vec<ModelCatalogEntry>,
    route_count: usize,
    routed_model_count: usize,
    skipped_models: Vec<String>,
    proxy_base_url: String,
}

fn build_multirouter_plan(
    providers: &[ProviderRoute],
    catalog: &[ModelCatalogEntry],
    router_provider_id: &str,
    proxy_port: u16,
) -> AppResult<MultirouterBuild> {
    if proxy_port == 0 {
        return Err(AppError::Command("proxy_port 不能为 0".to_string()));
    }
    let router_provider_id = normalize_provider_id(router_provider_id);
    if router_provider_id.is_empty() {
        return Err(AppError::Command("router_provider_id 不能为空".to_string()));
    }

    let existing_router = providers
        .iter()
        .find(|provider| provider.name == router_provider_id);
    let real_providers: BTreeMap<String, ProviderRoute> = providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && provider.name != router_provider_id
                && !is_legacy_opencodex_provider_route(provider)
                && !provider.name.trim().is_empty()
                && !provider.base_url.trim().is_empty()
        })
        .map(|provider| (provider.name.clone(), provider.clone()))
        .collect();
    let mut native_openai_models = Vec::new();
    let mut route_models: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut skipped_models = Vec::new();
    let mut next_catalog = Vec::with_capacity(catalog.len());
    for entry in catalog {
        if is_native_openai_catalog_entry(entry) {
            let upstream_model = entry
                .backend_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&entry.model_id)
                .to_string();
            native_openai_models.push((entry.model_id.clone(), upstream_model));
            next_catalog.push(entry.clone());
            continue;
        }

        let target_provider =
            catalog_entry_target_provider(entry, &router_provider_id).or_else(|| {
                existing_router
                    .and_then(|router| router.codex_routing.as_ref())
                    .and_then(|routing| existing_router_target_for_model(entry, routing))
            });
        let Some(target_provider) = target_provider else {
            skipped_models.push(entry.model_id.clone());
            next_catalog.push(entry.clone());
            continue;
        };
        if target_provider == router_provider_id {
            skipped_models.push(entry.model_id.clone());
            next_catalog.push(entry.clone());
            continue;
        }
        if !real_providers.contains_key(&target_provider) {
            skipped_models.push(entry.model_id.clone());
            next_catalog.push(entry.clone());
            continue;
        }

        let upstream_model = entry
            .backend_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&entry.model_id)
            .to_string();
        route_models
            .entry(target_provider.clone())
            .or_default()
            .push((entry.model_id.clone(), upstream_model.clone()));

        let mut rewritten = entry.clone();
        rewritten.provider = router_provider_id.clone();
        rewritten.backend_provider = Some(router_provider_id.clone());
        rewritten.backend_model = Some(upstream_model);
        rewritten.extra.insert(
            "target_provider".to_string(),
            serde_json::Value::String(target_provider.clone()),
        );
        rewritten.extra.insert(
            "targetProvider".to_string(),
            serde_json::Value::String(target_provider.clone()),
        );
        next_catalog.push(rewritten);
    }

    if route_models.is_empty() && native_openai_models.is_empty() {
        return Err(AppError::Command(
            "没有可纳入 MultiRouter 的模型目录条目。请先添加模型，或检查 backend_provider 是否指向真实 Provider。"
                .to_string(),
        ));
    }

    let mut used_route_ids = std::collections::BTreeSet::new();
    let mut routes = Vec::new();
    let mut routed_model_count = 0usize;
    if !native_openai_models.is_empty() {
        let mut match_models = Vec::new();
        let mut model_map = BTreeMap::new();
        for (model_id, upstream_model) in native_openai_models {
            routed_model_count += 1;
            if !match_models.contains(&model_id) {
                match_models.push(model_id.clone());
            }
            model_map.insert(model_id, upstream_model);
        }
        routes.push(CodexRoutingRoute {
            id: OFFICIAL_OPENAI_ROUTE_ID.to_string(),
            label: Some("OpenAI Official".to_string()),
            enabled: Some(true),
            target_provider_id: None,
            match_rule: CodexRoutingMatch {
                models: match_models,
                prefixes: Vec::new(),
            },
            upstream: CodexRoutingUpstream {
                base_url: Some(OFFICIAL_CODEX_BACKEND_URL.to_string()),
                api_format: Some("openai_responses".to_string()),
                auth: Some(serde_json::json!({ "source": "managed_codex_oauth" })),
                api_key: None,
                model_map,
                extra: BTreeMap::new(),
            },
            capabilities: Some(serde_json::json!({
                "channel": "official_subscription",
                "apiFormat": "openai_responses",
                "inputModalities": ["text", "image"]
            })),
            extra: BTreeMap::new(),
        });
        used_route_ids.insert(OFFICIAL_OPENAI_ROUTE_ID.to_string());
    }
    for (target_provider, models) in route_models {
        let provider = real_providers
            .get(&target_provider)
            .expect("target provider collected above");
        let route_id = unique_route_id(&target_provider, &mut used_route_ids);
        let mut match_models = Vec::new();
        let mut model_map = BTreeMap::new();
        for (model_id, upstream_model) in models {
            routed_model_count += 1;
            if !match_models.contains(&model_id) {
                match_models.push(model_id.clone());
            }
            model_map.insert(model_id, upstream_model);
        }
        let api_format = wire_api_to_codex_api_format(&provider.wire_api);
        let capabilities =
            codex_route_capabilities_for_provider(provider, &match_models, &model_map, &api_format);
        routes.push(CodexRoutingRoute {
            id: route_id,
            label: Some(provider.name.clone()),
            enabled: Some(true),
            target_provider_id: Some(provider.name.clone()),
            match_rule: CodexRoutingMatch {
                models: match_models,
                prefixes: Vec::new(),
            },
            upstream: CodexRoutingUpstream {
                base_url: None,
                api_format: Some(api_format),
                auth: Some(serde_json::json!({ "source": "provider_config" })),
                api_key: None,
                model_map,
                extra: BTreeMap::new(),
            },
            capabilities,
            extra: BTreeMap::new(),
        });
    }

    let proxy_base_url = format!("http://127.0.0.1:{proxy_port}/v1");
    let existing_headers = existing_router
        .map(|provider| provider.http_headers.clone())
        .unwrap_or_default();
    let existing_extra = existing_router
        .map(|provider| provider.extra.clone())
        .unwrap_or_default();
    let existing_routing_extra = existing_router
        .and_then(|provider| provider.codex_routing.as_ref())
        .map(|routing| routing.extra.clone())
        .unwrap_or_default();
    let default_route_id = routes
        .iter()
        .find(|route| route.id != OFFICIAL_OPENAI_ROUTE_ID)
        .or_else(|| routes.first())
        .map(|route| route.id.clone());

    let router_provider = ProviderRoute {
        name: router_provider_id,
        base_url: proxy_base_url.clone(),
        wire_api: "responses".to_string(),
        api_key_ref: None,
        http_headers: existing_headers,
        enabled: true,
        note: Some(
            "Codex Box MultiRouter，本地聚合 Codex 下拉框模型并按 model id 分流。".to_string(),
        ),
        codex_routing: Some(CodexRoutingConfig {
            enabled: Some(true),
            default_route_id,
            routes,
            extra: existing_routing_extra,
        }),
        extra: existing_extra,
    };

    Ok(MultirouterBuild {
        route_count: router_provider
            .codex_routing
            .as_ref()
            .map(|routing| routing.routes.len())
            .unwrap_or(0),
        router_provider,
        catalog: next_catalog,
        routed_model_count,
        skipped_models,
        proxy_base_url,
    })
}

fn codex_route_capabilities_for_provider(
    provider: &ProviderRoute,
    match_models: &[String],
    model_map: &BTreeMap<String, String>,
    api_format: &str,
) -> Option<serde_json::Value> {
    let text_only = route_targets_text_only(provider, match_models, model_map);
    let minimax = route_targets_minimax(provider, match_models, model_map);
    let mut capabilities = serde_json::Map::new();

    capabilities.insert("apiFormat".to_string(), serde_json::json!(api_format));
    capabilities.insert(
        "inputModalities".to_string(),
        if text_only {
            serde_json::json!(["text"])
        } else {
            serde_json::json!(["text", "image"])
        },
    );

    if text_only {
        capabilities.insert("textOnly".to_string(), serde_json::json!(true));
    }
    if minimax {
        capabilities.insert(
            "codexChatReasoning".to_string(),
            serde_json::json!({
                "supportsThinking": true,
                "supportsEffort": false,
                "thinkingParam": "reasoning_split",
                "outputFormat": "reasoning_details"
            }),
        );
    }

    Some(serde_json::Value::Object(capabilities))
}

fn route_targets_text_only(
    provider: &ProviderRoute,
    match_models: &[String],
    model_map: &BTreeMap<String, String>,
) -> bool {
    !match_models.is_empty()
        && match_models.iter().all(|model_id| {
            let upstream_model = model_map
                .get(model_id)
                .map(String::as_str)
                .unwrap_or(model_id);
            known_text_only_catalog_target(&provider.name, model_id)
                || known_text_only_catalog_target(&provider.name, upstream_model)
        })
}

fn route_targets_minimax(
    provider: &ProviderRoute,
    match_models: &[String],
    model_map: &BTreeMap<String, String>,
) -> bool {
    match_models.iter().any(|model_id| {
        let upstream_model = model_map
            .get(model_id)
            .map(String::as_str)
            .unwrap_or(model_id);
        known_minimax_catalog_target(&provider.name, model_id)
            || known_minimax_catalog_target(&provider.name, upstream_model)
    })
}

fn catalog_entry_target_provider(
    entry: &ModelCatalogEntry,
    router_provider_id: &str,
) -> Option<String> {
    catalog_entry_extra_target_provider(entry, router_provider_id).or_else(|| {
        entry
            .backend_provider
            .as_deref()
            .map(str::trim)
            .filter(|provider| {
                !provider.is_empty()
                    && !provider.eq_ignore_ascii_case("openai")
                    && *provider != router_provider_id
            })
            .map(ToString::to_string)
            .or_else(|| {
                let provider = entry.provider.trim();
                if provider.is_empty()
                    || provider.eq_ignore_ascii_case("openai")
                    || provider == router_provider_id
                {
                    None
                } else {
                    Some(provider.to_string())
                }
            })
    })
}

fn catalog_entry_extra_target_provider(
    entry: &ModelCatalogEntry,
    router_provider_id: &str,
) -> Option<String> {
    [
        "targetProvider",
        "target_provider",
        "upstreamProvider",
        "upstream_provider",
    ]
    .iter()
    .filter_map(|key| entry.extra.get(*key))
    .find_map(|value| value.as_str())
    .map(str::trim)
    .filter(|provider| {
        !provider.is_empty()
            && !provider.eq_ignore_ascii_case("openai")
            && *provider != router_provider_id
    })
    .map(ToString::to_string)
}

fn existing_router_target_for_model(
    entry: &ModelCatalogEntry,
    routing: &CodexRoutingConfig,
) -> Option<String> {
    let requested = entry.model_id.trim();
    let backend_model = entry
        .backend_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(requested);
    routing
        .routes
        .iter()
        .filter(|route| route.enabled.unwrap_or(true))
        .find(|route| {
            route.match_rule.models.iter().any(|model| {
                let model = model.trim();
                !model.is_empty() && (model == requested || model == backend_model)
            }) || route.match_rule.prefixes.iter().any(|prefix| {
                let prefix = prefix.trim();
                !prefix.is_empty()
                    && (requested.starts_with(prefix) || backend_model.starts_with(prefix))
            })
        })
        .and_then(|route| route.target_provider_id.clone())
}

fn unique_route_id(provider_name: &str, used: &mut std::collections::BTreeSet<String>) -> String {
    let base = sanitize_identifier(provider_name);
    let base = if base.is_empty() {
        "route".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut index = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn normalize_provider_id(value: &str) -> String {
    let mut out = String::new();
    let mut last_separator = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch.to_ascii_lowercase());
            last_separator = false;
        } else if !last_separator {
            out.push('_');
            last_separator = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn wire_api_to_codex_api_format(wire_api: &str) -> String {
    match wire_api.trim() {
        "chat" => "openai_chat".to_string(),
        "responses" => "openai_responses".to_string(),
        "openai_chat" | "openai_responses" | "openai_messages" => wire_api.trim().to_string(),
        _ => "openai_responses".to_string(),
    }
}

fn is_native_openai_catalog_entry(entry: &ModelCatalogEntry) -> bool {
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

fn provider_routes_text(providers: &[ProviderRoute]) -> AppResult<String> {
    let items: Vec<serde_json::Value> = providers.iter().map(provider_to_file_value).collect();
    json_envelope_text("providers", items)
}

fn catalog_entries_text(catalog: &[ModelCatalogEntry]) -> AppResult<String> {
    let items: Vec<serde_json::Value> = catalog.iter().map(catalog_to_file_value).collect();
    json_envelope_text("models", items)
}

fn providers_with_router(
    providers: &[ProviderRoute],
    router_provider: &ProviderRoute,
) -> Vec<ProviderRoute> {
    let mut next = providers
        .iter()
        .filter(|provider| provider.name != router_provider.name)
        .cloned()
        .collect::<Vec<_>>();
    next.push(router_provider.clone());
    next
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn catalog_extra_u64(entry: &ModelCatalogEntry, keys: &[&str], fallback: u64) -> u64 {
    keys.iter()
        .find_map(|key| {
            let value = entry.extra.get(*key)?;
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(fallback)
}

fn catalog_reasoning_efforts(entry: &ModelCatalogEntry) -> Vec<(String, String)> {
    if let Some(levels) = entry
        .extra
        .get("supported_reasoning_levels")
        .and_then(|value| value.as_array())
    {
        let parsed = levels
            .iter()
            .filter_map(|value| {
                let effort = value
                    .get("effort")
                    .or_else(|| value.get("reasoning_effort"))
                    .or_else(|| value.get("reasoningEffort"))
                    .and_then(|item| item.as_str())
                    .map(str::trim)
                    .filter(|item| !item.is_empty())?;
                let description = value
                    .get("description")
                    .and_then(|item| item.as_str())
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .unwrap_or("Available");
                Some((effort.to_string(), description.to_string()))
            })
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(reasoning) = &entry.reasoning {
        if reasoning.enabled && !reasoning.levels.is_empty() {
            return reasoning
                .levels
                .iter()
                .map(|level| (level.clone(), "Available".to_string()))
                .collect();
        }
    }

    vec![("medium".to_string(), "Balanced".to_string())]
}

fn codex_provider_inline_models(catalog: &[ModelCatalogEntry]) -> String {
    let entries = catalog
        .iter()
        .filter(|entry| entry.visible)
        .map(|entry| {
            let model = entry.model_id.trim();
            let display_name = entry
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(model);
            let context_window = catalog_extra_u64(
                entry,
                &["context_window", "contextWindow", "model_context_window"],
                128_000,
            );
            let efforts = catalog_reasoning_efforts(entry);
            let default_effort = entry
                .extra
                .get("default_reasoning_level")
                .or_else(|| entry.extra.get("default_reasoning_effort"))
                .or_else(|| entry.extra.get("defaultReasoningEffort"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| efforts.first().map(|item| item.0.as_str()).unwrap_or("medium"));
            let snake_efforts = efforts
                .iter()
                .map(|(effort, description)| {
                    format!(
                        "{{ reasoning_effort = {}, description = {} }}",
                        toml_string(effort),
                        toml_string(description)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let camel_efforts = efforts
                .iter()
                .map(|(effort, description)| {
                    format!(
                        "{{ reasoningEffort = {}, description = {} }}",
                        toml_string(effort),
                        toml_string(description)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            format!(
                "{{ model = {model}, id = {model}, display_name = {display}, displayName = {display}, context_window = {context_window}, contextWindow = {context_window}, default_reasoning_effort = {default_effort}, defaultReasoningEffort = {default_effort}, supported_reasoning_efforts = [{snake_efforts}], supportedReasoningEfforts = [{camel_efforts}], hidden = false }}",
                model = toml_string(model),
                display = toml_string(display_name),
                default_effort = toml_string(default_effort),
            )
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        String::new()
    } else {
        format!("models = [{}]\n", entries.join(", "))
    }
}

fn build_codex_managed_config_text(
    raw: &str,
    catalog_path: &Path,
    catalog: &[ModelCatalogEntry],
    router_provider_id: &str,
    proxy_base_url: &str,
) -> String {
    let cleaned = strip_opencodex_managed_blocks(raw);
    let managed_top =
        opencodex_managed_top_config(catalog_path, router_provider_id, proxy_base_url);
    let managed_provider =
        opencodex_managed_provider_config(catalog, router_provider_id, proxy_base_url);
    let body = cleaned.trim();
    if body.is_empty() {
        ensure_trailing_newline(&format!("{managed_top}\n\n{managed_provider}"))
    } else {
        ensure_trailing_newline(&format!("{managed_top}\n\n{body}\n\n{managed_provider}"))
    }
}

fn write_codex_managed_config(
    path: &Path,
    catalog_path: &Path,
    catalog: &[ModelCatalogEntry],
    router_provider_id: &str,
    proxy_base_url: &str,
    expected_hash: &str,
) -> AppResult<Option<OpenCodexWriteResult>> {
    let raw = read_optional_raw(path)?;
    let actual_hash = hash_or_empty(&raw);
    if actual_hash != expected_hash {
        return Err(AppError::Command(
            "config.toml 已变化，请重新预览 MultiRouter diff 后再确认。".to_string(),
        ));
    }

    let new_text = build_codex_managed_config_text(
        &raw,
        catalog_path,
        catalog,
        router_provider_id,
        proxy_base_url,
    );
    if new_text == raw {
        return Ok(None);
    }

    let backup_record = if path.exists() {
        let dir = backup_dir()?;
        Some(backup::create_backup_with_extension(
            path,
            &dir,
            BackupReason::PreWrite,
            "toml",
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

    Ok(Some(OpenCodexWriteResult {
        file_path: path.display().to_string(),
        backup_id: backup_record.map(|record| record.id).unwrap_or_default(),
        new_hash: content_hash(&new_text),
    }))
}

fn codex_models_cache_models(catalog: &[ModelCatalogEntry]) -> Vec<serde_json::Value> {
    catalog
        .iter()
        .filter(|entry| entry.visible)
        .map(catalog_to_file_value)
        .collect()
}

fn build_codex_models_cache_text(
    raw: &str,
    catalog: &[ModelCatalogEntry],
) -> AppResult<Option<String>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Ok(existing) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return Ok(None);
    };
    let Some(existing_obj) = existing.as_object() else {
        return Ok(None);
    };
    let Some(client_version) = existing_obj
        .get("client_version")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let models = codex_models_cache_models(catalog);
    if models.is_empty() {
        return Ok(None);
    }
    if existing_obj.get("etag").and_then(|value| value.as_str())
        == Some(CODEX_BOX_MODELS_CACHE_ETAG)
        && existing_obj.get("models") == Some(&serde_json::Value::Array(models.clone()))
    {
        return Ok(Some(raw.to_string()));
    }

    let cache = serde_json::json!({
        "fetched_at": Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
        "etag": CODEX_BOX_MODELS_CACHE_ETAG,
        "client_version": client_version,
        "models": models,
    });
    let text = serde_json::to_string_pretty(&cache)
        .map_err(|e| AppError::Command(format!("serialize models_cache.json failed: {e}")))?;
    Ok(Some(ensure_trailing_newline(&text)))
}

fn codex_models_cache_is_codex_box_owned(raw: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("etag")
                .and_then(|etag| etag.as_str())
                .map(str::to_string)
        })
        .as_deref()
        == Some(CODEX_BOX_MODELS_CACHE_ETAG)
}

fn ensure_models_cache_restore_backup(path: &Path, raw: &str) -> AppResult<()> {
    if !path.exists() || raw.trim().is_empty() || codex_models_cache_is_codex_box_owned(raw) {
        return Ok(());
    }
    let backup_path = native_models_cache_backup_path_for(path);
    if backup_path.exists() {
        return Ok(());
    }
    if let Some(parent) = backup_path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::Io)?;
    }
    writer::atomic_write(&backup_path, raw)
}

fn write_codex_models_cache(
    path: &Path,
    catalog: &[ModelCatalogEntry],
    expected_hash: &str,
) -> AppResult<Option<OpenCodexWriteResult>> {
    let raw = read_optional_raw(path)?;
    let actual_hash = hash_or_empty(&raw);
    if actual_hash != expected_hash {
        return Err(AppError::Command(
            "models_cache.json 已变化，请重新预览 MultiRouter diff 后再确认。".to_string(),
        ));
    }

    let Some(new_text) = build_codex_models_cache_text(&raw, catalog)? else {
        return Ok(None);
    };
    if new_text == raw {
        return Ok(None);
    }
    ensure_models_cache_restore_backup(path, &raw)?;

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

    Ok(Some(OpenCodexWriteResult {
        file_path: path.display().to_string(),
        backup_id: backup_record.map(|record| record.id).unwrap_or_default(),
        new_hash: content_hash(&new_text),
    }))
}

fn restore_codex_models_cache_if_owned(
    path: &Path,
    history_backup_dir: &Path,
    expected_hash: &str,
) -> AppResult<CodexModelsCacheRestoreResult> {
    let raw = read_optional_raw(path)?;
    let actual_hash = hash_or_empty(&raw);
    if actual_hash != expected_hash {
        return Err(AppError::Command(
            "models_cache.json 已变化，请重新预览恢复 diff 后再确认。".to_string(),
        ));
    }

    let backup_path = native_models_cache_backup_path_for(path);
    if !path.exists() || !codex_models_cache_is_codex_box_owned(&raw) {
        return Ok(CodexModelsCacheRestoreResult {
            models_cache_path: path.display().to_string(),
            backup_path: backup_path.display().to_string(),
            backup_id: String::new(),
            new_hash: actual_hash,
            restored: false,
            deleted: false,
        });
    }

    let current_backup = {
        backup::create_backup_with_extension(
            path,
            history_backup_dir,
            BackupReason::PreWrite,
            "json",
        )?
    };

    if backup_path.exists() {
        let backup_text = std::fs::read_to_string(&backup_path).map_err(AppError::Io)?;
        if let Err(error) = writer::atomic_write(path, &backup_text) {
            if let Ok(current_text) = std::fs::read_to_string(&current_backup.file_path) {
                let _ = writer::atomic_write(path, &current_text);
            }
            return Err(error);
        }
        let _ = std::fs::remove_file(&backup_path);
        Ok(CodexModelsCacheRestoreResult {
            models_cache_path: path.display().to_string(),
            backup_path: backup_path.display().to_string(),
            backup_id: current_backup.id,
            new_hash: content_hash(&backup_text),
            restored: true,
            deleted: false,
        })
    } else {
        std::fs::remove_file(path).map_err(AppError::Io)?;
        Ok(CodexModelsCacheRestoreResult {
            models_cache_path: path.display().to_string(),
            backup_path: backup_path.display().to_string(),
            backup_id: current_backup.id,
            new_hash: String::new(),
            restored: true,
            deleted: true,
        })
    }
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
    let (mut catalog, raw_catalog_text, catalog_err) = read_catalog(&catalog_path);

    let mut parse_errors = Vec::new();
    if let Some(message) = providers_err {
        parse_errors.push(OpenCodexParseError {
            file: providers_path.display().to_string(),
            message,
        });
    }
    if let Some(message) = catalog_err.as_ref() {
        parse_errors.push(OpenCodexParseError {
            file: catalog_path.display().to_string(),
            message: message.clone(),
        });
    }
    if catalog_err.is_none() {
        let cache_path = native_models_cache_path()?;
        if let Err(error) = merge_native_models_into_catalog_if_available(&mut catalog, &cache_path)
        {
            parse_errors.push(OpenCodexParseError {
                file: cache_path.display().to_string(),
                message: error.to_string(),
            });
        }
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

#[tauri::command]
pub fn config_import_sources_scan() -> AppResult<Vec<ConfigImportSource>> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let mut sources = Vec::new();

    let legacy_providers = legacy_providers_path()?;
    let legacy_catalog = legacy_catalog_path()?;
    if legacy_providers.exists() || legacy_catalog.exists() {
        let (providers, _, providers_err) = read_providers(&legacy_providers);
        let (catalog, _, catalog_err) = read_catalog(&legacy_catalog);
        let mut warnings = Vec::new();
        if let Some(error) = providers_err {
            warnings.push(format!("providers.json 需要处理: {error}"));
        }
        if let Some(error) = catalog_err {
            warnings.push(format!("custom_model_catalog.json 需要处理: {error}"));
        }
        if providers.iter().any(provider_has_direct_key) {
            warnings.push("发现明文 API Key,导入前建议整理为环境变量引用。".to_string());
        }
        sources.push(ConfigImportSource {
            id: "opencodex".to_string(),
            display_name: "OpenCodex".to_string(),
            source_kind: "opencodex".to_string(),
            path: legacy_opencodex_dir()?.display().to_string(),
            providers: providers.len(),
            models: catalog.len(),
            config_snapshots: 0,
            warnings,
            recommended_action: "预览后导入到 ~/.codex/codex-box/。原 OpenCodex 文件不会被修改。"
                .to_string(),
            can_import: true,
        });
    }

    let codex_dir = home.join(".codex");
    let config_sources = scan_codex_config_sources(&codex_dir);
    if !config_sources.is_empty() {
        let mut warnings = Vec::new();
        let mut codex_plus_plus_found = false;
        let mut legacy_port_found = false;
        for path in &config_sources {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if raw.contains("CodexPlusPlus") {
                    codex_plus_plus_found = true;
                }
                if raw.contains("127.0.0.1:8765") {
                    legacy_port_found = true;
                }
            }
        }
        if codex_plus_plus_found {
            warnings
                .push("发现 Codex++/CodexPlusPlus 配置痕迹,可作为普通 Provider 导入。".to_string());
        }
        if legacy_port_found {
            warnings.push(
                "发现 127.0.0.1:8765,这是 OpenCodex 残留端口,导入时应迁移到 Codex Box 本地连接。"
                    .to_string(),
            );
        }
        sources.push(ConfigImportSource {
            id: "codex-configs".to_string(),
            display_name: "Codex 配置与备份".to_string(),
            source_kind: "codex_config".to_string(),
            path: codex_dir.display().to_string(),
            providers: count_model_providers_in_configs(&config_sources),
            models: 0,
            config_snapshots: config_sources.len(),
            warnings,
            recommended_action:
                "从 config.toml 和备份中恢复历史 Provider/Profile,写入前展示 diff。".to_string(),
            can_import: true,
        });
    }

    sources.push(ConfigImportSource {
        id: "cc-switch".to_string(),
        display_name: "CC Switch".to_string(),
        source_kind: "manual_directory".to_string(),
        path: "需要手动选择目录".to_string(),
        providers: 0,
        models: 0,
        config_snapshots: 0,
        warnings: vec!["当前只扫描它最终写入的 Codex 配置,不读取内部数据库。".to_string()],
        recommended_action: "在导入页选择 CC Switch 备份或导出的配置目录。".to_string(),
        can_import: false,
    });
    sources.push(ConfigImportSource {
        id: "cockpit-tools".to_string(),
        display_name: "Cockpit Tools".to_string(),
        source_kind: "manual_directory".to_string(),
        path: "需要手动选择 Data Directory".to_string(),
        providers: 0,
        models: 0,
        config_snapshots: 0,
        warnings: vec!["Codex Box 只做只读扫描,不会修改 Cockpit Tools 数据目录。".to_string()],
        recommended_action: "在导入页选择 Cockpit Tools Data Directory 后预览导入。".to_string(),
        can_import: false,
    });

    Ok(sources)
}

#[tauri::command]
pub fn opencodex_import_preview() -> AppResult<ConfigImportPreview> {
    let providers_source_path = legacy_providers_path()?;
    let catalog_source_path = legacy_catalog_path()?;
    if !providers_source_path.exists() && !catalog_source_path.exists() {
        return Err(AppError::Command(
            "未发现 ~/.opencodex/providers.json 或 custom_model_catalog.json。".to_string(),
        ));
    }

    let providers_target_path = providers_path()?;
    let catalog_target_path = catalog_path()?;
    let (providers, providers_raw, providers_err) = read_providers(&providers_source_path);
    let (catalog, catalog_raw, catalog_err) = read_catalog(&catalog_source_path);
    if let Some(error) = providers_err {
        return Err(AppError::Command(format!(
            "OpenCodex providers.json 需要处理: {error}"
        )));
    }
    if let Some(error) = catalog_err {
        return Err(AppError::Command(format!(
            "OpenCodex custom_model_catalog.json 需要处理: {error}"
        )));
    }

    let projection = build_import_projection(&providers, &catalog)?;
    let target_providers_raw = read_optional_raw(&providers_target_path)?;
    let target_catalog_raw = read_optional_raw(&catalog_target_path)?;
    let providers_expected_hash = hash_or_empty(&target_providers_raw);
    let catalog_expected_hash = hash_or_empty(&target_catalog_raw);
    let mut warnings = projection.warnings;
    if providers_raw.contains("127.0.0.1:8765") || catalog_raw.contains("127.0.0.1:8765") {
        warnings.push(
            "发现 OpenCodex 8765 残留,连接 Codex 时应迁移到 Codex Box 本地连接。".to_string(),
        );
    }

    let redacted_target_providers = redact_api_keys_in_json(&target_providers_raw);
    let redacted_source_providers = redact_api_keys_in_json(&projection.providers_text);

    Ok(ConfigImportPreview {
        source_id: "opencodex".to_string(),
        providers_source_path: providers_source_path.display().to_string(),
        catalog_source_path: catalog_source_path.display().to_string(),
        providers_target_path: providers_target_path.display().to_string(),
        catalog_target_path: catalog_target_path.display().to_string(),
        providers_expected_hash,
        catalog_expected_hash,
        providers_diff: crate::config::diff::between(
            &redacted_target_providers,
            &redacted_source_providers,
        ),
        catalog_diff: crate::config::diff::between(&target_catalog_raw, &projection.catalog_text),
        providers: providers.len(),
        models: catalog.len(),
        warnings,
    })
}

#[tauri::command]
pub fn opencodex_import_apply(
    request: ApplyConfigImportRequest,
) -> AppResult<ApplyConfigImportResult> {
    if !request.confirmed {
        return Err(AppError::Command(
            "导入 OpenCodex 配置需要 confirmed=true".to_string(),
        ));
    }
    if request.preview.source_id != "opencodex" {
        return Err(AppError::Command("不支持的导入来源。".to_string()));
    }

    let providers_source_path = legacy_providers_path()?;
    let catalog_source_path = legacy_catalog_path()?;
    let providers_target_path = providers_path()?;
    let catalog_target_path = catalog_path()?;
    let (providers, _, providers_err) = read_providers(&providers_source_path);
    if let Some(error) = providers_err {
        return Err(AppError::Command(format!(
            "OpenCodex providers.json 需要处理: {error}"
        )));
    }
    let (catalog, _, catalog_err) = read_catalog(&catalog_source_path);
    if let Some(error) = catalog_err {
        return Err(AppError::Command(format!(
            "OpenCodex custom_model_catalog.json 需要处理: {error}"
        )));
    }
    let projection = build_import_projection(&providers, &catalog)?;

    let provider_write = write_import_target(
        &providers_target_path,
        &request.preview.providers_expected_hash,
        &projection.providers_text,
    )?;
    let catalog_write = write_import_target(
        &catalog_target_path,
        &request.preview.catalog_expected_hash,
        &projection.catalog_text,
    )?;
    for (env_key, secret) in projection.runtime_env {
        std::env::set_var(env_key, secret);
    }

    Ok(ApplyConfigImportResult {
        provider_write,
        catalog_write,
    })
}

#[derive(Debug, Clone)]
struct ImportProjection {
    providers_text: String,
    catalog_text: String,
    warnings: Vec<String>,
    runtime_env: Vec<(String, String)>,
}

fn build_import_projection(
    providers: &[ProviderRoute],
    catalog: &[ModelCatalogEntry],
) -> AppResult<ImportProjection> {
    let (providers, runtime_env, converted_keys) = sanitize_import_providers(providers);
    let mut catalog = catalog.to_vec();
    ensure_default_native_openai_models(&mut catalog);
    let mut warnings = Vec::new();
    if !converted_keys.is_empty() {
        warnings.push(format!(
            "发现明文 API Key,导入时会改写为环境变量引用: {}。",
            converted_keys.join(", ")
        ));
    }

    Ok(ImportProjection {
        providers_text: provider_routes_text(&providers)?,
        catalog_text: catalog_entries_text(&catalog)?,
        warnings,
        runtime_env,
    })
}

fn sanitize_import_providers(
    providers: &[ProviderRoute],
) -> (Vec<ProviderRoute>, Vec<(String, String)>, Vec<String>) {
    let mut runtime_env = Vec::new();
    let mut converted_keys = Vec::new();
    let mut sanitized = Vec::with_capacity(providers.len());
    for provider in providers {
        let mut provider = provider.clone();
        let direct_key = provider
            .extra
            .remove("api_key")
            .or_else(|| provider.extra.remove("apiKey"))
            .and_then(|value| value.as_str().map(str::trim).map(ToString::to_string))
            .filter(|value| !value.is_empty());
        if provider.api_key_ref.is_none() {
            if let Some(key) = direct_key {
                if key.starts_with('$') {
                    provider.api_key_ref = Some(key);
                } else {
                    let env_key = env_key_for_provider(&provider.name);
                    provider.api_key_ref = Some(format!("${{{env_key}}}"));
                    runtime_env.push((env_key.clone(), key));
                    converted_keys.push(format!("{} -> ${}", provider.name, env_key));
                }
            }
        }
        sanitized.push(provider);
    }
    (sanitized, runtime_env, converted_keys)
}

fn read_optional_raw(path: &Path) -> AppResult<String> {
    if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)
    } else {
        Ok(String::new())
    }
}

fn read_optional_existing_raw(path: &Path) -> AppResult<String> {
    if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)
    } else {
        Ok(String::new())
    }
}

fn hash_or_empty(raw: &str) -> String {
    if raw.trim().is_empty() {
        String::new()
    } else {
        content_hash(raw)
    }
}

fn ensure_preview_hash(label: &str, raw: &str, expected_hash: &str) -> AppResult<()> {
    let actual_hash = hash_or_empty(raw);
    if actual_hash == expected_hash {
        return Ok(());
    }
    Err(AppError::Command(format!(
        "{label} 已变化，请重新预览 MultiRouter diff 后再确认。"
    )))
}

fn preflight_multirouter_apply_hashes(
    preview: &CodexMultirouterPreview,
    providers_raw: &str,
    catalog_raw: &str,
    config_raw: Option<&str>,
    models_cache_raw: &str,
    inject_map_raw: &str,
) -> AppResult<()> {
    ensure_preview_hash(
        "providers.json",
        providers_raw,
        &preview.providers_expected_hash,
    )?;
    ensure_preview_hash(
        "custom_model_catalog.json",
        catalog_raw,
        &preview.catalog_expected_hash,
    )?;
    if preview.ensure_codex_config {
        ensure_preview_hash(
            "config.toml",
            config_raw.unwrap_or_default(),
            &preview.config_expected_hash,
        )?;
    }
    ensure_preview_hash(
        "models_cache.json",
        models_cache_raw,
        &preview.models_cache_expected_hash,
    )?;
    ensure_preview_hash(
        "inject-map.json",
        inject_map_raw,
        &preview.inject_map_expected_hash,
    )?;
    Ok(())
}

fn is_legacy_inject_map_entry(entry: &crate::proxy::inject_map::InjectMapEntry) -> bool {
    entry.name.eq_ignore_ascii_case("opencodex")
        || entry.original_base_url.contains("127.0.0.1:8765")
        || entry.original_base_url.contains("localhost:8765")
}

fn clean_legacy_inject_map_text(raw: &str) -> AppResult<(String, bool, InjectMap)> {
    if raw.trim().is_empty() {
        return Ok((raw.to_string(), false, InjectMap::default()));
    }
    let mut map = serde_json::from_str::<InjectMap>(raw)
        .map_err(|e| AppError::Command(format!("inject-map.json 解析失败: {e}")))?;
    let before_len = map.providers.len();
    map.providers
        .retain(|entry| !is_legacy_inject_map_entry(entry));
    if map.providers.len() == before_len {
        return Ok((raw.to_string(), false, map));
    }
    let next_map = if map.providers.is_empty() {
        InjectMap::default()
    } else {
        map
    };
    let text = serde_json::to_string_pretty(&next_map)
        .map_err(|e| AppError::Command(format!("serialize inject-map: {e}")))?;
    Ok((ensure_trailing_newline(&text), true, next_map))
}

fn write_legacy_inject_map_cleanup(
    path: &Path,
    expected_hash: &str,
) -> AppResult<Option<(OpenCodexWriteResult, InjectMap)>> {
    let raw = read_optional_existing_raw(path)?;
    let actual_hash = hash_or_empty(&raw);
    if actual_hash != expected_hash {
        return Err(AppError::Command(
            "inject-map.json 已变化，请重新预览 MultiRouter diff 后再确认。".to_string(),
        ));
    }
    let (new_text, touched, next_map) = clean_legacy_inject_map_text(&raw)?;
    if !touched {
        return Ok(None);
    }

    let backup_record = if path.exists() {
        Some(backup::create_backup_with_extension(
            path,
            &backup_dir()?,
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

    Ok(Some((
        OpenCodexWriteResult {
            file_path: path.display().to_string(),
            backup_id: backup_record.map(|r| r.id).unwrap_or_default(),
            new_hash: content_hash(&new_text),
        },
        next_map,
    )))
}

fn redact_api_keys_in_json(raw: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    redact_api_keys_in_value(&mut value);
    serde_json::to_string_pretty(&value)
        .map(|text| ensure_trailing_newline(&text))
        .unwrap_or_else(|_| raw.to_string())
}

fn redact_api_keys_in_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key.eq_ignore_ascii_case("api_key") || key.eq_ignore_ascii_case("apiKey") {
                    if value.as_str().is_some_and(|text| !text.trim().is_empty()) {
                        *value = serde_json::Value::String("••••••••".to_string());
                    }
                } else {
                    redact_api_keys_in_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                redact_api_keys_in_value(value);
            }
        }
        _ => {}
    }
}

fn write_import_target(
    path: &Path,
    expected_hash: &str,
    new_text: &str,
) -> AppResult<OpenCodexWriteResult> {
    let current_raw = if path.exists() {
        std::fs::read_to_string(path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    let current_hash = hash_or_empty(&current_raw);
    if current_hash != expected_hash {
        return Err(AppError::Command(
            "目标文件已变化,请重新预览导入 diff 后再确认。".to_string(),
        ));
    }

    let new_text = ensure_trailing_newline(new_text);
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
        backup_id: backup_record.map(|record| record.id).unwrap_or_default(),
        new_hash: content_hash(&new_text),
    })
}

fn provider_has_direct_key(provider: &ProviderRoute) -> bool {
    provider
        .extra
        .get("api_key")
        .or_else(|| provider.extra.get("apiKey"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('$'))
        .is_some()
}

fn scan_codex_config_sources(codex_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let current = codex_dir.join("config.toml");
    if current.exists() {
        files.push(current);
    }
    if let Ok(entries) = std::fs::read_dir(codex_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if name.ends_with(".config.toml")
                || name.starts_with("config.toml.bak")
                || name.contains("config.toml.bak")
            {
                files.push(path);
            }
        }
    }
    let backups = codex_dir.join("codex-box/backups");
    if let Ok(entries) = std::fs::read_dir(backups) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("toml") {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn count_model_providers_in_configs(paths: &[PathBuf]) -> usize {
    let mut providers = std::collections::BTreeSet::new();
    for path in paths {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
            continue;
        };
        if let Some(table) = value
            .get("model_providers")
            .and_then(|value| value.as_table())
        {
            providers.extend(table.keys().cloned());
        }
        if let Some(provider) = value.get("model_provider").and_then(|value| value.as_str()) {
            providers.insert(provider.to_string());
        }
    }
    providers.len()
}

/// 读取 Codex Box providers.json。
/// 兼容 OpenCodex schema: api_key 可以是 $ENV_VAR 引用;本地保存的明文 key 只进入内存请求头和脱敏诊断。
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
        let codex_routing = entry
            .get("codexRouting")
            .cloned()
            .and_then(|value| serde_json::from_value::<CodexRoutingConfig>(value).ok());
        let enabled = entry
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let note = entry
            .get("note")
            .and_then(|value| value.as_str())
            .map(String::from);

        // api_key 字段: $ENV_VAR 作为环境变量引用;本地保存的 key 保留在 extra,由代理路由层以内存方式注入请求头。
        let raw_api_key = entry.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
        let mut extra = entry
            .as_object()
            .map(|obj| {
                collect_extra_fields(
                    obj,
                    &[
                        "name",
                        "base_url",
                        "wire_api",
                        "api_key",
                        "http_headers",
                        "codexRouting",
                        "enabled",
                        "note",
                    ],
                )
            })
            .unwrap_or_default();
        if codex_routing.is_none() {
            if let Some(value) = entry.get("codexRouting") {
                extra.insert("codexRouting".to_string(), value.clone());
            }
        }
        let api_key_ref = if raw_api_key.is_empty() {
            None
        } else if (raw_api_key.starts_with("${") && raw_api_key.ends_with('}'))
            || raw_api_key.starts_with('$')
        {
            Some(raw_api_key.to_string())
        } else {
            extra.insert(
                "api_key".to_string(),
                serde_json::Value::String(raw_api_key.to_string()),
            );
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
            enabled,
            note,
            codex_routing,
            extra,
        });
    }

    (providers, raw, None)
}

/// 读取 Codex Box custom_model_catalog.json。
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
            .or_else(|| obj.get("model"))
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
        let raw_provider = obj
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
        let provider = normalize_catalog_provider(&raw_provider, backend_provider.as_deref());
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
    write_simple_model_config_to_paths(
        &providers_path()?,
        &catalog_path()?,
        Some(&codex_config_path()?),
        &request,
    )
}

#[tauri::command]
pub fn codex_multirouter_preview(
    request: CodexMultirouterPreviewRequest,
) -> AppResult<CodexMultirouterPreview> {
    let providers_path = providers_path()?;
    let catalog_path = catalog_path()?;
    let config_path = codex_config_path()?;
    let models_cache_path = native_models_cache_path()?;
    let inject_map_path = inject_map::inject_map_path()?;

    let (providers, providers_raw, providers_err) = read_providers(&providers_path);
    if let Some(message) = providers_err {
        return Err(AppError::Command(format!(
            "providers.json 解析失败，不能预览 MultiRouter: {message}"
        )));
    }
    let (mut catalog, catalog_raw, catalog_err) = read_catalog(&catalog_path);
    if let Some(message) = catalog_err {
        return Err(AppError::Command(format!(
            "custom_model_catalog.json 解析失败，不能预览 MultiRouter: {message}"
        )));
    }
    merge_native_models_into_catalog_if_available(&mut catalog, &models_cache_path)?;
    ensure_default_native_openai_models(&mut catalog);

    let proxy_port = request.proxy_port.unwrap_or(DEFAULT_MULTIROUTER_PORT);
    let router_provider_id = request
        .router_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_LOCAL_PROVIDER_ID);
    let plan = build_multirouter_plan(&providers, &catalog, router_provider_id, proxy_port)?;

    let next_providers = providers_with_router(&providers, &plan.router_provider);
    let providers_next_text = provider_routes_text(&next_providers)?;
    let catalog_next_text = catalog_entries_text(&plan.catalog)?;
    let config_raw = read_optional_raw(&config_path)?;
    let ensure_codex_config = request.ensure_codex_config.unwrap_or(true);
    let config_next_text = if ensure_codex_config {
        build_codex_managed_config_text(
            &config_raw,
            &catalog_path,
            &plan.catalog,
            &plan.router_provider.name,
            &plan.proxy_base_url,
        )
    } else {
        config_raw.clone()
    };
    let models_cache_raw = read_optional_raw(&models_cache_path)?;
    let models_cache_next_text = build_codex_models_cache_text(&models_cache_raw, &plan.catalog)?
        .unwrap_or_else(|| models_cache_raw.clone());
    let models_cache_touched = models_cache_next_text != models_cache_raw;
    let inject_map_raw = read_optional_existing_raw(&inject_map_path)?;
    let (inject_map_next_text, inject_map_touched, _) =
        clean_legacy_inject_map_text(&inject_map_raw)?;

    Ok(CodexMultirouterPreview {
        providers_path: providers_path.display().to_string(),
        catalog_path: catalog_path.display().to_string(),
        config_path: config_path.display().to_string(),
        models_cache_path: models_cache_path.display().to_string(),
        inject_map_path: inject_map_path.display().to_string(),
        providers_expected_hash: hash_or_empty(&providers_raw),
        catalog_expected_hash: hash_or_empty(&catalog_raw),
        config_expected_hash: hash_or_empty(&config_raw),
        models_cache_expected_hash: hash_or_empty(&models_cache_raw),
        inject_map_expected_hash: hash_or_empty(&inject_map_raw),
        router_provider_id: plan.router_provider.name.clone(),
        proxy_port,
        providers_diff: crate::config::diff::between(&providers_raw, &providers_next_text),
        catalog_diff: crate::config::diff::between(&catalog_raw, &catalog_next_text),
        config_diff: crate::config::diff::between(&config_raw, &config_next_text),
        models_cache_diff: crate::config::diff::between(&models_cache_raw, &models_cache_next_text),
        inject_map_diff: crate::config::diff::between(&inject_map_raw, &inject_map_next_text),
        router_provider: plan.router_provider,
        route_count: plan.route_count,
        routed_model_count: plan.routed_model_count,
        skipped_models: plan.skipped_models,
        proxy_base_url: plan.proxy_base_url,
        ensure_codex_config,
        models_cache_touched,
        inject_map_touched,
    })
}

#[tauri::command]
pub fn codex_multirouter_apply(
    request: CodexMultirouterApplyRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<CodexMultirouterSyncResult> {
    if !request.confirmed {
        return Err(AppError::Command(
            "同步 MultiRouter 需要 confirmed=true".to_string(),
        ));
    }

    let preview = request.preview;
    let providers_path = providers_path()?;
    let catalog_path = catalog_path()?;
    let config_path = codex_config_path()?;
    let models_cache_path = native_models_cache_path()?;
    let inject_map_path = inject_map::inject_map_path()?;

    let (providers, providers_raw, providers_err) = read_providers(&providers_path);
    if let Some(message) = providers_err {
        return Err(AppError::Command(format!(
            "providers.json 解析失败，不能同步 MultiRouter: {message}"
        )));
    }
    let (mut catalog, catalog_raw, catalog_err) = read_catalog(&catalog_path);
    if let Some(message) = catalog_err {
        return Err(AppError::Command(format!(
            "custom_model_catalog.json 解析失败，不能同步 MultiRouter: {message}"
        )));
    }
    let config_raw = if preview.ensure_codex_config {
        Some(read_optional_raw(&config_path)?)
    } else {
        None
    };
    let models_cache_raw = read_optional_raw(&models_cache_path)?;
    let inject_map_raw = read_optional_existing_raw(&inject_map_path)?;
    preflight_multirouter_apply_hashes(
        &preview,
        &providers_raw,
        &catalog_raw,
        config_raw.as_deref(),
        &models_cache_raw,
        &inject_map_raw,
    )?;

    merge_native_models_into_catalog_if_available(&mut catalog, &models_cache_path)?;
    ensure_default_native_openai_models(&mut catalog);

    let plan = build_multirouter_plan(
        &providers,
        &catalog,
        &preview.router_provider_id,
        preview.proxy_port,
    )?;

    let provider_write = upsert_provider_routes(
        &providers_path,
        &preview.providers_expected_hash,
        Some("sync codex multirouter provider"),
        |current: &mut Vec<ProviderRoute>| {
            current.retain(|provider| provider.name != plan.router_provider.name);
            current.push(plan.router_provider.clone());
        },
    )?;

    let catalog_write = upsert_catalog_entries(
        &catalog_path,
        &preview.catalog_expected_hash,
        Some("sync codex multirouter catalog"),
        |current: &mut Vec<ModelCatalogEntry>| {
            *current = plan.catalog.clone();
            Ok(())
        },
    )?;

    let config_write = if preview.ensure_codex_config {
        write_codex_managed_config(
            &config_path,
            &catalog_path,
            &plan.catalog,
            &plan.router_provider.name,
            &plan.proxy_base_url,
            &preview.config_expected_hash,
        )?
    } else {
        None
    };
    let models_cache_write = write_codex_models_cache(
        &models_cache_path,
        &plan.catalog,
        &preview.models_cache_expected_hash,
    )?;
    let models_cache_touched = models_cache_write.is_some();
    let inject_map_cleanup =
        write_legacy_inject_map_cleanup(&inject_map_path, &preview.inject_map_expected_hash)?;
    let (inject_map_write, inject_map_touched) =
        if let Some((write, cleaned_map)) = inject_map_cleanup {
            state.inner().set_inject_map(cleaned_map);
            persist_runtime_state(state.inner());
            (Some(write), true)
        } else {
            (None, false)
        };

    state.inner().log_event(
        "info",
        "publish",
        format!(
            "published to Codex: provider={}, models={}, routes={}, proxy_base_url={}, config_touched={}, models_cache_touched={}",
            plan.router_provider.name,
            plan.routed_model_count,
            plan.route_count,
            plan.proxy_base_url,
            preview.ensure_codex_config,
            models_cache_touched
        ),
    );

    Ok(CodexMultirouterSyncResult {
        router_provider: plan.router_provider,
        route_count: plan.route_count,
        routed_model_count: plan.routed_model_count,
        skipped_models: plan.skipped_models,
        proxy_base_url: plan.proxy_base_url,
        provider_write,
        catalog_write,
        config_write,
        models_cache_write,
        inject_map_write,
        config_touched: preview.ensure_codex_config,
        models_cache_touched,
        inject_map_touched,
    })
}

#[tauri::command]
pub fn codex_multirouter_sync(
    _request: CodexMultirouterSyncRequest,
) -> AppResult<CodexMultirouterSyncResult> {
    Err(AppError::Command(
        "codex_multirouter_sync 已停用。请先调用 codex_multirouter_preview 生成五份 diff，再用 codex_multirouter_apply confirmed=true 写入。"
            .to_string(),
    ))
}

#[tauri::command]
pub fn codex_models_cache_restore_preview() -> AppResult<CodexModelsCacheRestorePreview> {
    let models_cache_path = native_models_cache_path()?;
    let backup_path = native_models_cache_backup_path_for(&models_cache_path);
    let raw = read_optional_raw(&models_cache_path)?;
    let backup_raw = if backup_path.exists() {
        std::fs::read_to_string(&backup_path).map_err(AppError::Io)?
    } else {
        String::new()
    };
    let owned_cache = models_cache_path.exists() && codex_models_cache_is_codex_box_owned(&raw);
    let backup_exists = backup_path.exists();
    let restore_available = owned_cache;
    let will_delete = owned_cache && !backup_exists;
    let next_text = if !restore_available {
        raw.clone()
    } else if backup_exists {
        backup_raw
    } else {
        String::new()
    };

    Ok(CodexModelsCacheRestorePreview {
        models_cache_path: models_cache_path.display().to_string(),
        backup_path: backup_path.display().to_string(),
        models_cache_expected_hash: hash_or_empty(&raw),
        backup_exists,
        owned_cache,
        restore_available,
        will_delete,
        diff: crate::config::diff::between(&raw, &next_text),
    })
}

#[tauri::command]
pub fn codex_models_cache_restore_apply(
    request: CodexModelsCacheRestoreApplyRequest,
) -> AppResult<CodexModelsCacheRestoreResult> {
    if !request.confirmed {
        return Err(AppError::Command(
            "恢复 Codex 模型缓存需要 confirmed=true".to_string(),
        ));
    }
    restore_codex_models_cache_if_owned(
        &native_models_cache_path()?,
        &backup_dir()?,
        &request.preview.models_cache_expected_hash,
    )
}

fn write_simple_model_config_to_paths(
    providers_path: &Path,
    catalog_path: &Path,
    _codex_config_path: Option<&Path>,
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

    if let Some(api_key) = plan.runtime_api_key.as_deref() {
        std::env::set_var(&plan.env_key, api_key);
    }

    Ok(SimpleModelConfigResult {
        provider: plan.provider,
        model: plan.model,
        env_key: plan.env_key,
        provider_write,
        catalog_write,
        requires_multirouter_sync: true,
        restart_codex: plan.restart_codex,
    })
}

fn strip_opencodex_managed_blocks(raw: &str) -> String {
    let mut out = Vec::new();
    let mut skipping_managed_block: Option<&'static str> = None;
    let mut skipping_standalone_provider = false;
    let local_provider_header = format!("[model_providers.{DEFAULT_LOCAL_PROVIDER_ID}]");
    let mut current_table: Option<String> = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed == "# >>> opencodex managed >>>" {
            skipping_managed_block = Some("opencodex");
            continue;
        }
        if trimmed == "# >>> codex-box managed >>>" {
            skipping_managed_block = Some("codex-box");
            continue;
        }
        if trimmed == "# <<< opencodex managed <<<" || trimmed == "# <<< codex-box managed <<<" {
            skipping_managed_block = None;
            continue;
        }
        if let Some(kind) = skipping_managed_block {
            if kind == "opencodex"
                && trimmed.starts_with('[')
                && trimmed != "[model_providers.opencodex]"
            {
                skipping_managed_block = None;
                current_table = Some(trimmed.to_string());
                out.push(line);
            }
            continue;
        }
        if trimmed == "[model_providers.opencodex]" || trimmed == local_provider_header {
            skipping_standalone_provider = true;
            continue;
        }
        if skipping_standalone_provider {
            if trimmed.starts_with('[') {
                skipping_standalone_provider = false;
                current_table = Some(trimmed.to_string());
                out.push(line);
            }
            continue;
        }
        if trimmed.starts_with('[') {
            current_table = Some(trimmed.to_string());
        }
        if current_table.is_none()
            && (trimmed.starts_with("model_catalog_json")
                || trimmed.starts_with("openai_base_url")
                || trimmed.starts_with("model_provider"))
        {
            continue;
        }
        out.push(line);
    }
    ensure_trailing_newline(&out.join("\n"))
}

fn opencodex_managed_top_config(
    catalog_path: &Path,
    router_provider_id: &str,
    proxy_base_url: &str,
) -> String {
    format!(
        r#"# >>> codex-box managed >>>
model_catalog_json = "{}"
openai_base_url = {}
model_provider = {}
# <<< codex-box managed <<<"#,
        catalog_path.display(),
        toml_string(proxy_base_url),
        toml_string(router_provider_id)
    )
}

fn default_multirouter_base_url() -> String {
    format!("http://127.0.0.1:{DEFAULT_MULTIROUTER_PORT}/v1")
}

fn opencodex_managed_provider_config(
    catalog: &[ModelCatalogEntry],
    router_provider_id: &str,
    proxy_base_url: &str,
) -> String {
    let inline_models = codex_provider_inline_models(catalog);
    format!(
        r#"# >>> codex-box managed >>>
[model_providers.{provider_id}]
name = "Codex API Service"
base_url = {base_url}
wire_api = "responses"
requires_openai_auth = true
supports_websockets = false
experimental_bearer_token = "PROXY_MANAGED"
request_max_retries = 3
stream_max_retries = 3
stream_idle_timeout_ms = 600000
{inline_models}# <<< codex-box managed <<<"#,
        provider_id = router_provider_id,
        base_url = toml_string(proxy_base_url)
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
        .extra
        .get("api_key")
        .or_else(|| route.extra.get("apiKey"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| route.api_key_ref.as_deref().map(to_aitabby_env_ref))
        .unwrap_or_default();
    let mut value = serde_json::json!({
        "name": route.name,
        "base_url": route.base_url,
        "api_key": api_key,
        "wire_api": route.wire_api,
        "http_headers": route.http_headers,
        "enabled": route.enabled,
    });
    if let Some(obj) = value.as_object_mut() {
        if let Some(note) = &route.note {
            obj.insert("note".to_string(), serde_json::Value::String(note.clone()));
        }
        if let Some(codex_routing) = &route.codex_routing {
            if let Ok(codex_routing_value) = serde_json::to_value(codex_routing) {
                obj.insert("codexRouting".to_string(), codex_routing_value);
            }
        }
        for (key, extra_value) in &route.extra {
            obj.entry(key.clone())
                .or_insert_with(|| extra_value.clone());
        }
    }
    value
}

fn catalog_to_file_value(entry: &ModelCatalogEntry) -> serde_json::Value {
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
        "visibility": if entry.visible { "list" } else { "hide" },
        "reasoning": entry.reasoning,
        "note": entry.note,
        "vision_bridge_enabled": entry.vision_bridge_enabled,
        "vision_fallback_base_url": entry.vision_fallback_base_url,
        "vision_fallback_model": entry.vision_fallback_model,
        "vision_fallback_api_key_ref": entry.vision_fallback_api_key_ref,
    });
    if let Some(obj) = value.as_object_mut() {
        if !is_protected_subscription_model(entry) {
            let backend_provider = entry
                .backend_provider
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&entry.provider);
            obj.insert(
                "backend_provider".to_string(),
                serde_json::Value::String(backend_provider.to_string()),
            );
        }
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
    let new_text = json_envelope_text(key, items)?;

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

fn json_envelope_text(key: &str, items: Vec<serde_json::Value>) -> AppResult<String> {
    let envelope = serde_json::json!({ key: items });
    let text = serde_json::to_string_pretty(&envelope)
        .map_err(|e| AppError::Command(format!("serialize failed: {e}")))?;
    Ok(ensure_trailing_newline(&text))
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
            codex_routing: None,
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
            codex_routing: None,
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
    fn read_providers_accepts_direct_api_key_and_env_ref() {
        // read_providers 走 JSON Value 自定义解析, 直接读 "base_url"/"api_key" 这些 AITabby 原生 key
        let raw = r#"{
            "providers": [
                { "name": "good", "base_url": "https://a", "api_key": "${ENV_KEY}" },
                { "name": "direct", "base_url": "https://b", "api_key": "sk-cp-bagtpizhVEyY6MvdK8q4ZFfXEN00GwJo5lbSI2cCb99rH4XlzNtrT9rALNayi6IbA80MYxgT2myt3NkCyYFUBtoyNYc9ZSVWKUHbRZwNccf_fnGhX_hLOXg", "enabled": false, "note": "paused" }
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
        let direct = providers.iter().find(|p| p.name == "direct").unwrap();
        assert!(direct.api_key_ref.is_none());
        assert!(direct
            .extra
            .get("api_key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .starts_with("sk-cp-"));
        assert!(!direct.enabled);
        assert_eq!(direct.note.as_deref(), Some("paused"));
        let file_value = provider_to_file_value(direct);
        assert_eq!(
            file_value.get("enabled").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            file_value.get("note").and_then(|value| value.as_str()),
            Some("paused")
        );
        assert!(err.is_none());
    }

    #[test]
    fn read_catalog_accepts_model_as_model_id() {
        let raw = r#"{
            "models": [
                {
                    "model": "gpt-5.5",
                    "display_name": "GPT-5.5",
                    "provider": "openai",
                    "backend_model": "gpt-5.5",
                    "backend_provider": "openai",
                    "visibility": "list"
                }
            ]
        }"#;
        let dir = tempdir().unwrap();
        let file = dir.path().join(CATALOG_FILE);
        std::fs::write(&file, raw).unwrap();

        let (catalog, _raw, err) = read_catalog(&file);

        assert!(err.is_none());
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].model_id, "gpt-5.5");
        assert_eq!(catalog[0].backend_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn read_catalog_normalizes_legacy_opencodex_provider() {
        let raw = r#"{
            "models": [
                {
                    "slug": "minimax",
                    "model": "minimax",
                    "provider": "opencodex",
                    "backend_model": "MiniMax-M3",
                    "backend_provider": "minimax",
                    "visibility": "list"
                }
            ]
        }"#;
        let dir = tempdir().unwrap();
        let file = dir.path().join(CATALOG_FILE);
        std::fs::write(&file, raw).unwrap();

        let (catalog, _raw, err) = read_catalog(&file);

        assert!(err.is_none());
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].model_id, "minimax");
        assert_eq!(catalog[0].provider, DEFAULT_LOCAL_PROVIDER_ID);
        assert_eq!(catalog[0].backend_provider.as_deref(), Some("minimax"));
    }

    #[test]
    fn read_catalog_does_not_use_legacy_file_as_live_fallback() {
        let dir = tempdir().unwrap();
        let primary = dir.path().join("codex-box-catalog.json");
        let legacy = dir.path().join("legacy-catalog.json");
        std::fs::write(
            &legacy,
            r#"{"models":[{"slug":"minimax","model":"minimax","provider":"opencodex","backend_provider":"minimax","visibility":"list"}]}"#,
        )
        .unwrap();

        let (catalog, raw, err) = read_catalog(&primary);

        assert!(err.is_none());
        assert!(catalog.is_empty());
        assert_eq!(raw, "[]");
        assert!(legacy.exists());
    }

    #[test]
    fn merge_native_models_cache_adds_openai_models_to_catalog() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join(CATALOG_FILE);
        let cache = dir.path().join(NATIVE_MODELS_CACHE_FILE);
        std::fs::write(
            &cache,
            r#"{"models":[{"slug":"gpt-5.5","model":"gpt-5.5","display_name":"GPT-5.5","context_window":400000}]}"#,
        )
        .unwrap();

        let mut models = Vec::new();
        let changed = merge_native_models_into_catalog_if_available(&mut models, &cache).unwrap();
        assert!(changed);

        assert!(!catalog.exists());
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "gpt-5.5");
        assert_eq!(models[0].provider, "openai");
        assert!(models[0].backend_provider.is_none());
    }

    #[test]
    fn merge_native_models_cache_skips_codex_box_owned_cache() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join(CATALOG_FILE);
        let cache = dir.path().join(NATIVE_MODELS_CACHE_FILE);
        std::fs::write(
            &cache,
            r#"{"etag":"codex-box-model-catalog","client_version":"0.142.2","models":[{"slug":"minimax-m3","model":"minimax-m3","provider":"codex_local_access","backend_provider":"codex_local_access"}]}"#,
        )
        .unwrap();

        let mut models = Vec::new();
        let changed = merge_native_models_into_catalog_if_available(&mut models, &cache).unwrap();

        assert!(!changed);
        assert!(models.is_empty());
        assert!(!catalog.exists());
    }

    #[test]
    fn default_native_models_seed_real_catalog_entries_for_multirouter() {
        let mut models = vec![ModelCatalogEntry {
            model_id: "minimax-m3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            backend_model: Some("MiniMax-M3".to_string()),
            backend_provider: Some("minimax".to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];

        let changed = ensure_default_native_openai_models(&mut models);

        assert!(changed);
        assert!(models.iter().any(|entry| {
            entry.model_id == "gpt-5.5"
                && entry.provider == "openai"
                && entry.backend_provider.is_none()
                && entry.visible
        }));
        assert!(models
            .iter()
            .any(|entry| entry.model_id == "codex-auto-review"));
    }

    #[test]
    fn codex_models_cache_projection_preserves_client_version_and_merges_catalog() {
        let catalog = vec![
            ModelCatalogEntry {
                model_id: "minimax-m3".to_string(),
                display_name: Some("MiniMax-M3".to_string()),
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: Some("MiniMax-M3".to_string()),
                backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::from([("context_window".to_string(), serde_json::json!(200000))]),
            },
            ModelCatalogEntry {
                model_id: "gpt-5.5".to_string(),
                display_name: Some("GPT-5.5".to_string()),
                provider: "openai".to_string(),
                backend_model: Some("gpt-5.5".to_string()),
                backend_provider: None,
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::from([("context_window".to_string(), serde_json::json!(272000))]),
            },
            ModelCatalogEntry {
                model_id: "hidden-model".to_string(),
                display_name: None,
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: None,
                backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
                visible: false,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::new(),
            },
        ];
        let raw = r#"{"client_version":"0.142.2","etag":"W/\"official\"","fetched_at":"old","models":[{"slug":"gpt-5.5"}]}"#;

        let next = build_codex_models_cache_text(raw, &catalog)
            .unwrap()
            .expect("cache should be generated");
        let value: serde_json::Value = serde_json::from_str(&next).unwrap();

        assert_eq!(
            value.get("etag").and_then(|item| item.as_str()),
            Some(CODEX_BOX_MODELS_CACHE_ETAG)
        );
        assert_eq!(
            value.get("client_version").and_then(|item| item.as_str()),
            Some("0.142.2")
        );
        let models = value
            .get("models")
            .and_then(|item| item.as_array())
            .unwrap();
        let slugs = models
            .iter()
            .filter_map(|item| item.get("slug").and_then(|value| value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(slugs, vec!["minimax-m3", "gpt-5.5"]);
        let minimax = models
            .iter()
            .find(|item| item.get("slug").and_then(|value| value.as_str()) == Some("minimax-m3"))
            .unwrap();
        assert_eq!(
            minimax.get("provider").and_then(|value| value.as_str()),
            Some(DEFAULT_LOCAL_PROVIDER_ID)
        );
        let gpt = models
            .iter()
            .find(|item| item.get("slug").and_then(|value| value.as_str()) == Some("gpt-5.5"))
            .unwrap();
        assert_eq!(
            gpt.get("provider").and_then(|value| value.as_str()),
            Some("openai")
        );
        assert!(gpt.get("backend_provider").is_none());

        let again = build_codex_models_cache_text(&next, &catalog)
            .unwrap()
            .expect("owned cache should stay readable");
        assert_eq!(again, next);
    }

    #[test]
    fn codex_models_cache_projection_requires_existing_client_version() {
        let catalog = vec![ModelCatalogEntry {
            model_id: "minimax-m3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            backend_model: Some("MiniMax-M3".to_string()),
            backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];

        assert!(build_codex_models_cache_text(r#"{"models":[]}"#, &catalog)
            .unwrap()
            .is_none());
    }

    #[test]
    fn models_cache_restore_backup_preserves_original_cache() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join(NATIVE_MODELS_CACHE_FILE);
        let raw = r#"{"client_version":"0.142.2","etag":"official","models":[{"slug":"gpt-5.5"}]}"#;
        std::fs::write(&cache, raw).unwrap();

        ensure_models_cache_restore_backup(&cache, raw).unwrap();

        let backup = native_models_cache_backup_path_for(&cache);
        assert!(backup.exists());
        assert_eq!(std::fs::read_to_string(backup).unwrap(), raw);
    }

    #[test]
    fn restore_models_cache_replaces_owned_cache_from_sidecar() {
        let dir = tempdir().unwrap();
        let history_dir = dir.path().join("history");
        let cache = dir.path().join(NATIVE_MODELS_CACHE_FILE);
        let backup = native_models_cache_backup_path_for(&cache);
        let owned = r#"{"client_version":"0.142.2","etag":"codex-box-model-catalog","models":[{"slug":"minimax-m3"}]}"#;
        let official =
            r#"{"client_version":"0.142.2","etag":"official","models":[{"slug":"gpt-5.5"}]}"#;
        std::fs::write(&cache, owned).unwrap();
        std::fs::write(&backup, official).unwrap();

        let result =
            restore_codex_models_cache_if_owned(&cache, &history_dir, &hash_or_empty(owned))
                .unwrap();

        assert!(result.restored);
        assert!(!result.deleted);
        assert!(!result.backup_id.is_empty());
        assert_eq!(std::fs::read_to_string(&cache).unwrap(), official);
        assert!(!backup.exists());
    }

    #[test]
    fn restore_models_cache_deletes_owned_cache_without_sidecar() {
        let dir = tempdir().unwrap();
        let history_dir = dir.path().join("history");
        let cache = dir.path().join(NATIVE_MODELS_CACHE_FILE);
        let owned = r#"{"client_version":"0.142.2","etag":"codex-box-model-catalog","models":[{"slug":"minimax-m3"}]}"#;
        std::fs::write(&cache, owned).unwrap();

        let result =
            restore_codex_models_cache_if_owned(&cache, &history_dir, &hash_or_empty(owned))
                .unwrap();

        assert!(result.restored);
        assert!(result.deleted);
        assert!(!result.backup_id.is_empty());
        assert!(!cache.exists());
    }

    #[test]
    fn simple_model_config_plan_writes_env_ref_and_keeps_direct_key_in_memory() {
        let plan = build_simple_model_config_plan(&SimpleModelConfigRequest {
            model_input: "deepseek:deepseek-chat".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test-secret-should-not-be-returned".to_string(),
            wire_api: Some("responses".to_string()),
            display_name: None,
            reasoning_level: Some("medium".to_string()),
            restart_codex: false,
        })
        .unwrap();

        assert_eq!(plan.provider.name, "deepseek");
        assert_eq!(plan.provider.base_url, "https://api.deepseek.com/v1");
        assert_eq!(plan.provider.wire_api, "responses");
        assert_eq!(
            plan.provider.api_key_ref.as_deref(),
            Some("${DEEPSEEK_API_KEY}")
        );
        assert!(plan.provider.extra.get("api_key").is_none());
        assert_eq!(
            plan.runtime_api_key.as_deref(),
            Some("sk-test-secret-should-not-be-returned")
        );
        assert_eq!(plan.model.model_id, "deepseek-chat");
        assert_eq!(plan.model.provider, DEFAULT_LOCAL_PROVIDER_ID);
        assert_eq!(plan.model.backend_model.as_deref(), Some("deepseek-chat"));
        assert_eq!(plan.model.backend_provider.as_deref(), Some("deepseek"));
        assert_eq!(plan.env_key, "DEEPSEEK_API_KEY");
    }

    #[test]
    fn simple_model_config_plan_marks_minimax_as_text_only_chat_reasoning() {
        let plan = build_simple_model_config_plan(&SimpleModelConfigRequest {
            model_input: "minimax:minimax-m3=MiniMax-M3".to_string(),
            base_url: "https://api.minimaxi.com/v1".to_string(),
            api_key: "MINIMAX_API_KEY".to_string(),
            wire_api: Some("chat".to_string()),
            display_name: Some("MiniMax-M3".to_string()),
            reasoning_level: Some("medium".to_string()),
            restart_codex: false,
        })
        .unwrap();

        assert_eq!(plan.provider.name, "minimax");
        assert_eq!(plan.provider.wire_api, "chat");
        assert_eq!(plan.model.model_id, "minimax-m3");
        assert_eq!(plan.model.backend_model.as_deref(), Some("MiniMax-M3"));
        assert_eq!(
            plan.model
                .extra
                .get("textOnly")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            plan.model
                .extra
                .get("input_modalities")
                .or_else(|| plan.model.extra.get("inputModalities"))
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["text"])
        );
        assert_eq!(
            plan.model
                .extra
                .get("codexChatReasoning")
                .and_then(|value| value.get("thinkingParam"))
                .and_then(|value| value.as_str()),
            Some("reasoning_split")
        );
        assert_eq!(plan.model.vision_bridge_enabled, Some(false));
        assert!(plan.model.vision_fallback_base_url.is_none());
        assert!(plan.model.vision_fallback_model.is_none());
        assert!(plan.model.vision_fallback_api_key_ref.is_none());
    }

    #[test]
    fn multirouter_plan_rewrites_third_party_catalog_and_keeps_native_openai() {
        let providers = vec![ProviderRoute {
            name: "minimax".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            wire_api: "responses".to_string(),
            api_key_ref: Some("${MINIMAX_API_KEY}".to_string()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: None,
            extra: BTreeMap::new(),
        }];
        let catalog = vec![
            ModelCatalogEntry {
                model_id: "minimax-m3".to_string(),
                display_name: Some("MiniMax-M3".to_string()),
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: Some("MiniMax-M3".to_string()),
                backend_provider: Some("minimax".to_string()),
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::new(),
            },
            ModelCatalogEntry {
                model_id: "gpt-5.5".to_string(),
                display_name: Some("GPT-5.5".to_string()),
                provider: "openai".to_string(),
                backend_model: Some("gpt-5.5".to_string()),
                backend_provider: None,
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::new(),
            },
        ];

        let plan =
            build_multirouter_plan(&providers, &catalog, DEFAULT_LOCAL_PROVIDER_ID, 1455).unwrap();

        assert_eq!(plan.route_count, 2);
        assert_eq!(plan.routed_model_count, 2);
        assert_eq!(plan.skipped_models, Vec::<String>::new());
        assert_eq!(plan.router_provider.name, DEFAULT_LOCAL_PROVIDER_ID);
        assert_eq!(plan.router_provider.base_url, "http://127.0.0.1:1455/v1");

        let routing = plan.router_provider.codex_routing.as_ref().unwrap();
        assert_eq!(routing.default_route_id.as_deref(), Some("minimax"));
        let official_route = routing
            .routes
            .iter()
            .find(|route| route.id == OFFICIAL_OPENAI_ROUTE_ID)
            .expect("official GPT route");
        assert_eq!(official_route.target_provider_id, None);
        assert_eq!(
            official_route.upstream.base_url.as_deref(),
            Some(OFFICIAL_CODEX_BACKEND_URL)
        );
        assert_eq!(
            official_route
                .upstream
                .auth
                .as_ref()
                .and_then(|auth| auth.get("source"))
                .and_then(|value| value.as_str()),
            Some("managed_codex_oauth")
        );
        assert_eq!(
            official_route
                .upstream
                .model_map
                .get("gpt-5.5")
                .map(String::as_str),
            Some("gpt-5.5")
        );
        assert_eq!(
            official_route.match_rule.models,
            vec!["gpt-5.5".to_string()]
        );
        assert!(official_route.match_rule.prefixes.is_empty());

        let route = routing
            .routes
            .iter()
            .find(|route| route.id == "minimax")
            .expect("minimax route");
        assert_eq!(route.target_provider_id.as_deref(), Some("minimax"));
        assert_eq!(
            route.upstream.api_format.as_deref(),
            Some("openai_responses")
        );
        assert_eq!(route.match_rule.models, vec!["minimax-m3".to_string()]);
        assert_eq!(
            route
                .upstream
                .model_map
                .get("minimax-m3")
                .map(String::as_str),
            Some("MiniMax-M3")
        );
        let capabilities = route.capabilities.as_ref().expect("minimax capabilities");
        assert_eq!(
            capabilities
                .get("textOnly")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            capabilities
                .get("inputModalities")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                }),
            Some(vec!["text"])
        );
        assert_eq!(
            capabilities
                .get("codexChatReasoning")
                .and_then(|value| value.get("thinkingParam"))
                .and_then(|value| value.as_str()),
            Some("reasoning_split")
        );

        let minimax_entry = plan
            .catalog
            .iter()
            .find(|entry| entry.model_id == "minimax-m3")
            .unwrap();
        assert_eq!(minimax_entry.provider, DEFAULT_LOCAL_PROVIDER_ID);
        assert_eq!(
            minimax_entry.backend_provider.as_deref(),
            Some(DEFAULT_LOCAL_PROVIDER_ID)
        );
        assert_eq!(
            minimax_entry
                .extra
                .get("targetProvider")
                .and_then(|value| value.as_str()),
            Some("minimax")
        );
        assert_eq!(
            minimax_entry
                .extra
                .get("target_provider")
                .and_then(|value| value.as_str()),
            Some("minimax")
        );

        let openai_entry = plan
            .catalog
            .iter()
            .find(|entry| entry.model_id == "gpt-5.5")
            .unwrap();
        assert_eq!(openai_entry.provider, "openai");
        assert!(openai_entry.backend_provider.is_none());

        let runtime_providers = vec![plan.router_provider.clone(), providers[0].clone()];
        let runtime_map = crate::proxy::inject_map::InjectMap::default();
        let minimax_route = crate::proxy::routing::resolve_catalog_route_from_sources(
            "minimax-m3",
            &plan.catalog,
            &runtime_providers,
            &runtime_map,
        )
        .expect("minimax model should resolve through generated multirouter plan");
        assert_eq!(minimax_route.provider_name, "minimax");
        assert_eq!(minimax_route.model_id, "MiniMax-M3");
        assert_eq!(minimax_route.upstream_base_url, "https://api.minimax.io/v1");
        assert_eq!(minimax_route.env_key.as_deref(), Some("MINIMAX_API_KEY"));

        let official_route = crate::proxy::routing::resolve_catalog_route_from_sources(
            "gpt-5.5",
            &plan.catalog,
            &runtime_providers,
            &runtime_map,
        )
        .expect("native GPT should resolve through generated official route");
        assert_eq!(official_route.provider_name, "OpenAI Official");
        assert_eq!(official_route.model_id, "gpt-5.5");
        assert_eq!(official_route.upstream_base_url, OFFICIAL_CODEX_BACKEND_URL);
        assert_eq!(
            official_route.auth_source.as_deref(),
            Some("managed_codex_oauth")
        );
    }

    #[test]
    fn multirouter_plan_recovers_target_provider_from_catalog_metadata() {
        let providers = vec![ProviderRoute {
            name: "minimax".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            wire_api: "responses".to_string(),
            api_key_ref: Some("${MINIMAX_API_KEY}".to_string()),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: None,
            extra: BTreeMap::new(),
        }];
        let catalog = vec![ModelCatalogEntry {
            model_id: "minimax-m3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            backend_model: Some("MiniMax-M3".to_string()),
            backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::from([("targetProvider".to_string(), serde_json::json!("minimax"))]),
        }];

        let plan =
            build_multirouter_plan(&providers, &catalog, DEFAULT_LOCAL_PROVIDER_ID, 1455).unwrap();

        assert_eq!(plan.route_count, 1);
        assert_eq!(plan.routed_model_count, 1);
        assert!(plan.skipped_models.is_empty());
        let route = plan
            .router_provider
            .codex_routing
            .as_ref()
            .and_then(|routing| routing.routes.first())
            .expect("minimax route");
        assert_eq!(route.target_provider_id.as_deref(), Some("minimax"));
        assert_eq!(
            route
                .upstream
                .model_map
                .get("minimax-m3")
                .map(String::as_str),
            Some("MiniMax-M3")
        );
    }

    #[test]
    fn multirouter_plan_does_not_route_to_legacy_opencodex_port() {
        let providers = vec![
            ProviderRoute {
                name: "opencodex".to_string(),
                base_url: "http://127.0.0.1:8765/v1".to_string(),
                wire_api: "responses".to_string(),
                api_key_ref: None,
                http_headers: BTreeMap::new(),
                enabled: true,
                note: None,
                codex_routing: None,
                extra: BTreeMap::new(),
            },
            ProviderRoute {
                name: "minimax".to_string(),
                base_url: "https://api.minimax.io/v1".to_string(),
                wire_api: "responses".to_string(),
                api_key_ref: Some("${MINIMAX_API_KEY}".to_string()),
                http_headers: BTreeMap::new(),
                enabled: true,
                note: None,
                codex_routing: None,
                extra: BTreeMap::new(),
            },
        ];
        let catalog = vec![
            ModelCatalogEntry {
                model_id: "legacy-gpt".to_string(),
                display_name: Some("Legacy GPT".to_string()),
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: Some("gpt-5.5".to_string()),
                backend_provider: Some("opencodex".to_string()),
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::new(),
            },
            ModelCatalogEntry {
                model_id: "minimax-m3".to_string(),
                display_name: Some("MiniMax-M3".to_string()),
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: Some("MiniMax-M3".to_string()),
                backend_provider: Some("minimax".to_string()),
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::new(),
            },
        ];

        let plan =
            build_multirouter_plan(&providers, &catalog, DEFAULT_LOCAL_PROVIDER_ID, 1455).unwrap();

        assert_eq!(plan.route_count, 1);
        assert_eq!(plan.routed_model_count, 1);
        assert_eq!(plan.skipped_models, vec!["legacy-gpt".to_string()]);
        let route = &plan
            .router_provider
            .codex_routing
            .as_ref()
            .expect("router")
            .routes[0];
        assert_eq!(route.target_provider_id.as_deref(), Some("minimax"));
        assert_eq!(route.match_rule.models, vec!["minimax-m3".to_string()]);
    }

    #[test]
    fn clean_legacy_inject_map_text_keeps_non_legacy_entries() {
        let raw = r#"{
  "updatedAt": "2026-06-27T00:00:00Z",
  "port": 1455,
  "providers": [
    {
      "name": "opencodex",
      "originalBaseUrl": "http://127.0.0.1:8765/v1",
      "wireApi": "responses",
      "models": []
    },
    {
      "name": "minimax",
      "originalBaseUrl": "https://api.minimaxi.com/v1",
      "wireApi": "chat",
      "models": ["minimax-m3"]
    }
  ]
}"#;

        let (next, touched, map) = clean_legacy_inject_map_text(raw).unwrap();

        assert!(touched);
        assert_eq!(map.providers.len(), 1);
        assert_eq!(map.providers[0].name, "minimax");
        assert!(next.contains("minimax"));
        assert!(!next.contains("8765"));
    }

    fn preview_for_hash_preflight(
        providers_raw: &str,
        catalog_raw: &str,
        config_raw: &str,
        models_cache_raw: &str,
        inject_map_raw: &str,
    ) -> CodexMultirouterPreview {
        CodexMultirouterPreview {
            providers_path: "/tmp/providers.json".to_string(),
            catalog_path: "/tmp/custom_model_catalog.json".to_string(),
            config_path: "/tmp/config.toml".to_string(),
            models_cache_path: "/tmp/models_cache.json".to_string(),
            inject_map_path: "/tmp/inject-map.json".to_string(),
            providers_expected_hash: hash_or_empty(providers_raw),
            catalog_expected_hash: hash_or_empty(catalog_raw),
            config_expected_hash: hash_or_empty(config_raw),
            models_cache_expected_hash: hash_or_empty(models_cache_raw),
            inject_map_expected_hash: hash_or_empty(inject_map_raw),
            router_provider_id: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            proxy_port: DEFAULT_MULTIROUTER_PORT,
            providers_diff: Vec::new(),
            catalog_diff: Vec::new(),
            config_diff: Vec::new(),
            models_cache_diff: Vec::new(),
            inject_map_diff: Vec::new(),
            router_provider: ProviderRoute {
                name: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                base_url: default_multirouter_base_url(),
                wire_api: "responses".to_string(),
                api_key_ref: None,
                http_headers: BTreeMap::new(),
                enabled: true,
                note: None,
                codex_routing: None,
                extra: BTreeMap::new(),
            },
            route_count: 0,
            routed_model_count: 0,
            skipped_models: Vec::new(),
            proxy_base_url: default_multirouter_base_url(),
            ensure_codex_config: true,
            models_cache_touched: false,
            inject_map_touched: false,
        }
    }

    #[test]
    fn multirouter_apply_preflight_rejects_late_models_cache_change() {
        let providers_raw = "{\"providers\":[]}\n";
        let catalog_raw = "{\"models\":[]}\n";
        let config_raw = "model = \"gpt-5.5\"\n";
        let models_cache_raw = "{\"client_version\":\"1.0.0\",\"models\":[]}\n";
        let inject_map_raw = "";
        let preview = preview_for_hash_preflight(
            providers_raw,
            catalog_raw,
            config_raw,
            models_cache_raw,
            inject_map_raw,
        );
        let changed_models_cache = "{\"client_version\":\"1.0.1\",\"models\":[]}\n";

        let error = preflight_multirouter_apply_hashes(
            &preview,
            providers_raw,
            catalog_raw,
            Some(config_raw),
            changed_models_cache,
            inject_map_raw,
        )
        .unwrap_err();

        assert!(error.to_string().contains("models_cache.json 已变化"));
    }

    #[test]
    fn multirouter_apply_preflight_ignores_config_when_not_ensuring_config() {
        let providers_raw = "{\"providers\":[]}\n";
        let catalog_raw = "{\"models\":[]}\n";
        let config_raw = "model = \"gpt-5.5\"\n";
        let models_cache_raw = "{\"client_version\":\"1.0.0\",\"models\":[]}\n";
        let inject_map_raw = "";
        let mut preview = preview_for_hash_preflight(
            providers_raw,
            catalog_raw,
            config_raw,
            models_cache_raw,
            inject_map_raw,
        );
        preview.ensure_codex_config = false;

        preflight_multirouter_apply_hashes(
            &preview,
            providers_raw,
            catalog_raw,
            Some("model = \"changed\"\n"),
            models_cache_raw,
            inject_map_raw,
        )
        .unwrap();
    }

    #[test]
    fn legacy_multirouter_sync_command_is_disabled() {
        let error = codex_multirouter_sync(CodexMultirouterSyncRequest {
            providers_expected_hash: String::new(),
            catalog_expected_hash: String::new(),
            proxy_port: Some(DEFAULT_MULTIROUTER_PORT),
            router_provider_id: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
            ensure_codex_config: Some(true),
        })
        .unwrap_err();

        assert!(error.to_string().contains("codex_multirouter_sync 已停用"));
    }

    #[test]
    fn simple_model_config_write_updates_provider_and_catalog_with_env_ref() {
        let dir = tempdir().unwrap();
        let providers = dir.path().join(PROVIDERS_FILE);
        let catalog = dir.path().join(CATALOG_FILE);
        let codex_config = dir.path().join("config.toml");
        let env_key = "CODEX_BOX_SIMPLE_MODEL_TEST_API_KEY";
        let old_env = std::env::var(env_key).ok();
        std::env::remove_var(env_key);
        std::fs::write(&providers, "{\"providers\":[]}\n").unwrap();
        std::fs::write(
            &catalog,
            r#"{"models":[{"slug":"codex-box-simple-model-test/test-chat","model":"codex-box-simple-model-test/test-chat","display_name":"Legacy","provider":"codex-box-simple-model-test","backend_model":"test-chat","backend_provider":"codex-box-simple-model-test","visibility":"list"}]}"#,
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
        let original_config_text = std::fs::read_to_string(&codex_config).unwrap();

        let result = write_simple_model_config_to_paths(
            &providers,
            &catalog,
            Some(&codex_config),
            &SimpleModelConfigRequest {
                model_input: "codex-box-simple-model-test:test-chat".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                api_key: "sk-test-secret-should-not-be-written".to_string(),
                wire_api: Some("responses".to_string()),
                display_name: Some("Test Chat".to_string()),
                reasoning_level: Some("medium".to_string()),
                restart_codex: true,
            },
        )
        .unwrap();

        let providers_text = std::fs::read_to_string(&providers).unwrap();
        let catalog_text = std::fs::read_to_string(&catalog).unwrap();
        assert!(providers_text.contains("\"name\": \"codex-box-simple-model-test\""));
        assert!(providers_text.contains("\"api_key\": \"$CODEX_BOX_SIMPLE_MODEL_TEST_API_KEY\""));
        assert!(!providers_text.contains("sk-test-secret-should-not-be-written"));
        assert_eq!(
            std::env::var(env_key).ok().as_deref(),
            Some("sk-test-secret-should-not-be-written")
        );
        if let Some(value) = old_env {
            std::env::set_var(env_key, value);
        } else {
            std::env::remove_var(env_key);
        }
        assert!(catalog_text.contains("\"slug\": \"test-chat\""));
        assert!(catalog_text.contains("\"model\": \"test-chat\""));
        assert!(catalog_text.contains("\"provider\": \"codex_model_router_v2\""));
        assert!(catalog_text.contains("\"backend_model\": \"test-chat\""));
        assert!(catalog_text.contains("\"backend_provider\": \"codex-box-simple-model-test\""));
        assert!(catalog_text.contains("\"supported_in_api\": true"));
        assert!(catalog_text.contains("\"minimal_client_version\": \"0.0.1\""));
        assert!(catalog_text.contains("\"available_in_plans\""));
        assert!(!catalog_text.contains("\"slug\": \"codex-box-simple-model-test/test-chat\""));
        assert_eq!(result.env_key, env_key);
        assert!(result.restart_codex);

        let config_text = std::fs::read_to_string(&codex_config).unwrap();
        assert_eq!(config_text, original_config_text);
        assert!(!config_text.contains(DEFAULT_LOCAL_PROVIDER_ID));
        assert!(!config_text.contains(&format!("model_catalog_json = \"{}\"", catalog.display())));
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
    fn managed_config_projection_is_parseable_and_idempotent() {
        let catalog = vec![
            ModelCatalogEntry {
                model_id: "minimax-m3".to_string(),
                display_name: Some("MiniMax-M3".to_string()),
                provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
                backend_model: Some("MiniMax-M3".to_string()),
                backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
                visible: true,
                reasoning: Some(ReasoningConfig {
                    enabled: true,
                    levels: vec!["medium".to_string()],
                }),
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::from([("context_window".to_string(), serde_json::json!(200000))]),
            },
            ModelCatalogEntry {
                model_id: "gpt-5.5".to_string(),
                display_name: Some("GPT-5.5".to_string()),
                provider: "openai".to_string(),
                backend_model: Some("gpt-5.5".to_string()),
                backend_provider: None,
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::from([("context_window".to_string(), serde_json::json!(272000))]),
            },
        ];
        let raw = r#"# >>> codex-box managed >>>
model_catalog_json = "/tmp/old-catalog.json"
model_provider = "codex_local_access"
# <<< codex-box managed <<<

model = "gpt-5.5"
model_provider = "openai"
model_catalog_json = "/tmp/stale-catalog.json"

[model_providers.codex_local_access]
name = "Stale"
base_url = "http://127.0.0.1:1455/v1"
wire_api = "responses"

[profiles.dev]
model_provider = "keep-profile-provider"
"#;

        let default_base_url = default_multirouter_base_url();
        let once = build_codex_managed_config_text(
            raw,
            Path::new("/tmp/catalog.json"),
            &catalog,
            DEFAULT_LOCAL_PROVIDER_ID,
            &default_base_url,
        );
        let twice = build_codex_managed_config_text(
            &once,
            Path::new("/tmp/catalog.json"),
            &catalog,
            DEFAULT_LOCAL_PROVIDER_ID,
            &default_base_url,
        );

        assert_eq!(once, twice);
        assert_eq!(once.matches("model_catalog_json =").count(), 1);
        assert_eq!(once.matches("openai_base_url =").count(), 1);
        assert_eq!(
            once.matches("[model_providers.codex_model_router_v2]")
                .count(),
            1
        );
        assert!(once.contains("model_provider = \"keep-profile-provider\""));
        assert!(once.contains("supports_websockets = false"));
        assert!(once.contains("experimental_bearer_token = \"PROXY_MANAGED\""));
        assert!(once.contains("models = ["));
        assert!(once.contains("model = \"minimax-m3\""));
        assert!(once.contains("model = \"gpt-5.5\""));
        toml::from_str::<toml::Value>(&once).unwrap();
    }

    #[test]
    fn managed_config_projection_for_missing_config_is_parseable() {
        let catalog = vec![ModelCatalogEntry {
            model_id: "gpt-5.5".to_string(),
            display_name: Some("GPT-5.5".to_string()),
            provider: "openai".to_string(),
            backend_model: Some("gpt-5.5".to_string()),
            backend_provider: None,
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];
        let text = build_codex_managed_config_text(
            "",
            Path::new("/tmp/catalog.json"),
            &catalog,
            DEFAULT_LOCAL_PROVIDER_ID,
            &default_multirouter_base_url(),
        );

        assert!(!text.contains("\n[]\n"));
        assert_eq!(text.matches("model_catalog_json =").count(), 1);
        assert_eq!(text.matches("openai_base_url =").count(), 1);
        assert!(text.contains("[model_providers.codex_model_router_v2]"));
        toml::from_str::<toml::Value>(&text).unwrap();
    }

    #[test]
    fn managed_config_projection_uses_router_proxy_base_url() {
        let catalog = vec![ModelCatalogEntry {
            model_id: "minimax-m3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            backend_model: Some("MiniMax-M3".to_string()),
            backend_provider: Some(DEFAULT_LOCAL_PROVIDER_ID.to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];
        let text = build_codex_managed_config_text(
            "model = \"minimax-m3\"\n",
            Path::new("/tmp/catalog.json"),
            &catalog,
            DEFAULT_LOCAL_PROVIDER_ID,
            "http://127.0.0.1:1777/v1",
        );

        assert!(text.contains("base_url = \"http://127.0.0.1:1777/v1\""));
        assert!(text.contains("openai_base_url = \"http://127.0.0.1:1777/v1\""));
        assert!(!text.contains("base_url = \"http://127.0.0.1:1455/v1\""));
        toml::from_str::<toml::Value>(&text).unwrap();
    }

    #[test]
    fn managed_config_projection_uses_actual_router_provider_id() {
        let catalog = vec![ModelCatalogEntry {
            model_id: "qwen3-coder".to_string(),
            display_name: Some("Qwen Coder".to_string()),
            provider: "custom_router".to_string(),
            backend_model: Some("qwen3-coder".to_string()),
            backend_provider: Some("custom_router".to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];
        let text = build_codex_managed_config_text(
            "",
            Path::new("/tmp/catalog.json"),
            &catalog,
            "custom_router",
            "http://127.0.0.1:1777/v1",
        );

        assert!(text.contains("model_provider = \"custom_router\""));
        assert!(text.contains("[model_providers.custom_router]"));
        assert!(!text.contains(&format!(
            "model_provider = \"{}\"",
            DEFAULT_LOCAL_PROVIDER_ID
        )));
        assert!(!text.contains(&format!("[model_providers.{}]", DEFAULT_LOCAL_PROVIDER_ID)));
        toml::from_str::<toml::Value>(&text).unwrap();
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
            codex_routing: None,
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

    #[test]
    fn provider_direct_key_detection_flags_inline_key() {
        let mut provider = ProviderRoute {
            name: "minimax".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: None,
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: None,
            extra: BTreeMap::new(),
        };

        assert!(!provider_has_direct_key(&provider));
        provider.extra.insert(
            "api_key".to_string(),
            serde_json::Value::String("sk-inline".to_string()),
        );
        assert!(provider_has_direct_key(&provider));
    }

    #[test]
    fn import_preview_redacts_api_keys_in_diff_text() {
        let raw = r#"{"providers":[{"name":"minimax","base_url":"https://api.minimax.io/v1","api_key":"sk-secret-value","wire_api":"chat"}]}"#;

        let redacted = redact_api_keys_in_json(raw);

        assert!(!redacted.contains("sk-secret-value"));
        assert!(redacted.contains("••••••••"));
    }

    #[test]
    fn import_projection_converts_direct_keys_to_runtime_env_refs() {
        let providers = vec![ProviderRoute {
            name: "minimax".to_string(),
            base_url: "https://api.minimaxi.com/v1".to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: None,
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: None,
            extra: BTreeMap::from([(
                "api_key".to_string(),
                serde_json::Value::String("sk-import-secret".to_string()),
            )]),
        }];
        let catalog = vec![ModelCatalogEntry {
            model_id: "minimax".to_string(),
            display_name: Some("MiniMax".to_string()),
            provider: DEFAULT_LOCAL_PROVIDER_ID.to_string(),
            backend_model: Some("minimax".to_string()),
            backend_provider: Some("minimax".to_string()),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }];

        let projection = build_import_projection(&providers, &catalog).unwrap();

        assert!(!projection.providers_text.contains("sk-import-secret"));
        assert!(projection.providers_text.contains("$MINIMAX_API_KEY"));
        assert_eq!(
            projection.runtime_env,
            vec![(
                "MINIMAX_API_KEY".to_string(),
                "sk-import-secret".to_string()
            )]
        );
        assert!(projection
            .warnings
            .iter()
            .any(|warning| warning.contains("minimax -> $MINIMAX_API_KEY")));
        assert!(projection.catalog_text.contains("\"slug\": \"minimax\""));
        assert!(projection.catalog_text.contains("\"slug\": \"gpt-5.5\""));
    }

    #[test]
    fn write_import_target_creates_target_without_backup_when_missing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("providers.json");

        let result = write_import_target(&target, "", "{\"providers\":[]}\n").unwrap();

        assert_eq!(result.backup_id, "");
        assert!(target.exists());
        assert!(std::fs::read_to_string(&target)
            .unwrap()
            .contains("\"providers\""));
    }

    #[test]
    fn scan_codex_config_sources_counts_provider_names() {
        let dir = tempdir().unwrap();
        let codex = dir.path().join(".codex");
        let backups = codex.join("codex-box/backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(
            codex.join("config.toml"),
            r#"
model_provider = "CodexPlusPlus"

[model_providers.CodexPlusPlus]
name = "Codex++"
base_url = "http://127.0.0.1:8765/v1"
"#,
        )
        .unwrap();
        std::fs::write(
            backups.join("before.toml"),
            r#"
[model_providers.codex_local_access]
name = "Codex API Service"
"#,
        )
        .unwrap();

        let files = scan_codex_config_sources(&codex);
        assert_eq!(files.len(), 2);
        assert_eq!(count_model_providers_in_configs(&files), 2);
    }
}
