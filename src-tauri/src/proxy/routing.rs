// src-tauri/src/proxy/routing.rs
//
// model id → upstream provider 解析。
//
// Codex App 调 Codex Box 代理时,model 字段格式约定:
//   - 命名空间形式: "provider_name/model_id"   (推荐,避免冲突)
//   - 裸 model id: "model_id"                  (唯一匹配时才返回)
//
// 解析流程:
//   1. 优先按命名空间形式解析
//   2. 否则在 inject-map 里按唯一 model_id 匹配
//   3. 都没命中返回 None,handler 返回 404
use crate::commands::opencodex::{
    CodexRoutingConfig, CodexRoutingRoute, ModelCatalogEntry, ProviderRoute,
};
use crate::proxy::inject_map::InjectMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 解析后的路由
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRoute {
    pub provider_name: String,
    pub model_id: String,
    pub upstream_base_url: String,
    pub wire_api: String,
    pub auth_source: Option<String>,
    pub env_key: Option<String>,
    pub api_key: Option<String>,
    pub http_headers: std::collections::BTreeMap<String, String>,
    pub chat_reasoning: Option<ChatReasoningConfig>,
    pub text_only: bool,
    pub vision_bridge: Option<VisionBridgeConfig>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReasoningConfig {
    #[serde(default, alias = "supports_thinking")]
    pub supports_thinking: Option<bool>,
    #[serde(default, alias = "supports_effort")]
    pub supports_effort: Option<bool>,
    #[serde(default, alias = "thinking_param")]
    pub thinking_param: Option<String>,
    #[serde(default, alias = "effort_param")]
    pub effort_param: Option<String>,
    #[serde(default, alias = "effort_value_mode")]
    pub effort_value_mode: Option<String>,
    #[serde(default, alias = "min_output_tokens")]
    pub min_output_tokens: Option<u64>,
    #[serde(default, alias = "output_format")]
    pub output_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionBridgeConfig {
    pub base_url: String,
    pub model: String,
    pub env_key: Option<String>,
}

pub fn resolve_route(model_id: &str, map: &InjectMap) -> Option<ResolvedRoute> {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. 命名空间形式: provider/model
    if let Some((ns, rest)) = trimmed.split_once('/') {
        if !ns.is_empty() && !rest.is_empty() {
            for entry in &map.providers {
                if is_legacy_opencodex_proxy_route(&entry.name, &entry.original_base_url) {
                    continue;
                }
                if entry.name == ns {
                    if entry.models.iter().any(|m| m == rest) || entry.models.is_empty() {
                        return Some(build_route(entry, rest));
                    }
                }
            }
            // 命名空间命中 provider 但 model 不在 models 列表里,继续裸匹配兜底
        }
    }

    // 2. 裸 model id 唯一匹配
    let mut hits = Vec::new();
    for entry in &map.providers {
        if is_legacy_opencodex_proxy_route(&entry.name, &entry.original_base_url) {
            continue;
        }
        if entry.models.iter().any(|m| m == trimmed) {
            hits.push(entry);
        }
    }
    if hits.len() == 1 {
        return Some(build_route(hits[0], trimmed));
    }

    // 3. 兜底: 任何 entry.models 为空且 provider 唯一,允许透传 model id
    if hits.is_empty() {
        let mut empty = Vec::new();
        for entry in &map.providers {
            if is_legacy_opencodex_proxy_route(&entry.name, &entry.original_base_url) {
                continue;
            }
            if entry.models.is_empty() {
                empty.push(entry);
            }
        }
        if empty.len() == 1 {
            return Some(build_route(empty[0], trimmed));
        }
    }

    None
}

pub fn resolve_catalog_route(model_id: &str, map: &InjectMap) -> Option<ResolvedRoute> {
    let requested = model_id.trim();
    if requested.is_empty() {
        return None;
    }

    let cfg = crate::commands::opencodex::opencodex_config_read().ok()?;
    resolve_catalog_route_from_sources(requested, &cfg.catalog, &cfg.providers, map)
}

pub fn is_native_openai_catalog_model(model_id: &str) -> bool {
    let requested = model_id.trim();
    if requested.is_empty() {
        return false;
    }

    let Ok(cfg) = crate::commands::opencodex::opencodex_config_read() else {
        return false;
    };
    is_native_openai_catalog_model_from_sources(requested, &cfg.catalog)
}

pub fn is_native_openai_catalog_model_from_sources(
    requested: &str,
    catalog: &[ModelCatalogEntry],
) -> bool {
    catalog.iter().any(|entry| {
        is_native_openai_catalog_entry(entry)
            && (entry.model_id == requested
                || format!("{}/{}", entry.provider, entry.model_id) == requested)
    })
}

fn is_native_openai_catalog_entry(entry: &ModelCatalogEntry) -> bool {
    entry.visible
        && entry.provider == "openai"
        && entry
            .backend_provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
}

pub fn resolve_catalog_route_from_sources(
    requested: &str,
    catalog: &[ModelCatalogEntry],
    providers: &[ProviderRoute],
    map: &InjectMap,
) -> Option<ResolvedRoute> {
    let entry = catalog.iter().find(|entry| {
        entry.visible
            && (entry.model_id == requested
                || format!("{}/{}", entry.provider, entry.model_id) == requested)
    })?;
    let upstream_model_for_routing = entry
        .backend_model
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&entry.model_id);

    if is_native_openai_catalog_entry(entry) {
        for router_provider in providers.iter().filter(|provider| {
            provider.enabled
                && provider.codex_routing.is_some()
                && !is_legacy_opencodex_proxy_route(&provider.name, &provider.base_url)
        }) {
            if let Some(route) = resolve_codex_routing_route(
                requested,
                upstream_model_for_routing,
                entry,
                router_provider,
                providers,
                map,
            ) {
                return Some(route);
            }
        }
    }

    let provider_name = entry
        .backend_provider
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&entry.provider);
    let upstream_model = entry
        .backend_model
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&entry.model_id);

    if let Some(router_provider) = providers
        .iter()
        .find(|provider| provider.enabled && provider.name == provider_name)
    {
        if let Some(route) = resolve_codex_routing_route(
            requested,
            upstream_model,
            entry,
            router_provider,
            providers,
            map,
        ) {
            return Some(route);
        }
        if router_provider.codex_routing.is_some() {
            return None;
        }
    }

    if let Some(provider) = map.providers.iter().find(|p| {
        p.name == provider_name && !is_legacy_opencodex_proxy_route(&p.name, &p.original_base_url)
    }) {
        return Some(ResolvedRoute {
            provider_name: provider.name.clone(),
            model_id: upstream_model.to_string(),
            upstream_base_url: provider.original_base_url.clone(),
            wire_api: provider.wire_api.clone(),
            auth_source: None,
            env_key: provider.env_key.clone(),
            api_key: None,
            http_headers: provider.http_headers.clone(),
            chat_reasoning: chat_reasoning_from_extra(&provider.extra)
                .or_else(|| chat_reasoning_from_catalog(entry)),
            text_only: text_only_from_catalog(entry)
                .or_else(|| text_only_from_inject_extra(&provider.extra))
                .unwrap_or(false),
            vision_bridge: vision_bridge_from_catalog(entry),
        });
    }

    let provider = providers.iter().find(|provider| {
        provider.enabled
            && provider.name == provider_name
            && !is_legacy_opencodex_proxy_route(&provider.name, &provider.base_url)
    })?;
    Some(ResolvedRoute {
        provider_name: provider.name.clone(),
        model_id: upstream_model.to_string(),
        upstream_base_url: provider.base_url.clone(),
        wire_api: provider.wire_api.clone(),
        auth_source: None,
        env_key: provider.api_key_ref.as_deref().and_then(normalize_env_ref),
        api_key: provider_runtime_api_key(provider),
        http_headers: provider.http_headers.clone(),
        chat_reasoning: chat_reasoning_from_provider(provider)
            .or_else(|| chat_reasoning_from_catalog(entry)),
        text_only: text_only_from_catalog(entry)
            .or_else(|| text_only_from_provider(provider))
            .unwrap_or(false),
        vision_bridge: vision_bridge_from_catalog(entry),
    })
}

