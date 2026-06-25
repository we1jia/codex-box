// src-tauri/src/config/parser.rs
use crate::config::model::{
    CodexConfig, DashboardSummary, HealthSummary, MarketplaceEntry, McpCount, McpServerEntry,
    McpServerKind, ModelProviderEntry, ProfileEntry, ProviderChannel, ProviderKind, WireApi,
};
use crate::error::AppResult;

/// 解析 raw TOML 文本为 CodexConfig
pub fn parse(raw: &str) -> AppResult<CodexConfig> {
    let value: toml::Value = toml::from_str(raw)?;

    let mut top_level = serde_json::Map::new();
    let mut profiles = Vec::new();
    let mut model_providers = Vec::new();
    let mut mcp_servers = Vec::new();
    let mut marketplaces = Vec::new();
    let mut other_tables = serde_json::Map::new();

    if let Some(table) = value.as_table() {
        for (key, val) in table {
            if key == "model_providers" {
                model_providers.extend(parse_model_providers(val));
            } else if key == "profile" || key == "profiles" {
                profiles.extend(parse_profiles(val));
            } else if key == "mcp_servers" {
                if let Some(t) = val.as_table() {
                    for (name, entry) in t {
                        mcp_servers.push(parse_mcp_entry(name, entry));
                    }
                }
            } else if key == "marketplaces" {
                if let Some(t) = val.as_table() {
                    for (name, entry) in t {
                        marketplaces.push(parse_marketplace(name, entry));
                    }
                }
            } else if val.is_table() {
                other_tables.insert(
                    key.clone(),
                    serde_json::to_value(val).unwrap_or(serde_json::Value::Null),
                );
            } else {
                let json_val = serde_json::to_value(val).unwrap_or(serde_json::Value::Null);
                top_level.insert(key.clone(), json_val);
            }
        }
    }

    let active_profile = top_level
        .get("active_profile")
        .or_else(|| top_level.get("profile"))
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(active_profile) = active_profile {
        for profile in &mut profiles {
            profile.is_active = profile.name == active_profile;
        }
    }

    Ok(CodexConfig {
        top_level,
        profiles,
        model_providers,
        mcp_servers,
        marketplaces,
        other_tables,
    })
}

fn parse_profiles(val: &toml::Value) -> Vec<ProfileEntry> {
    let Some(table) = val.as_table() else {
        return Vec::new();
    };

    if table.values().all(|entry| entry.as_table().is_some()) {
        table
            .iter()
            .map(|(name, entry)| parse_profile_entry(name, entry))
            .collect()
    } else {
        vec![parse_profile_entry("default", val)]
    }
}

fn parse_profile_entry(name: &str, val: &toml::Value) -> ProfileEntry {
    let table = val.as_table();
    ProfileEntry {
        name: name.to_string(),
        model: get_string(table, "model"),
        model_provider: get_string(table, "model_provider"),
        approval_policy: get_string(table, "approval_policy"),
        sandbox_mode: get_string(table, "sandbox_mode"),
        network: get_string(table, "network"),
        mcp_refs: get_string_array(table, "mcp_refs"),
        is_active: false,
    }
}

fn parse_model_providers(val: &toml::Value) -> Vec<ModelProviderEntry> {
    let Some(table) = val.as_table() else {
        return Vec::new();
    };

    if table.values().all(|entry| entry.as_table().is_some()) {
        table
            .iter()
            .map(|(name, entry)| parse_model_provider(name, entry))
            .collect()
    } else {
        vec![parse_model_provider("default", val)]
    }
}

