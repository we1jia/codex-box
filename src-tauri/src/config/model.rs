// src-tauri/src/config/model.rs
use serde::{Deserialize, Serialize};

/// 解析后的 Codex 配置（顶层 view）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexConfig {
    /// 顶层标量字段
    pub top_level: serde_json::Map<String, serde_json::Value>,
    /// profile / profiles 表
    pub profiles: Vec<ProfileEntry>,
    /// model_providers 表
    pub model_providers: Vec<ModelProviderEntry>,
    /// mcp_servers 表
    pub mcp_servers: Vec<McpServerEntry>,
    /// marketplaces 表
    pub marketplaces: Vec<MarketplaceEntry>,
    /// 其他 TOML 表
    pub other_tables: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileEntry {
    pub name: String,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox_mode: Option<String>,
    pub network: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderEntry {
    pub name: String,
    pub kind: ProviderKind,
    pub channel: ProviderChannel,
    pub base_url: Option<String>,
    pub wire_api: WireApi,
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiSubscription,
    OpenAiOfficialApi,
    OpenAiCompatibleApi,
    LocalGateway,
    CliProxyApi,
    CodexProxy,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderChannel {
    Subscription,
    Api,
    Gateway,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireApi {
    Chat,
    Responses,
    SseStream,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerEntry {
    pub name: String,
    pub kind: McpServerKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerKind {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Http {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceEntry {
    pub name: String,
    pub source_type: Option<String>,
    pub source: Option<String>,
}

/// 一次备份记录
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupRecord {
    pub id: String,
    pub created_at: String,
    pub file_path: String,
    pub reason: BackupReason,
    pub content_hash: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    Manual,
    PreWrite,
    PreRollback,
    Scheduled,
}

/// 文本 diff 一行
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub content: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Context,
    Insert,
    Delete,
}

/// Dashboard 摘要：前端 4 张指标卡的最小数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardSummary {
    pub active_profile: Option<String>,
    pub provider_count: usize,
    pub mcp_count: McpCount,
    pub network: String,
    pub last_backup_at: Option<String>,
    pub health_summary: HealthSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpCount {
    pub enabled: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthSummary {
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
}