fn resolve_codex_routing_route(
    requested: &str,
    catalog_upstream_model: &str,
    catalog_entry: &ModelCatalogEntry,
    router_provider: &ProviderRoute,
    providers: &[ProviderRoute],
    map: &InjectMap,
) -> Option<ResolvedRoute> {
    let routing = router_provider.codex_routing.as_ref()?;
    let selected = select_codex_route(requested, catalog_upstream_model, routing)?;
    let target_provider_name = selected
        .target_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let upstream_model = route_upstream_model(selected, requested, catalog_upstream_model);
    let upstream_wire_api = selected
        .upstream
        .api_format
        .as_deref()
        .and_then(api_format_to_wire_api);
    let route_chat_reasoning = chat_reasoning_from_codex_route(selected);
    let catalog_chat_reasoning = chat_reasoning_from_catalog(catalog_entry);
    let route_text_only = text_only_from_codex_route(selected);
    let catalog_text_only = text_only_from_catalog(catalog_entry);
    let route_vision_bridge = vision_bridge_from_codex_route(selected);
    let catalog_vision_bridge = vision_bridge_from_catalog(catalog_entry);

    if let Some(target_name) = target_provider_name {
        if target_name == router_provider.name {
            return None;
        }
        if let Some(provider) = providers.iter().find(|provider| {
            provider.enabled
                && provider.name == target_name
                && !is_legacy_opencodex_proxy_route(&provider.name, &provider.base_url)
        }) {
            return Some(ResolvedRoute {
                provider_name: provider.name.clone(),
                model_id: upstream_model,
                upstream_base_url: selected
                    .upstream
                    .base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&provider.base_url)
                    .to_string(),
                wire_api: upstream_wire_api.unwrap_or_else(|| provider.wire_api.clone()),
                auth_source: route_auth_source(selected),
                env_key: route_env_key(selected)
                    .or_else(|| provider.api_key_ref.as_deref().and_then(normalize_env_ref)),
                api_key: route_runtime_api_key(selected)
                    .or_else(|| provider_runtime_api_key(provider)),
                http_headers: provider.http_headers.clone(),
                chat_reasoning: route_chat_reasoning
                    .clone()
                    .or_else(|| chat_reasoning_from_provider(provider))
                    .or_else(|| catalog_chat_reasoning.clone()),
                text_only: route_text_only
                    .or(catalog_text_only)
                    .or_else(|| text_only_from_provider(provider))
                    .unwrap_or(false),
                vision_bridge: route_vision_bridge
                    .clone()
                    .or_else(|| catalog_vision_bridge.clone()),
            });
        }

        if let Some(provider) = map.providers.iter().find(|provider| {
            provider.name == target_name
                && !is_legacy_opencodex_proxy_route(&provider.name, &provider.original_base_url)
        }) {
            return Some(ResolvedRoute {
                provider_name: provider.name.clone(),
                model_id: upstream_model,
                upstream_base_url: selected
                    .upstream
                    .base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&provider.original_base_url)
                    .to_string(),
                wire_api: upstream_wire_api.unwrap_or_else(|| provider.wire_api.clone()),
                auth_source: route_auth_source(selected),
                env_key: route_env_key(selected).or_else(|| provider.env_key.clone()),
                api_key: None,
                http_headers: provider.http_headers.clone(),
                chat_reasoning: route_chat_reasoning
                    .clone()
                    .or_else(|| chat_reasoning_from_extra(&provider.extra))
                    .or_else(|| catalog_chat_reasoning.clone()),
                text_only: route_text_only
                    .or(catalog_text_only)
                    .or_else(|| text_only_from_inject_extra(&provider.extra))
                    .unwrap_or(false),
                vision_bridge: route_vision_bridge
                    .clone()
                    .or_else(|| catalog_vision_bridge.clone()),
            });
        }
    }

    let base_url = selected
        .upstream
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(ResolvedRoute {
        provider_name: target_provider_name
            .or(selected.label.as_deref())
            .unwrap_or(&selected.id)
            .to_string(),
        model_id: upstream_model,
        upstream_base_url: base_url.to_string(),
        wire_api: upstream_wire_api.unwrap_or_else(|| "responses".to_string()),
        auth_source: route_auth_source(selected),
        env_key: route_env_key(selected),
        api_key: route_runtime_api_key(selected),
        http_headers: BTreeMap::new(),
        chat_reasoning: route_chat_reasoning.or(catalog_chat_reasoning),
        text_only: route_text_only.or(catalog_text_only).unwrap_or(false),
        vision_bridge: route_vision_bridge.or(catalog_vision_bridge),
    })
}