fn parse_model_provider(name: &str, val: &toml::Value) -> ModelProviderEntry {
    let table = val.as_table();
    let base_url = get_string(table, "base_url");
    let explicit_kind = get_string(table, "kind")
        .or_else(|| get_string(table, "provider_kind"))
        .or_else(|| get_string(table, "type"));
    let kind = infer_provider_kind(name, base_url.as_deref(), explicit_kind.as_deref());
    let channel = infer_provider_channel(&kind, get_string(table, "channel").as_deref());
    let wire_api = get_string(table, "wire_api")
        .or_else(|| get_string(table, "api"))
        .or_else(|| get_string(table, "request_format"))
        .map(|s| parse_wire_api(&s))
        .unwrap_or(WireApi::Chat);
    let api_key_env = get_string(table, "api_key_env")
        .or_else(|| get_string(table, "api_key_env_var"))
        .or_else(|| get_string(table, "env_key"))
        .or_else(|| get_string(table, "env_key_name"));
    let mut models = get_string_array(table, "models");
    if models.is_empty() {
        if let Some(model) = get_string(table, "model") {
            models.push(model);
        }
    }

    ModelProviderEntry {
        name: name.to_string(),
        kind,
        channel,
        base_url,
        wire_api,
        api_key_env,
        models,
    }
}

fn parse_mcp_entry(name: &str, val: &toml::Value) -> McpServerEntry {
    let table = match val.as_table() {
        Some(t) => t,
        None => {
            return McpServerEntry {
                name: name.to_string(),
                kind: McpServerKind::Http { url: String::new() },
            }
        }
    };

    if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Http {
                url: url.to_string(),
            },
        }
    } else if let Some(cmd) = table.get("command").and_then(|v| v.as_str()) {
        let args = table
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Stdio {
                command: cmd.to_string(),
                args,
            },
        }
    } else {
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Http { url: String::new() },
        }
    }
}

