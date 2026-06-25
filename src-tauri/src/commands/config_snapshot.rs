use crate::config::model::{CodexConfig, ModelProviderEntry, ProviderKind, WireApi};
use crate::config::{loader, parser};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshotView {
    pub config_path: String,
    pub active_profile: Option<String>,
    pub profiles: Vec<ProfileView>,
    pub providers: Vec<ProviderView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub model: String,
    pub provider_id: String,
    pub sandbox: String,
    pub approval: String,
    pub network: String,
    pub mcp_refs: Vec<String>,
    pub status: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub wire_api: String,
    pub env_key: String,
    pub status: String,
    pub models: Vec<String>,
}

#[tauri::command]
pub fn config_snapshot() -> AppResult<ConfigSnapshotView> {
    let path = resolve_config_path()?;
    let raw = loader::read_raw(&path)?;
    let config = parser::parse(&raw)?;
    Ok(to_snapshot_view(&path, &config))
}

fn resolve_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(DEFAULT_CONFIG_PATH))
}

fn to_snapshot_view(path: &PathBuf, config: &CodexConfig) -> ConfigSnapshotView {
    let active_profile = active_profile_name(config);
    let profiles = profile_views(config, active_profile.as_deref());
    let providers = provider_views(config, &profiles);

    ConfigSnapshotView {
        config_path: path.to_string_lossy().to_string(),
        active_profile,
        profiles,
        providers,
    }
}

fn active_profile_name(config: &CodexConfig) -> Option<String> {
    config
        .top_level
        .get("active_profile")
        .or_else(|| config.top_level.get("profile"))
        .and_then(|value| value.as_str())
        .map(String::from)
        .or_else(|| {
            config
                .profiles
                .iter()
                .find(|profile| profile.is_active)
                .map(|profile| profile.name.clone())
        })
}

fn profile_views(config: &CodexConfig, active_profile: Option<&str>) -> Vec<ProfileView> {
    if config.profiles.is_empty() {
        return synthesized_default_profile(config);
    }

    config
        .profiles
        .iter()
        .map(|profile| {
            let is_active = profile.is_active || active_profile == Some(profile.name.as_str());
            let provider_id = profile
                .model_provider
                .clone()
                .unwrap_or_else(|| default_provider_id(config));
            ProfileView {
                id: profile.name.clone(),
                name: profile.name.clone(),
                model: profile
                    .model
                    .clone()
                    .unwrap_or_else(|| top_level_string(config, "model").unwrap_or_default()),
                provider_id,
                sandbox: profile.sandbox_mode.clone().unwrap_or_else(|| {
                    top_level_string(config, "sandbox_mode").unwrap_or_default()
                }),
                approval: profile.approval_policy.clone().unwrap_or_else(|| {
                    top_level_string(config, "approval_policy").unwrap_or_default()
                }),
                network: profile.network.clone().unwrap_or_else(|| {
                    top_level_string(config, "network_access").unwrap_or_default()
                }),
                mcp_refs: if profile.mcp_refs.is_empty() {
                    config
                        .mcp_servers
                        .iter()
                        .map(|server| server.name.clone())
                        .collect()
                } else {
                    profile.mcp_refs.clone()
                },
                status: if is_active { "ok" } else { "idle" }.to_string(),
                is_active,
            }
        })
        .collect()
}

fn synthesized_default_profile(config: &CodexConfig) -> Vec<ProfileView> {
    let model = top_level_string(config, "model").unwrap_or_default();
    let provider_id = default_provider_id(config);
    if model.is_empty() && provider_id.is_empty() {
        return Vec::new();
    }

    vec![ProfileView {
        id: "default".to_string(),
        name: "default".to_string(),
        model,
        provider_id,
        sandbox: top_level_string(config, "sandbox_mode").unwrap_or_default(),
        approval: top_level_string(config, "approval_policy").unwrap_or_default(),
        network: top_level_string(config, "network_access").unwrap_or_default(),
        mcp_refs: config
            .mcp_servers
            .iter()
            .map(|server| server.name.clone())
            .collect(),
        status: "ok".to_string(),
        is_active: true,
    }]
}