fn select_codex_route<'a>(
    requested: &str,
    catalog_upstream_model: &str,
    routing: &'a CodexRoutingConfig,
) -> Option<&'a CodexRoutingRoute> {
    if routing.enabled.is_some_and(|enabled| !enabled) {
        return None;
    }

    for route in routing.routes.iter().filter(|route| route_enabled(route)) {
        if route.match_rule.models.iter().any(|model| {
            model_matches(model, requested) || model_matches(model, catalog_upstream_model)
        }) {
            return Some(route);
        }
    }

    for route in routing.routes.iter().filter(|route| route_enabled(route)) {
        if route.match_rule.prefixes.iter().any(|prefix| {
            prefix_matches(prefix, requested) || prefix_matches(prefix, catalog_upstream_model)
        }) {
            return Some(route);
        }
    }

    if let Some(default_route_id) = routing
        .default_route_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(route) = routing
            .routes
            .iter()
            .find(|route| route_enabled(route) && route.id == default_route_id)
        {
            return Some(route);
        }
    }

    routing.routes.iter().find(|route| route_enabled(route))
}

fn route_enabled(route: &CodexRoutingRoute) -> bool {
    route.enabled.unwrap_or(true)
}

fn model_matches(rule_model: &str, model: &str) -> bool {
    let rule_model = rule_model.trim();
    let model = model.trim();
    !rule_model.is_empty() && !model.is_empty() && rule_model == model
}

fn prefix_matches(rule_prefix: &str, model: &str) -> bool {
    let rule_prefix = rule_prefix.trim();
    let model = model.trim();
    !rule_prefix.is_empty() && !model.is_empty() && model.starts_with(rule_prefix)
}

fn route_upstream_model(
    route: &CodexRoutingRoute,
    requested: &str,
    catalog_upstream_model: &str,
) -> String {
    route
        .upstream
        .model_map
        .get(requested)
        .or_else(|| route.upstream.model_map.get(catalog_upstream_model))
        .cloned()
        .unwrap_or_else(|| catalog_upstream_model.to_string())
}

fn api_format_to_wire_api(value: &str) -> Option<String> {
    match value.trim() {
        "openai_chat" | "chat" => Some("chat".to_string()),
        "openai_responses" | "responses" => Some("responses".to_string()),
        "sse_stream" => Some("sse_stream".to_string()),
        "custom" => Some("custom".to_string()),
        _ => None,
    }
}

fn route_env_key(route: &CodexRoutingRoute) -> Option<String> {
    route
        .upstream
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| value.starts_with('$'))
        .and_then(normalize_env_ref)
}

fn route_runtime_api_key(route: &CodexRoutingRoute) -> Option<String> {
    route
        .upstream
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('$'))
        .map(ToString::to_string)
}

fn provider_runtime_api_key(provider: &ProviderRoute) -> Option<String> {
    provider
        .extra
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.starts_with('$'))
        .map(ToString::to_string)
}