fn parse_marketplace(name: &str, val: &toml::Value) -> MarketplaceEntry {
    let table = val.as_table();
    MarketplaceEntry {
        name: name.to_string(),
        source_type: table
            .and_then(|t| t.get("source_type"))
            .and_then(|v| v.as_str())
            .map(String::from),
        source: table
            .and_then(|t| t.get("source"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

fn get_string(table: Option<&toml::map::Map<String, toml::Value>>, key: &str) -> Option<String> {
    table
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn get_string_array(table: Option<&toml::map::Map<String, toml::Value>>, key: &str) -> Vec<String> {
    table
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn infer_provider_kind(
    name: &str,
    base_url: Option<&str>,
    explicit_kind: Option<&str>,
) -> ProviderKind {
    if let Some(kind) = explicit_kind {
        match normalize_kind(kind).as_str() {
            "openai_subscription" | "subscription" => return ProviderKind::OpenAiSubscription,
            "openai_official" | "openai_official_api" | "official" => {
                return ProviderKind::OpenAiOfficialApi
            }
            "openai_compatible" | "openai_compatible_api" | "compatible" => {
                return ProviderKind::OpenAiCompatibleApi
            }
            "local_gateway" | "gateway" | "local" => return ProviderKind::LocalGateway,
            "cli_proxy_api" | "cliproxyapi" => return ProviderKind::CliProxyApi,
            "codex_proxy" | "codexproxy" => return ProviderKind::CodexProxy,
            _ => return ProviderKind::Custom,
        }
    }

    let normalized_name = normalize_kind(name);
    if normalized_name.contains("subscription") {
        ProviderKind::OpenAiSubscription
    } else if normalized_name.contains("codex_proxy") || normalized_name.contains("codexproxy") {
        ProviderKind::CodexProxy
    } else if normalized_name.contains("cli_proxy") || normalized_name.contains("cliproxy") {
        ProviderKind::CliProxyApi
    } else if is_local_gateway(base_url) {
        ProviderKind::LocalGateway
    } else if base_url
        .map(|url| url.contains("api.openai.com"))
        .unwrap_or(false)
    {
        ProviderKind::OpenAiOfficialApi
    } else {
        ProviderKind::OpenAiCompatibleApi
    }
}

fn infer_provider_channel(kind: &ProviderKind, explicit_channel: Option<&str>) -> ProviderChannel {
    if let Some(channel) = explicit_channel {
        match normalize_kind(channel).as_str() {
            "subscription" => return ProviderChannel::Subscription,
            "gateway" => return ProviderChannel::Gateway,
            "api" => return ProviderChannel::Api,
            _ => {}
        }
    }

    match kind {
        ProviderKind::OpenAiSubscription => ProviderChannel::Subscription,
        ProviderKind::LocalGateway | ProviderKind::CliProxyApi | ProviderKind::CodexProxy => {
            ProviderChannel::Gateway
        }
        ProviderKind::OpenAiOfficialApi
        | ProviderKind::OpenAiCompatibleApi
        | ProviderKind::Custom => ProviderChannel::Api,
    }
}

fn parse_wire_api(raw: &str) -> WireApi {
    match normalize_kind(raw).as_str() {
        "chat" | "chat_completions" | "chat_completion" => WireApi::Chat,
        "responses" | "responses_api" => WireApi::Responses,
        "sse" | "sse_stream" | "stream" => WireApi::SseStream,
        _ => WireApi::Custom,
    }
}

fn normalize_kind(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

fn is_local_gateway(base_url: Option<&str>) -> bool {
    base_url
        .map(|url| {
            url.contains("127.0.0.1") || url.contains("localhost") || url.contains("0.0.0.0")
        })
        .unwrap_or(false)
}

/// 转换为 Dashboard 摘要
pub fn to_dashboard_summary(config: &CodexConfig) -> DashboardSummary {
    let active_profile = config
        .top_level
        .get("active_profile")
        .or_else(|| config.top_level.get("profile"))
        .or_else(|| config.top_level.get("model"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let provider_count =
        if config.model_providers.is_empty() && config.top_level.contains_key("model_provider") {
            1
        } else {
            config.model_providers.len()
        };

    let total = config.mcp_servers.len();
    let enabled = total;

    let network = config
        .top_level
        .get("network_access")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "direct".to_string());

    DashboardSummary {
        active_profile,
        provider_count,
        mcp_count: McpCount { enabled, total },
        network,
        last_backup_at: None,
        health_summary: HealthSummary {
            ok: 0,
            warn: 0,
            fail: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_extracts_top_level() {
        let raw = r#"
model = "gpt-5.5"
approval_policy = "never"
sandbox_mode = "danger-full-access"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(
            cfg.top_level.get("model").and_then(|v| v.as_str()),
            Some("gpt-5.5")
        );
        assert_eq!(
            cfg.top_level
                .get("approval_policy")
                .and_then(|v| v.as_str()),
            Some("never")
        );
        assert_eq!(cfg.mcp_servers.len(), 0);
        assert_eq!(cfg.marketplaces.len(), 0);
        assert_eq!(cfg.model_providers.len(), 0);
        assert_eq!(cfg.profiles.len(), 0);
    }

    #[test]
    fn parse_with_mcp_extracts_stdio_and_http() {
        let raw = r#"
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@x/y"]

[mcp_servers.docs]
url = "https://example.com/mcp"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.mcp_servers.len(), 2);

        let fs = cfg
            .mcp_servers
            .iter()
            .find(|s| s.name == "filesystem")
            .unwrap();
        match &fs.kind {
            McpServerKind::Stdio { command, args } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &vec!["-y".to_string(), "@x/y".to_string()]);
            }
            _ => panic!("expected stdio"),
        }

        let docs = cfg.mcp_servers.iter().find(|s| s.name == "docs").unwrap();
        match &docs.kind {
            McpServerKind::Http { url } => assert_eq!(url, "https://example.com/mcp"),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn parse_with_marketplace_extracts_entries() {
        let raw = r#"
[marketplaces.alpha]
source_type = "local"
source = "/tmp/alpha"

[marketplaces.beta]
source_type = "git"
source = "https://example.com/beta.git"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.marketplaces.len(), 2);
        assert!(cfg.marketplaces.iter().any(|m| m.name == "alpha"));
        assert!(cfg.marketplaces.iter().any(|m| m.name == "beta"));
    }

    #[test]
    fn parse_model_providers_extracts_gateway_and_api_metadata() {
        let raw = r#"
[model_providers.openai]
base_url = "https://api.openai.com/v1"
wire_api = "responses"
api_key_env = "OPENAI_API_KEY"

[model_providers.local_gateway]
base_url = "http://127.0.0.1:8080/v1"
wire_api = "chat"

[model_providers.openrouter]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
models = ["openai/gpt-5-mini", "anthropic/claude-sonnet"]
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.model_providers.len(), 3);

        let openai = cfg
            .model_providers
            .iter()
            .find(|p| p.name == "openai")
            .unwrap();
        assert_eq!(openai.kind, ProviderKind::OpenAiOfficialApi);
        assert_eq!(openai.channel, ProviderChannel::Api);
        assert_eq!(openai.wire_api, WireApi::Responses);
        assert_eq!(openai.api_key_env.as_deref(), Some("OPENAI_API_KEY"));

        let gateway = cfg
            .model_providers
            .iter()
            .find(|p| p.name == "local_gateway")
            .unwrap();
        assert_eq!(gateway.kind, ProviderKind::LocalGateway);
        assert_eq!(gateway.channel, ProviderChannel::Gateway);

        let openrouter = cfg
            .model_providers
            .iter()
            .find(|p| p.name == "openrouter")
            .unwrap();
        assert_eq!(openrouter.kind, ProviderKind::OpenAiCompatibleApi);
        assert_eq!(
            openrouter.api_key_env.as_deref(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(
            openrouter.models,
            vec![
                "openai/gpt-5-mini".to_string(),
                "anthropic/claude-sonnet".to_string()
            ]
        );
    }

    #[test]
    fn parse_profiles_marks_active_profile() {
        let raw = r#"
active_profile = "dev"

[profile.dev]
model = "gpt-5.5"
model_provider = "openai"
sandbox_mode = "workspace-write"
approval_policy = "on-request"
network = "direct"
mcp_refs = ["filesystem", "git"]

[profile.gateway]
model = "claude-sonnet"
model_provider = "local_gateway"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.profiles.len(), 2);

        let dev = cfg.profiles.iter().find(|p| p.name == "dev").unwrap();
        assert!(dev.is_active);
        assert_eq!(dev.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(dev.model_provider.as_deref(), Some("openai"));
        assert_eq!(dev.sandbox_mode.as_deref(), Some("workspace-write"));
        assert_eq!(
            dev.mcp_refs,
            vec!["filesystem".to_string(), "git".to_string()]
        );

        let gateway = cfg.profiles.iter().find(|p| p.name == "gateway").unwrap();
        assert!(!gateway.is_active);
        assert_eq!(gateway.model_provider.as_deref(), Some("local_gateway"));
    }

    #[test]
    fn parse_invalid_toml_returns_err() {
        let raw = "this is not valid toml ====";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn to_dashboard_summary_minimal() {
        let raw = r#"
model = "gpt-5.5"
network_access = "enabled"

[mcp_servers.a]
command = "x"

[mcp_servers.b]
url = "https://e.com/m"
"#;
        let cfg = parse(raw).expect("parse ok");
        let summary = to_dashboard_summary(&cfg);
        assert_eq!(summary.active_profile.as_deref(), Some("gpt-5.5"));
        assert_eq!(summary.mcp_count.total, 2);
        assert_eq!(summary.mcp_count.enabled, 2);
        assert_eq!(summary.network, "enabled");
    }

    #[test]
    fn to_dashboard_summary_counts_model_providers() {
        let raw = r#"
active_profile = "dev"

[model_providers.openai]
base_url = "https://api.openai.com/v1"

[model_providers.local]
base_url = "http://localhost:8080/v1"
"#;
        let cfg = parse(raw).expect("parse ok");
        let summary = to_dashboard_summary(&cfg);
        assert_eq!(summary.active_profile.as_deref(), Some("dev"));
        assert_eq!(summary.provider_count, 2);
    }
}
