// src-tauri/src/proxy/models_endpoint.rs
//
// GET /v1/models: 返回 Codex Box 代理看到的合并模型列表。
//
// 数据源:
//   1. ~/.codex/config.toml  [model_providers.*].models
//   2. ~/.opencodex/custom_model_catalog.json  visible=true 条目
//
// 输出: OpenAI /v1/models 标准 schema
//   { "object": "list", "data": [{ "id": "...", "object": "model", "owned_by": "...", ... }] }
//
// id 格式: "provider_name/model_id" (routing 模块按此格式解析)
use crate::commands::opencodex::{self as opencodex_cmd, ModelCatalogEntry};
use crate::error::AppResult;
use crate::proxy::inject_map::InjectMap;
use serde::Serialize;

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
        for model_id in &entry.models {
            data.push(MergedModelEntry {
                id: format!("{}/{}", entry.name, model_id),
                object: "model",
                owned_by: entry.name.clone(),
                provider: entry.name.clone(),
                source: "config",
                visible: true,
                display_name: None,
            });
        }
        // models 为空时,保留 provider 自身作为占位(让用户知道有这个 provider)
        if entry.models.is_empty() {
            data.push(MergedModelEntry {
                id: entry.name.clone(),
                object: "model",
                owned_by: entry.name.clone(),
                provider: entry.name.clone(),
                source: "config",
                visible: true,
                display_name: None,
            });
        }
    }

    // 2. custom_model_catalog(visible=true 才合并)
    for entry in catalog.iter().filter(|c| c.visible) {
        let id = entry.model_id.clone();
        // 避免与 inject-map 重复
        if data.iter().any(|d| d.id == id) {
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
        });
    }

    Ok(MergedModelsResponse {
        object: "list",
        data,
    })
}

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
        assert!(ids.contains(&"openai/gpt-5"));
        assert!(ids.contains(&"openai/gpt-5-mini"));
        assert!(ids.contains(&"zhipu/glm-4"));
        assert_eq!(resp.data.len(), 3);
    }

    #[test]
    fn empty_models_produces_provider_placeholder() {
        let map = InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![entry("local", vec![])],
        };
        let resp = merged_models_with_catalog(&map, &[]).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].id, "local");
        assert_eq!(resp.data[0].provider, "local");
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
        assert_eq!(entry["id"], "zhipu/glm-4");
        assert_eq!(entry["object"], "model");
        assert_eq!(entry["owned_by"], "zhipu");
    }
}