fn route_auth_source(route: &CodexRoutingRoute) -> Option<String> {
    route
        .upstream
        .auth
        .as_ref()
        .and_then(|auth| {
            auth.get("source")
                .or_else(|| auth.get("authProvider"))
                .or_else(|| auth.get("auth_provider"))
        })
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn is_legacy_opencodex_proxy_route(name: &str, base_url: &str) -> bool {
    name.eq_ignore_ascii_case("opencodex")
        || base_url.contains("127.0.0.1:8765")
        || base_url.contains("localhost:8765")
}

fn chat_reasoning_from_provider(provider: &ProviderRoute) -> Option<ChatReasoningConfig> {
    chat_reasoning_from_extra(&provider.extra)
}

fn chat_reasoning_from_catalog(entry: &ModelCatalogEntry) -> Option<ChatReasoningConfig> {
    chat_reasoning_from_extra(&entry.extra)
}

fn text_only_from_catalog(entry: &ModelCatalogEntry) -> Option<bool> {
    text_only_from_extra(&entry.extra)
}

fn vision_bridge_from_catalog(entry: &ModelCatalogEntry) -> Option<VisionBridgeConfig> {
    if entry.vision_bridge_enabled != Some(true) {
        return None;
    }
    let base_url = entry
        .vision_fallback_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let model = entry
        .vision_fallback_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(VisionBridgeConfig {
        base_url: base_url.to_string(),
        model: model.to_string(),
        env_key: entry
            .vision_fallback_api_key_ref
            .as_deref()
            .and_then(normalize_env_ref),
    })
}

fn text_only_from_provider(provider: &ProviderRoute) -> Option<bool> {
    text_only_from_extra(&provider.extra)
}

fn text_only_from_inject_extra(extra: &BTreeMap<String, serde_json::Value>) -> Option<bool> {
    text_only_from_extra(extra)
}

fn chat_reasoning_from_codex_route(route: &CodexRoutingRoute) -> Option<ChatReasoningConfig> {
    route
        .upstream
        .extra
        .get("codexChatReasoning")
        .or_else(|| route.upstream.extra.get("codex_chat_reasoning"))
        .and_then(chat_reasoning_from_value)
        .or_else(|| {
            route.capabilities.as_ref().and_then(|capabilities| {
                capabilities
                    .get("codexChatReasoning")
                    .or_else(|| capabilities.get("codex_chat_reasoning"))
                    .and_then(chat_reasoning_from_value)
                    .or_else(|| chat_reasoning_from_value(capabilities))
            })
        })
        .or_else(|| chat_reasoning_from_extra(&route.extra))
}

fn text_only_from_codex_route(route: &CodexRoutingRoute) -> Option<bool> {
    route
        .capabilities
        .as_ref()
        .and_then(text_only_from_value)
        .or_else(|| text_only_from_extra(&route.upstream.extra))
        .or_else(|| text_only_from_extra(&route.extra))
}

fn vision_bridge_from_codex_route(route: &CodexRoutingRoute) -> Option<VisionBridgeConfig> {
    route
        .capabilities
        .as_ref()
        .and_then(vision_bridge_from_value)
        .or_else(|| vision_bridge_from_extra(&route.upstream.extra))
        .or_else(|| vision_bridge_from_extra(&route.extra))
}

fn chat_reasoning_from_extra(
    extra: &BTreeMap<String, serde_json::Value>,
) -> Option<ChatReasoningConfig> {
    extra
        .get("codexChatReasoning")
        .or_else(|| extra.get("codex_chat_reasoning"))
        .and_then(chat_reasoning_from_value)
}

fn chat_reasoning_from_value(value: &serde_json::Value) -> Option<ChatReasoningConfig> {
    if value.is_null() {
        return None;
    }
    let config: ChatReasoningConfig = serde_json::from_value(value.clone()).ok()?;
    if chat_reasoning_config_is_empty(&config) {
        None
    } else {
        Some(normalize_chat_reasoning_config(config))
    }
}

fn normalize_chat_reasoning_config(mut config: ChatReasoningConfig) -> ChatReasoningConfig {
    config.thinking_param = normalize_optional_string(config.thinking_param);
    config.effort_param = normalize_optional_string(config.effort_param);
    config.effort_value_mode = normalize_optional_string(config.effort_value_mode);
    config.output_format = normalize_optional_string(config.output_format);
    config
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn text_only_from_extra(extra: &BTreeMap<String, serde_json::Value>) -> Option<bool> {
    extra
        .get("textOnly")
        .or_else(|| extra.get("text_only"))
        .and_then(|value| value.as_bool())
        .or_else(|| {
            extra
                .get("inputModalities")
                .or_else(|| extra.get("input_modalities"))
                .and_then(text_only_from_modalities_value)
        })
}

fn text_only_from_value(value: &serde_json::Value) -> Option<bool> {
    value
        .get("textOnly")
        .or_else(|| value.get("text_only"))
        .and_then(|value| value.as_bool())
        .or_else(|| {
            value
                .get("inputModalities")
                .or_else(|| value.get("input_modalities"))
                .and_then(text_only_from_modalities_value)
        })
}

fn text_only_from_modalities_value(value: &serde_json::Value) -> Option<bool> {
    let array = value.as_array()?;
    let mut has_text = false;
    let mut has_image = false;
    for item in array {
        let Some(modality) = item.as_str().map(|value| value.trim().to_ascii_lowercase()) else {
            continue;
        };
        if modality == "text" {
            has_text = true;
        }
        if modality == "image" {
            has_image = true;
        }
    }
    if has_image {
        Some(false)
    } else if has_text {
        Some(true)
    } else {
        None
    }
}

fn vision_bridge_from_extra(
    extra: &BTreeMap<String, serde_json::Value>,
) -> Option<VisionBridgeConfig> {
    extra
        .get("visionBridge")
        .or_else(|| extra.get("vision_bridge"))
        .and_then(vision_bridge_from_value)
        .or_else(|| {
            let enabled = extra
                .get("vision_bridge_enabled")
                .or_else(|| extra.get("visionBridgeEnabled"))
                .and_then(|value| value.as_bool())?;
            if !enabled {
                return None;
            }
            let base_url = extra
                .get("vision_fallback_base_url")
                .or_else(|| extra.get("visionFallbackBaseUrl"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let model = extra
                .get("vision_fallback_model")
                .or_else(|| extra.get("visionFallbackModel"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let env_key = extra
                .get("vision_fallback_api_key_ref")
                .or_else(|| extra.get("visionFallbackApiKeyRef"))
                .and_then(|value| value.as_str())
                .and_then(normalize_env_ref);
            Some(VisionBridgeConfig {
                base_url: base_url.to_string(),
                model: model.to_string(),
                env_key,
            })
        })
}

fn vision_bridge_from_value(value: &serde_json::Value) -> Option<VisionBridgeConfig> {
    if value.is_null() {
        return None;
    }
    let enabled = value
        .get("enabled")
        .or_else(|| value.get("visionBridgeEnabled"))
        .or_else(|| value.get("vision_bridge_enabled"))
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !enabled {
        return None;
    }
    let base_url = value
        .get("baseUrl")
        .or_else(|| value.get("base_url"))
        .or_else(|| value.get("visionFallbackBaseUrl"))
        .or_else(|| value.get("vision_fallback_base_url"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let model = value
        .get("model")
        .or_else(|| value.get("visionModel"))
        .or_else(|| value.get("vision_model"))
        .or_else(|| value.get("visionFallbackModel"))
        .or_else(|| value.get("vision_fallback_model"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let env_key = value
        .get("apiKeyRef")
        .or_else(|| value.get("api_key_ref"))
        .or_else(|| value.get("envKey"))
        .or_else(|| value.get("env_key"))
        .or_else(|| value.get("visionFallbackApiKeyRef"))
        .or_else(|| value.get("vision_fallback_api_key_ref"))
        .and_then(|value| value.as_str())
        .and_then(normalize_env_ref);
    Some(VisionBridgeConfig {
        base_url: base_url.to_string(),
        model: model.to_string(),
        env_key,
    })
}

fn chat_reasoning_config_is_empty(config: &ChatReasoningConfig) -> bool {
    config.supports_thinking.is_none()
        && config.supports_effort.is_none()
        && config
            .thinking_param
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        && config
            .effort_param
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        && config
            .effort_value_mode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        && config.min_output_tokens.is_none()
        && config
            .output_format
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
}

fn normalize_env_ref(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(inner) = trimmed.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        return Some(inner.to_string());
    }
    if let Some(inner) = trimmed.strip_prefix('$') {
        return Some(inner.to_string());
    }
    Some(trimmed.to_string())
}

fn build_route(entry: &crate::proxy::inject_map::InjectMapEntry, model_id: &str) -> ResolvedRoute {
    ResolvedRoute {
        provider_name: entry.name.clone(),
        model_id: model_id.to_string(),
        upstream_base_url: entry.original_base_url.clone(),
        wire_api: entry.wire_api.clone(),
        auth_source: None,
        env_key: entry.env_key.clone(),
        api_key: None,
        http_headers: entry.http_headers.clone(),
        chat_reasoning: chat_reasoning_from_extra(&entry.extra),
        text_only: text_only_from_inject_extra(&entry.extra).unwrap_or(false),
        vision_bridge: vision_bridge_from_inject_extra(&entry.extra),
    }
}

fn vision_bridge_from_inject_extra(
    extra: &BTreeMap<String, serde_json::Value>,
) -> Option<VisionBridgeConfig> {
    vision_bridge_from_extra(extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::inject_map::{InjectMap, InjectMapEntry};
    use std::collections::BTreeMap;

    fn entry(name: &str, base: &str, models: Vec<&str>) -> InjectMapEntry {
        InjectMapEntry {
            name: name.to_string(),
            original_base_url: base.to_string(),
            env_key: Some(format!("{}_KEY", name.to_uppercase())),
            http_headers: BTreeMap::new(),
            wire_api: "chat".to_string(),
            models: models.into_iter().map(String::from).collect(),
            kind: "compatible_api".to_string(),
            extra: BTreeMap::new(),
        }
    }

    fn map_with(providers: Vec<InjectMapEntry>) -> InjectMap {
        InjectMap {
            updated_at: "2026-06-25T00:00:00Z".to_string(),
            port: 1455,
            providers,
        }
    }

    fn catalog_entry(
        model_id: &str,
        provider: &str,
        backend_provider: Option<&str>,
        backend_model: Option<&str>,
    ) -> ModelCatalogEntry {
        ModelCatalogEntry {
            model_id: model_id.to_string(),
            display_name: None,
            provider: provider.to_string(),
            backend_model: backend_model.map(ToString::to_string),
            backend_provider: backend_provider.map(ToString::to_string),
            visible: true,
            reasoning: None,
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::new(),
        }
    }

    fn provider_route(name: &str, base_url: &str, api_key_ref: Option<&str>) -> ProviderRoute {
        ProviderRoute {
            name: name.to_string(),
            base_url: base_url.to_string(),
            wire_api: "chat".to_string(),
            api_key_ref: api_key_ref.map(ToString::to_string),
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: None,
            extra: BTreeMap::new(),
        }
    }

    fn router_provider(name: &str, routes: Vec<CodexRoutingRoute>) -> ProviderRoute {
        ProviderRoute {
            name: name.to_string(),
            base_url: "http://127.0.0.1:1455/v1".to_string(),
            wire_api: "responses".to_string(),
            api_key_ref: None,
            http_headers: BTreeMap::new(),
            enabled: true,
            note: None,
            codex_routing: Some(CodexRoutingConfig {
                enabled: Some(true),
                default_route_id: None,
                routes,
                extra: BTreeMap::new(),
            }),
            extra: BTreeMap::new(),
        }
    }

    fn codex_route(
        id: &str,
        target_provider_id: &str,
        match_models: Vec<&str>,
        model_map: Vec<(&str, &str)>,
        api_format: &str,
    ) -> CodexRoutingRoute {
        CodexRoutingRoute {
            id: id.to_string(),
            label: None,
            enabled: Some(true),
            target_provider_id: Some(target_provider_id.to_string()),
            match_rule: crate::commands::opencodex::CodexRoutingMatch {
                models: match_models.into_iter().map(String::from).collect(),
                prefixes: Vec::new(),
            },
            upstream: crate::commands::opencodex::CodexRoutingUpstream {
                base_url: None,
                api_format: Some(api_format.to_string()),
                auth: None,
                api_key: None,
                model_map: model_map
                    .into_iter()
                    .map(|(from, to)| (from.to_string(), to.to_string()))
                    .collect(),
                extra: BTreeMap::new(),
            },
            capabilities: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn resolves_namespaced_model() {
        let map = map_with(vec![entry(
            "zhipu",
            "https://open.bigmodel.cn/api/paas/v4",
            vec!["glm-4", "glm-4-plus"],
        )]);
        let r = resolve_route("zhipu/glm-4", &map).unwrap();
        assert_eq!(r.provider_name, "zhipu");
        assert_eq!(r.model_id, "glm-4");
        assert_eq!(r.upstream_base_url, "https://open.bigmodel.cn/api/paas/v4");
        assert_eq!(r.wire_api, "chat");
    }

    #[test]
    fn resolves_bare_model_when_unique() {
        let map = map_with(vec![entry("zhipu", "https://x", vec!["glm-4"])]);
        let r = resolve_route("glm-4", &map).unwrap();
        assert_eq!(r.provider_name, "zhipu");
    }

    #[test]
    fn bare_model_ambiguous_returns_none() {
        let map = map_with(vec![
            entry("zhipu", "https://a", vec!["glm-4"]),
            entry("deepseek", "https://b", vec!["glm-4"]),
        ]);
        assert!(resolve_route("glm-4", &map).is_none());
    }

    #[test]
    fn empty_string_returns_none() {
        let map = map_with(vec![]);
        assert!(resolve_route("", &map).is_none());
        assert!(resolve_route("  ", &map).is_none());
    }

    #[test]
    fn unknown_namespaced_returns_none() {
        let map = map_with(vec![entry("zhipu", "https://x", vec!["glm-4"])]);
        assert!(resolve_route("openai/gpt-5", &map).is_none());
    }

    #[test]
    fn empty_models_list_falls_back_when_provider_unique() {
        let map = map_with(vec![entry("local", "http://127.0.0.1:11434/v1", vec![])]);
        let r = resolve_route("llama3", &map).unwrap();
        assert_eq!(r.provider_name, "local");
        assert_eq!(r.model_id, "llama3");
    }

    #[test]
    fn legacy_opencodex_proxy_does_not_hijack_unknown_models() {
        let map = map_with(vec![entry("opencodex", "http://127.0.0.1:8765/v1", vec![])]);

        assert!(resolve_route("gpt-5.5", &map).is_none());
        assert!(resolve_route("opencodex/gpt-5.5", &map).is_none());
    }

    #[test]
    fn catalog_direct_legacy_opencodex_provider_is_ignored() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry("gpt-5.5", "opencodex", None, Some("gpt-5.5"))];
        let providers = vec![provider_route(
            "opencodex",
            "http://127.0.0.1:8765/v1",
            None,
        )];

        assert!(
            resolve_catalog_route_from_sources("gpt-5.5", &catalog, &providers, &map).is_none()
        );
    }

    #[test]
    fn catalog_backend_provider_resolves_from_opencodex_providers_when_not_in_inject_map() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax",
            "opencodex",
            Some("minimax"),
            Some("MiniMax-M1"),
        )];
        let providers = vec![provider_route(
            "minimax",
            "https://api.minimaxi.com/v1",
            Some("${MINIMAX_API_KEY}"),
        )];

        let r = resolve_catalog_route_from_sources("minimax", &catalog, &providers, &map).unwrap();

        assert_eq!(r.provider_name, "minimax");
        assert_eq!(r.model_id, "MiniMax-M1");
        assert_eq!(r.upstream_base_url, "https://api.minimaxi.com/v1");
        assert_eq!(r.env_key.as_deref(), Some("MINIMAX_API_KEY"));
    }

    #[test]
    fn catalog_provider_local_api_key_is_used_in_memory() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax",
            "codex_local_access",
            Some("minimax"),
            Some("MiniMax-M3"),
        )];
        let mut provider = provider_route("minimax", "https://api.minimaxi.com/v1", None);
        provider
            .extra
            .insert("api_key".to_string(), serde_json::json!("sk-plaintext"));

        let r = resolve_catalog_route_from_sources("minimax", &catalog, &[provider], &map)
            .expect("catalog route should resolve with local API key");

        assert_eq!(r.provider_name, "minimax");
        assert!(r.env_key.is_none());
        assert_eq!(r.api_key.as_deref(), Some("sk-plaintext"));
    }

    #[test]
    fn catalog_multirouter_route_resolves_to_target_provider() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax-m3",
            "codex_model_router_v2",
            Some("codex_model_router_v2"),
            Some("minimax-m3"),
        )];
        let providers = vec![
            router_provider(
                "codex_model_router_v2",
                vec![codex_route(
                    "minimax",
                    "minimax",
                    vec!["minimax-m3"],
                    vec![("minimax-m3", "MiniMax-M3")],
                    "openai_responses",
                )],
            ),
            provider_route(
                "minimax",
                "https://api.minimaxi.com/v1",
                Some("${MINIMAX_API_KEY}"),
            ),
        ];

        let r = resolve_catalog_route_from_sources("minimax-m3", &catalog, &providers, &map)
            .expect("multirouter route should resolve");

        assert_eq!(r.provider_name, "minimax");
        assert_eq!(r.model_id, "MiniMax-M3");
        assert_eq!(r.upstream_base_url, "https://api.minimaxi.com/v1");
        assert_eq!(r.wire_api, "responses");
        assert_eq!(r.env_key.as_deref(), Some("MINIMAX_API_KEY"));
    }

    #[test]
    fn native_openai_catalog_model_prefers_multirouter_official_route() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry("gpt-5.5", "openai", None, Some("gpt-5.5"))];
        let official_route = CodexRoutingRoute {
            id: "openai-official".to_string(),
            label: Some("OpenAI Official".to_string()),
            enabled: Some(true),
            target_provider_id: None,
            match_rule: crate::commands::opencodex::CodexRoutingMatch {
                models: vec!["gpt-5.5".to_string()],
                prefixes: vec!["gpt-".to_string()],
            },
            upstream: crate::commands::opencodex::CodexRoutingUpstream {
                base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
                api_format: Some("openai_responses".to_string()),
                auth: Some(serde_json::json!({ "source": "managed_codex_oauth" })),
                api_key: None,
                model_map: BTreeMap::from([("gpt-5.5".to_string(), "gpt-5.5".to_string())]),
                extra: BTreeMap::new(),
            },
            capabilities: None,
            extra: BTreeMap::new(),
        };
        let providers = vec![router_provider("codex_local_access", vec![official_route])];

        let r = resolve_catalog_route_from_sources("gpt-5.5", &catalog, &providers, &map)
            .expect("native GPT should resolve through the explicit official route");

        assert_eq!(r.provider_name, "OpenAI Official");
        assert_eq!(r.model_id, "gpt-5.5");
        assert_eq!(r.upstream_base_url, "https://chatgpt.com/backend-api/codex");
        assert_eq!(r.wire_api, "responses");
        assert_eq!(r.auth_source.as_deref(), Some("managed_codex_oauth"));
    }

    #[test]
    fn catalog_route_local_api_key_is_used_in_memory() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax-m3",
            "codex_model_router_v2",
            Some("codex_model_router_v2"),
            Some("minimax-m3"),
        )];
        let mut route = codex_route(
            "minimax",
            "minimax",
            vec!["minimax-m3"],
            vec![("minimax-m3", "MiniMax-M3")],
            "openai_responses",
        );
        route.upstream.api_key = Some("sk-route-plaintext".to_string());
        let providers = vec![
            router_provider("codex_model_router_v2", vec![route]),
            provider_route("minimax", "https://api.minimaxi.com/v1", None),
        ];

        let r = resolve_catalog_route_from_sources("minimax-m3", &catalog, &providers, &map)
            .expect("catalog route should resolve with local route API key");

        assert_eq!(r.provider_name, "minimax");
        assert!(r.env_key.is_none());
        assert_eq!(r.api_key.as_deref(), Some("sk-route-plaintext"));
    }

    #[test]
    fn catalog_multirouter_legacy_target_provider_is_ignored() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "gpt-5.5",
            "codex_local_access",
            Some("codex_local_access"),
            Some("gpt-5.5"),
        )];
        let providers = vec![
            router_provider(
                "codex_local_access",
                vec![codex_route(
                    "legacy",
                    "opencodex",
                    vec!["gpt-5.5"],
                    vec![("gpt-5.5", "gpt-5.5")],
                    "openai_responses",
                )],
            ),
            provider_route("opencodex", "http://127.0.0.1:8765/v1", None),
        ];

        assert!(
            resolve_catalog_route_from_sources("gpt-5.5", &catalog, &providers, &map).is_none()
        );
    }

    #[test]
    fn catalog_multirouter_route_preserves_chat_reasoning_config() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax-m3",
            "codex_model_router_v2",
            Some("codex_model_router_v2"),
            Some("minimax-m3"),
        )];
        let mut route = codex_route(
            "minimax",
            "minimax",
            vec!["minimax-m3"],
            vec![("minimax-m3", "MiniMax-M3")],
            "openai_chat",
        );
        route.upstream.extra.insert(
            "codexChatReasoning".to_string(),
            serde_json::json!({
                "supportsThinking": true,
                "supportsEffort": false,
                "thinkingParam": "reasoning_split",
                "effortParam": "none",
                "outputFormat": "reasoning_details"
            }),
        );
        let providers = vec![
            router_provider("codex_model_router_v2", vec![route]),
            provider_route(
                "minimax",
                "https://api.minimaxi.com/v1",
                Some("${MINIMAX_API_KEY}"),
            ),
        ];

        let r = resolve_catalog_route_from_sources("minimax-m3", &catalog, &providers, &map)
            .expect("multirouter route should resolve");
        let chat_reasoning = r
            .chat_reasoning
            .expect("route-level codexChatReasoning should be preserved");

        assert_eq!(r.wire_api, "chat");
        assert_eq!(chat_reasoning.supports_thinking, Some(true));
        assert_eq!(chat_reasoning.supports_effort, Some(false));
        assert_eq!(
            chat_reasoning.thinking_param.as_deref(),
            Some("reasoning_split")
        );
        assert_eq!(
            chat_reasoning.output_format.as_deref(),
            Some("reasoning_details")
        );
    }

    #[test]
    fn catalog_multirouter_route_preserves_text_only_capability() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "minimax-m3",
            "codex_model_router_v2",
            Some("codex_model_router_v2"),
            Some("minimax-m3"),
        )];
        let mut route = codex_route(
            "minimax",
            "minimax",
            vec!["minimax-m3"],
            vec![("minimax-m3", "MiniMax-M3")],
            "openai_chat",
        );
        route.capabilities = Some(serde_json::json!({
            "textOnly": true,
            "inputModalities": ["text"]
        }));
        let providers = vec![
            router_provider("codex_model_router_v2", vec![route]),
            provider_route(
                "minimax",
                "https://api.minimaxi.com/v1",
                Some("${MINIMAX_API_KEY}"),
            ),
        ];

        let r = resolve_catalog_route_from_sources("minimax-m3", &catalog, &providers, &map)
            .expect("multirouter route should resolve");

        assert!(r.text_only);
    }

    #[test]
    fn catalog_entry_preserves_vision_bridge_config() {
        let map = map_with(vec![]);
        let mut entry = catalog_entry("minimax-m3", "minimax", Some("minimax"), Some("MiniMax-M3"));
        entry.vision_bridge_enabled = Some(true);
        entry.vision_fallback_base_url = Some("https://vision.example.com/v1".to_string());
        entry.vision_fallback_model = Some("vision-model".to_string());
        entry.vision_fallback_api_key_ref = Some("${VISION_API_KEY}".to_string());
        let providers = vec![provider_route(
            "minimax",
            "https://api.minimaxi.com/v1",
            Some("${MINIMAX_API_KEY}"),
        )];

        let r = resolve_catalog_route_from_sources("minimax-m3", &[entry], &providers, &map)
            .expect("catalog route should resolve");
        let vision = r
            .vision_bridge
            .expect("catalog vision fallback should be preserved");

        assert_eq!(vision.base_url, "https://vision.example.com/v1");
        assert_eq!(vision.model, "vision-model");
        assert_eq!(vision.env_key.as_deref(), Some("VISION_API_KEY"));
    }

    #[test]
    fn catalog_multirouter_default_route_is_used_when_no_match() {
        let map = map_with(vec![]);
        let catalog = vec![catalog_entry(
            "deepseek-v4",
            "codex_model_router_v2",
            Some("codex_model_router_v2"),
            Some("deepseek-v4"),
        )];
        let mut router = router_provider(
            "codex_model_router_v2",
            vec![codex_route(
                "deepseek",
                "deepseek",
                vec![],
                vec![("deepseek-v4", "deepseek-chat")],
                "openai_chat",
            )],
        );
        router
            .codex_routing
            .as_mut()
            .expect("router")
            .default_route_id = Some("deepseek".to_string());
        let providers = vec![
            router,
            provider_route(
                "deepseek",
                "https://api.deepseek.com/v1",
                Some("$DEEPSEEK_API_KEY"),
            ),
        ];

        let r = resolve_catalog_route_from_sources("deepseek-v4", &catalog, &providers, &map)
            .expect("default multirouter route should resolve");

        assert_eq!(r.provider_name, "deepseek");
        assert_eq!(r.model_id, "deepseek-chat");
        assert_eq!(r.wire_api, "chat");
        assert_eq!(r.env_key.as_deref(), Some("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn detects_native_openai_catalog_model() {
        let catalog = vec![
            catalog_entry("gpt-5.5", "openai", None, Some("gpt-5.5")),
            catalog_entry(
                "minimax",
                "codex_local_access",
                Some("minimax"),
                Some("MiniMax-M3"),
            ),
        ];

        assert!(is_native_openai_catalog_model_from_sources(
            "gpt-5.5", &catalog
        ));
        assert!(is_native_openai_catalog_model_from_sources(
            "openai/gpt-5.5",
            &catalog
        ));
        assert!(!is_native_openai_catalog_model_from_sources(
            "minimax", &catalog
        ));
    }
}
