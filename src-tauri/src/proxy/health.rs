// src-tauri/src/proxy/health.rs
//
// /healthz 已在 server.rs 暴露。这里保留健康检查的辅助函数
// (供 Tauri command 在不发起 HTTP 请求时直接读取内存中的状态)。
use crate::proxy::state::ProxyState;

/// 简单 health check: 仅依赖内存中的 ProxyState。
/// 返回 true 表示代理处于 Running 状态且至少配置了一个 provider。
pub fn is_healthy(state: &ProxyState) -> bool {
    use crate::proxy::state::ProxyStatus;
    state.status() == ProxyStatus::Running && !state.inject_map().providers.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::inject_map::{InjectMap, InjectMapEntry};
    use crate::proxy::state::ProxyStatus;
    use std::collections::BTreeMap;

    #[test]
    fn stopped_is_not_healthy() {
        let s = ProxyState::new();
        assert!(!is_healthy(&s));
    }

    #[test]
    fn running_without_providers_is_not_healthy() {
        let s = ProxyState::new();
        s.set_status(ProxyStatus::Running);
        assert!(!is_healthy(&s));
    }

    #[test]
    fn running_with_provider_is_healthy() {
        let s = ProxyState::new();
        s.set_status(ProxyStatus::Running);
        s.set_inject_map(InjectMap {
            updated_at: "".to_string(),
            port: 1455,
            providers: vec![InjectMapEntry {
                name: "zhipu".to_string(),
                original_base_url: "https://x".to_string(),
                env_key: None,
                http_headers: BTreeMap::new(),
                wire_api: "chat".to_string(),
                models: vec!["glm-4".to_string()],
                kind: "compatible_api".to_string(),
                extra: BTreeMap::new(),
            }],
        });
        assert!(is_healthy(&s));
    }
}
