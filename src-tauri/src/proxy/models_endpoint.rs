// src-tauri/src/proxy/models_endpoint.rs
//
// GET /v1/models: 返回 Codex Box 代理看到的合并模型列表。
//
// 数据源:
//   1. ~/.codex/config.toml  [model_providers.*].models
//   2. ~/.codex/codex-box/custom_model_catalog.json visible=true 条目
//
// 输出: OpenAI /v1/models 标准 schema
//   { "object": "list", "data": [{ "id": "...", "object": "model", "owned_by": "...", ... }] }
//
// id 格式: 优先使用 catalog 裸 slug/model_id, 对齐 Codex picker 行为。
use crate::commands::opencodex::{self as opencodex_cmd, ModelCatalogEntry};
use crate::error::AppResult;
use crate::proxy::inject_map::InjectMap;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct MergedModelsResponse {
    pub object: &'static str,
    pub data: Vec<MergedModelEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergedModelEntry {
    pub id: String,
    pub object: &'static str,
    /// owner/provider(OpenAI 字段:owned_by)
    pub owned_by: String,
    /// provider name(Codex Box 扩展)
    pub provider: String,
    /// source: "config" | "catalog"
    pub source: &'static str,
    /// visible toggle 是否在 picker 中显示
    pub visible: bool,
    /// display name(若有)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Codex Desktop 不同版本会读 snake_case 或 camelCase 元数据。
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// 合并 inject-map + custom_model_catalog → 合并模型列表
pub fn merged_models(map: &InjectMap) -> AppResult<MergedModelsResponse> {
    let catalog = opencodex_cmd::opencodex_config_read()
        .map(|cfg| cfg.catalog)
        .unwrap_or_default();
    merged_models_with_catalog(map, &catalog)
}

fn merged_models_with_catalog(
    map: &InjectMap,
    catalog: &[ModelCatalogEntry],
) -> AppResult<MergedModelsResponse> {
    let mut data = Vec::new();

    // 1. inject-map providers(从 config.toml 来的)
    for entry in &map.providers {
        if is_legacy_opencodex_proxy_route(&entry.name, &entry.original_base_url) {
            continue;
        }
        for model_id in &entry.models {
            data.push(MergedModelEntry {
                id: model_id.clone(),
                object: "model",
                owned_by: entry.name.clone(),
                provider: entry.name.clone(),
                source: "config",
                visible: true,
                display_name: None,
                extra: BTreeMap::new(),
            });
        }
    }

    // 2. custom_model_catalog(visible=true 才合并)。如果 inject-map 已有同名模型,
    // catalog 仍是 picker 展示元数据的权威来源,避免旧注入状态盖掉 displayName/provider。
    for entry in catalog.iter().filter(|c| c.visible) {
        let id = entry.model_id.clone();
        if let Some(existing) = data.iter_mut().find(|model| model.id == id) {
            existing.owned_by = entry.provider.clone();
            existing.provider = entry.provider.clone();
            existing.source = "catalog";
            existing.visible = entry.visible;
            existing.display_name = entry.display_name.clone();
            existing.extra.extend(catalog_model_metadata(entry));
            continue;
        }
        data.push(MergedModelEntry {
            id,
            object: "model",
            owned_by: entry.provider.clone(),
            provider: entry.provider.clone(),
            source: "catalog",
            visible: entry.visible,
            display_name: entry.display_name.clone(),
            extra: catalog_model_metadata(entry),
        });
    }

    Ok(MergedModelsResponse {
        object: "list",
        data,
    })
}

fn is_legacy_opencodex_proxy_route(name: &str, base_url: &str) -> bool {
    name.eq_ignore_ascii_case("opencodex")
        || base_url.contains("127.0.0.1:8765")
        || base_url.contains("localhost:8765")
}

fn catalog_model_metadata(entry: &ModelCatalogEntry) -> BTreeMap<String, Value> {
    let mut extra = BTreeMap::new();

    if let Some(display_name) = entry.display_name.as_deref() {
        extra.insert("displayName".to_string(), json!(display_name));
    }
    if let Some(backend_model) = entry.backend_model.as_deref() {
        extra.insert("backend_model".to_string(), json!(backend_model));
        extra.insert("backendModel".to_string(), json!(backend_model));
    }
    let target_provider = extra_string(
        &entry.extra,
        &[
            "targetProvider",
            "target_provider",
            "upstreamProvider",
            "upstream_provider",
        ],
    );
    if let Some(backend_provider) = entry.backend_provider.as_deref() {
        let display_provider = target_provider.as_deref().unwrap_or(backend_provider);
        extra.insert("backend_provider".to_string(), json!(display_provider));
        extra.insert("backendProvider".to_string(), json!(display_provider));
        if target_provider
            .as_deref()
            .is_some_and(|target| target != backend_provider)
        {
            extra.insert("router_provider".to_string(), json!(backend_provider));
            extra.insert("routerProvider".to_string(), json!(backend_provider));
        }
    } else if let Some(target_provider) = target_provider.as_deref() {
        extra.insert("backend_provider".to_string(), json!(target_provider));
        extra.insert("backendProvider".to_string(), json!(target_provider));
    }
    if let Some(reasoning) = &entry.reasoning {
        extra.insert(
            "reasoning".to_string(),
            json!({
                "enabled": reasoning.enabled,
                "levels": reasoning.levels,
            }),
        );
    }

    for key in MODEL_METADATA_KEYS {
        if let Some(value) = entry.extra.get(*key) {
            extra.insert((*key).to_string(), value.clone());
        }
    }

    copy_alias(
        &mut extra,
        &entry.extra,
        &["context_window", "contextWindow", "model_context_window"],
        "context_window",
        "contextWindow",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &["max_context_window", "maxContextWindow"],
        "max_context_window",
        "maxContextWindow",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &["auto_compact_token_limit", "autoCompactTokenLimit"],
        "auto_compact_token_limit",
        "autoCompactTokenLimit",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &[
            "default_reasoning_level",
            "default_reasoning_effort",
            "defaultReasoningEffort",
        ],
        "default_reasoning_effort",
        "defaultReasoningEffort",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &[
            "supported_reasoning_levels",
            "supported_reasoning_efforts",
            "supportedReasoningEfforts",
        ],
        "supported_reasoning_efforts",
        "supportedReasoningEfforts",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &["input_modalities", "inputModalities"],
        "input_modalities",
        "inputModalities",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &["supports_parallel_tool_calls", "supportsParallelToolCalls"],
        "supports_parallel_tool_calls",
        "supportsParallelToolCalls",
    );
    copy_alias(
        &mut extra,
        &entry.extra,
        &[
            "supports_image_detail_original",
            "supportsImageDetailOriginal",
        ],
        "supports_image_detail_original",
        "supportsImageDetailOriginal",
    );

    extra
}

fn copy_alias(
    out: &mut BTreeMap<String, Value>,
    source: &BTreeMap<String, Value>,
    candidates: &[&str],
    snake_key: &str,
    camel_key: &str,
) {
    let Some(value) = candidates
        .iter()
        .filter_map(|key| source.get(*key))
        .next()
        .cloned()
    else {
        return;
    };
    out.entry(snake_key.to_string())
        .or_insert_with(|| value.clone());
    out.entry(camel_key.to_string()).or_insert(value);
}

fn extra_string(source: &BTreeMap<String, Value>, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|key| source.get(*key))
        .find_map(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

const MODEL_METADATA_KEYS: &[&str] = &[
    "description",
    "priority",
    "supported_in_api",
    "available_in_plans",
    "minimal_client_version",
    "prefer_websockets",
    "default_verbosity",
    "support_verbosity",
    "default_reasoning_summary",
    "reasoning_summary_format",
    "supports_reasoning_summaries",
    "supports_search_tool",
    "web_search_tool_type",
    "apply_patch_tool_type",
    "shell_type",
    "experimental_supported_tools",
    "truncation_policy",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::inject_map::{InjectMap, InjectMapEntry};
    use std::collections::BTreeMap;

    fn entry(name: &str, models: Vec<&str>) -> InjectMapEntry {
        InjectMapEntry {
            name: name.to_string(),
            original_base_url: format!("https://api.{name}.example/v1"),
            env_key: None,
            http_headers: BTreeMap::new(),
            wire_api: "chat".to_string(),
            models: models.into_iter().map(String::from).collect(),
            kind: "compatible_api".to_string(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn merge_from_inject_map_only() {
        let map = InjectMap {
            updated_at: "2026-06-25T00:00:00Z".to_string(),
            port: 1455,
            providers: vec![
                entry("openai", vec!["gpt-5", "gpt-5-mini"]),
                entry("zhipu", vec!["glm-4"]),
            ],
        };
        let resp = merged_models_with_catalog(&map, &[]).unwrap();
        assert_eq!(resp.object, "list");
        let ids: Vec<&str> = resp.data.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"gpt-5"));
        assert!(ids.contains(&"gpt-5-mini"));
        assert!(ids.contains(&"glm-4"));
        assert_eq!(resp.data.len(), 3);
    }

    #[test]
    fn legacy_opencodex_provider_is_hidden_from_models_endpoint() {
        let mut legacy = entry("opencodex", vec!["gpt-5.5"]);
        legacy.original_base_url = "http://127.0.0.1:8765/v1".to_string();
        let map = InjectMap {
            updated_at: "2026-06-25T00:00:00Z".to_string(),
            port: 1455,
            providers: vec![legacy, entry("minimax", vec!["minimax-m3"])],
        };

        let resp = merged_models_with_catalog(&map, &[]).unwrap();
        let ids: Vec<&str> = resp.data.iter().map(|m| m.id.as_str()).collect();

        assert_eq!(ids, vec!["minimax-m3"]);
    }

    #[test]
    fn empty_models_does_not_produce_provider_placeholder() {
        let map = InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![entry("local", vec![])],
        };
        let resp = merged_models_with_catalog(&map, &[]).unwrap();
        assert!(resp.data.is_empty());
    }

    #[test]
    fn response_shape_is_openai_compatible() {
        let map = InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![entry("zhipu", vec!["glm-4"])],
        };
        let resp = merged_models_with_catalog(&map, &[]).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "list");
        let entry = &json["data"][0];
        assert_eq!(entry["id"], "glm-4");
        assert_eq!(entry["object"], "model");
        assert_eq!(entry["owned_by"], "zhipu");
    }

    #[test]
    fn catalog_entries_project_codex_picker_metadata() {
        let map = InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![],
        };
        let catalog = vec![ModelCatalogEntry {
            model_id: "minimax-m3".to_string(),
            display_name: Some("MiniMax-M3".to_string()),
            provider: "codex_local_access".to_string(),
            backend_model: Some("MiniMax-M3".to_string()),
            backend_provider: Some("minimax".to_string()),
            visible: true,
            reasoning: Some(opencodex_cmd::ReasoningConfig {
                enabled: true,
                levels: vec!["medium".to_string()],
            }),
            note: None,
            vision_bridge_enabled: None,
            vision_fallback_base_url: None,
            vision_fallback_model: None,
            vision_fallback_api_key_ref: None,
            extra: BTreeMap::from([
                ("context_window".to_string(), json!(200000)),
                ("max_context_window".to_string(), json!(1000000)),
                ("default_reasoning_level".to_string(), json!("medium")),
                (
                    "supported_reasoning_levels".to_string(),
                    json!([{ "effort": "medium", "description": "Balanced" }]),
                ),
                ("input_modalities".to_string(), json!(["text", "image"])),
                ("supports_parallel_tool_calls".to_string(), json!(true)),
            ]),
        }];

        let resp = merged_models_with_catalog(&map, &catalog).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        let model = &json["data"][0];

        assert_eq!(model["id"], "minimax-m3");
        assert_eq!(model["display_name"], "MiniMax-M3");
        assert_eq!(model["displayName"], "MiniMax-M3");
        assert_eq!(model["backendProvider"], "minimax");
        assert_eq!(model["context_window"], 200000);
        assert_eq!(model["contextWindow"], 200000);
        assert_eq!(model["max_context_window"], 1000000);
        assert_eq!(model["maxContextWindow"], 1000000);
        assert_eq!(model["defaultReasoningEffort"], "medium");
        assert_eq!(model["inputModalities"], json!(["text", "image"]));
        assert_eq!(model["supportsParallelToolCalls"], true);
    }

    #[test]
    fn catalog_metadata_overrides_duplicate_inject_map_entry() {
        let map = InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![entry(
                "codex_model_router_v2",
                vec!["gpt-5.5", "minimax-m3"],
            )],
        };
        let catalog = vec![
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
                extra: BTreeMap::from([("context_window".to_string(), json!(272000))]),
            },
            ModelCatalogEntry {
                model_id: "minimax-m3".to_string(),
                display_name: Some("MiniMax-M3".to_string()),
                provider: "codex_model_router_v2".to_string(),
                backend_model: Some("MiniMax-M3".to_string()),
                backend_provider: Some("codex_model_router_v2".to_string()),
                visible: true,
                reasoning: None,
                note: None,
                vision_bridge_enabled: None,
                vision_fallback_base_url: None,
                vision_fallback_model: None,
                vision_fallback_api_key_ref: None,
                extra: BTreeMap::from([("targetProvider".to_string(), json!("minimax"))]),
            },
        ];

        let resp = merged_models_with_catalog(&map, &catalog).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        let models = json["data"].as_array().unwrap();
        assert_eq!(models.len(), 2);

        let gpt = models
            .iter()
            .find(|model| model["id"] == "gpt-5.5")
            .expect("gpt model");
        assert_eq!(gpt["provider"], "openai");
        assert_eq!(gpt["owned_by"], "openai");
        assert_eq!(gpt["source"], "catalog");
        assert_eq!(gpt["displayName"], "GPT-5.5");
        assert_eq!(gpt["contextWindow"], 272000);

        let minimax = models
            .iter()
            .find(|model| model["id"] == "minimax-m3")
            .expect("minimax model");
        assert_eq!(minimax["provider"], "codex_model_router_v2");
        assert_eq!(minimax["backendProvider"], "minimax");
        assert_eq!(minimax["routerProvider"], "codex_model_router_v2");
        assert_eq!(minimax["displayName"], "MiniMax-M3");
    }
}