fn provider_views(config: &CodexConfig, profiles: &[ProfileView]) -> Vec<ProviderView> {
    let models_by_provider = models_by_provider(profiles);
    let mut providers: Vec<ProviderView> = config
        .model_providers
        .iter()
        .map(|provider| provider_view(provider, &models_by_provider))
        .collect();

    ensure_subscription_provider(&mut providers, profiles);

    if providers.is_empty() {
        let models = profiles
            .iter()
            .filter_map(|profile| {
                if profile.model.is_empty() {
                    None
                } else {
                    Some(profile.model.clone())
                }
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        providers.push(ProviderView {
            id: "codex-subscription".to_string(),
            name: "Codex Subscription".to_string(),
            kind: "subscription".to_string(),
            base_url: "official desktop channel".to_string(),
            wire_api: "custom".to_string(),
            env_key: "official login".to_string(),
            status: "ok".to_string(),
            models,
        });
    }

    for profile in profiles {
        if profile.provider_id.is_empty()
            || providers
                .iter()
                .any(|provider| provider.id == profile.provider_id)
        {
            continue;
        }
        providers.push(ProviderView {
            id: profile.provider_id.clone(),
            name: profile.provider_id.clone(),
            kind: "subscription".to_string(),
            base_url: "official desktop channel".to_string(),
            wire_api: "custom".to_string(),
            env_key: "official login".to_string(),
            status: "warn".to_string(),
            models: vec![profile.model.clone()]
                .into_iter()
                .filter(|model| !model.is_empty())
                .collect(),
        });
    }

    providers
}

fn ensure_subscription_provider(providers: &mut Vec<ProviderView>, profiles: &[ProfileView]) {
    if providers
        .iter()
        .any(|provider| provider.id == "codex-subscription" || provider.kind == "subscription")
    {
        return;
    }

    let models = profiles
        .iter()
        .filter(|profile| profile.provider_id == "codex-subscription")
        .filter_map(|profile| {
            if profile.model.is_empty() {
                None
            } else {
                Some(profile.model.clone())
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    providers.insert(
        0,
        ProviderView {
            id: "codex-subscription".to_string(),
            name: "Codex Subscription".to_string(),
            kind: "subscription".to_string(),
            base_url: "official desktop channel".to_string(),
            wire_api: "custom".to_string(),
            env_key: "official login".to_string(),
            status: "ok".to_string(),
            models,
        },
    );
}

fn provider_view(
    provider: &ModelProviderEntry,
    models_by_provider: &BTreeMap<String, BTreeSet<String>>,
) -> ProviderView {
    let env_key = provider
        .api_key_env
        .clone()
        .unwrap_or_else(|| match provider.kind {
            ProviderKind::OpenAiSubscription => "official login".to_string(),
            _ => "not configured".to_string(),
        });
    let status = match provider.kind {
        ProviderKind::OpenAiSubscription => "ok",
        _ if provider.api_key_env.is_some() => "ok",
        _ => "warn",
    };

    ProviderView {
        id: provider.name.clone(),
        name: provider.name.clone(),
        kind: provider_kind_key(&provider.kind).to_string(),
        base_url: provider
            .base_url
            .clone()
            .unwrap_or_else(|| "official desktop channel".to_string()),
        wire_api: wire_api_key(&provider.wire_api).to_string(),
        env_key,
        status: status.to_string(),
        models: provider_models(provider, models_by_provider),
    }
}

fn provider_models(
    provider: &ModelProviderEntry,
    models_by_provider: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut models = provider.models.iter().cloned().collect::<BTreeSet<_>>();
    if let Some(from_profiles) = models_by_provider.get(&provider.name) {
        models.extend(from_profiles.iter().cloned());
    }
    models.into_iter().collect()
}

fn models_by_provider(profiles: &[ProfileView]) -> BTreeMap<String, BTreeSet<String>> {
    let mut map = BTreeMap::new();
    for profile in profiles {
        if profile.provider_id.is_empty() || profile.model.is_empty() {
            continue;
        }
        map.entry(profile.provider_id.clone())
            .or_insert_with(BTreeSet::new)
            .insert(profile.model.clone());
    }
    map
}

fn default_provider_id(config: &CodexConfig) -> String {
    top_level_string(config, "model_provider").unwrap_or_else(|| "codex-subscription".to_string())
}

fn top_level_string(config: &CodexConfig, key: &str) -> Option<String> {
    config
        .top_level
        .get(key)
        .and_then(|value| value.as_str())
        .map(String::from)
}

fn provider_kind_key(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiSubscription => "subscription",
        ProviderKind::OpenAiOfficialApi => "official_api",
        ProviderKind::OpenAiCompatibleApi => "compatible_api",
        ProviderKind::LocalGateway | ProviderKind::CliProxyApi | ProviderKind::CodexProxy => {
            "local_gateway"
        }
        ProviderKind::Custom => "compatible_api",
    }
}

fn wire_api_key(wire_api: &WireApi) -> &'static str {
    match wire_api {
        WireApi::Chat => "chat",
        WireApi::Responses => "responses",
        WireApi::SseStream => "sse_stream",
        WireApi::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_maps_profiles_and_providers() {
        let raw = r#"
active_profile = "dev"

[model_providers.openai]
base_url = "https://api.openai.com/v1"
wire_api = "responses"
api_key_env = "OPENAI_API_KEY"

[model_providers.openrouter]
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
api_key_env = "OPENROUTER_API_KEY"

[profile.dev]
model = "gpt-5.1"
model_provider = "openai"
sandbox_mode = "workspace-write"
approval_policy = "on-request"

[profile.router]
model = "openai/gpt-5-mini"
model_provider = "openrouter"
"#;
        let cfg = parser::parse(raw).expect("parse ok");
        let snapshot = to_snapshot_view(&PathBuf::from("/tmp/config.toml"), &cfg);

        assert_eq!(snapshot.active_profile.as_deref(), Some("dev"));
        assert_eq!(snapshot.profiles.len(), 2);
        assert!(snapshot
            .profiles
            .iter()
            .any(|profile| profile.name == "dev" && profile.is_active));

        let openai = snapshot
            .providers
            .iter()
            .find(|provider| provider.id == "openai")
            .expect("openai provider");
        assert_eq!(openai.kind, "official_api");
        assert_eq!(openai.wire_api, "responses");
        assert_eq!(openai.env_key, "OPENAI_API_KEY");
        assert_eq!(openai.models, vec!["gpt-5.1".to_string()]);
    }

    #[test]
    fn snapshot_synthesizes_subscription_provider_for_minimal_config() {
        let raw = r#"
model = "gpt-5.5"
approval_policy = "never"
sandbox_mode = "danger-full-access"
"#;
        let cfg = parser::parse(raw).expect("parse ok");
        let snapshot = to_snapshot_view(&PathBuf::from("/tmp/config.toml"), &cfg);

        assert_eq!(snapshot.profiles.len(), 1);
        assert_eq!(snapshot.profiles[0].provider_id, "codex-subscription");
        assert_eq!(snapshot.providers.len(), 1);
        assert_eq!(snapshot.providers[0].kind, "subscription");
        assert_eq!(snapshot.providers[0].env_key, "official login");
    }

    #[test]
    fn snapshot_keeps_subscription_provider_with_third_party_provider() {
        let raw = r#"
active_profile = "official-codex"

[model_providers.openrouter]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
api_key_env = "OPENROUTER_API_KEY"

[profile.official-codex]
model = "gpt-5-codex"
model_provider = "codex-subscription"

[profile.router]
model = "openai/gpt-5-mini"
model_provider = "openrouter"
"#;
        let cfg = parser::parse(raw).expect("parse ok");
        let snapshot = to_snapshot_view(&PathBuf::from("/tmp/config.toml"), &cfg);

        assert!(
            snapshot
                .providers
                .iter()
                .any(|provider| provider.id == "codex-subscription"
                    && provider.kind == "subscription")
        );
        assert!(snapshot
            .providers
            .iter()
            .any(|provider| provider.id == "openrouter" && provider.kind == "compatible_api"));
    }
}
