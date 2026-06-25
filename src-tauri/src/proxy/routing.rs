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
use crate::proxy::inject_map::InjectMap;

/// 解析后的路由
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRoute {
    pub provider_name: String,
    pub model_id: String,
    pub upstream_base_url: String,
    pub wire_api: String,
    pub env_key: Option<String>,
    pub http_headers: std::collections::BTreeMap<String, String>,
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
    let entry = cfg.catalog.iter().find(|entry| {
        entry.visible
            && (entry.model_id == requested
                || format!("{}/{}", entry.provider, entry.model_id) == requested)
    })?;

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

    let provider = map.providers.iter().find(|p| p.name == provider_name)?;
    Some(ResolvedRoute {
        provider_name: provider.name.clone(),
        model_id: upstream_model.to_string(),
        upstream_base_url: provider.original_base_url.clone(),
        wire_api: provider.wire_api.clone(),
        env_key: provider.env_key.clone(),
        http_headers: provider.http_headers.clone(),
    })
}

fn build_route(entry: &crate::proxy::inject_map::InjectMapEntry, model_id: &str) -> ResolvedRoute {
    ResolvedRoute {
        provider_name: entry.name.clone(),
        model_id: model_id.to_string(),
        upstream_base_url: entry.original_base_url.clone(),
        wire_api: entry.wire_api.clone(),
        env_key: entry.env_key.clone(),
        http_headers: entry.http_headers.clone(),
    }
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
}
