use crate::error::{AppError, AppResult};
use crate::proxy::state::{ProxyState, ProxyStatus};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const CODEX_BOX_CATALOG_PATH: &str = ".codex/codex-box/custom_model_catalog.json";
const CODEX_BOX_PROVIDERS_PATH: &str = ".codex/codex-box/providers.json";
const CODEX_BOX_INJECT_MAP_PATH: &str = ".codex/codex-box/inject-map.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRoutingStatus {
    pub config_path: String,
    pub current_model: Option<String>,
    pub model_provider: String,
    pub request_base_url: Option<String>,
    pub request_base_url_source: String,
    pub model_catalog_path: Option<String>,
    pub catalog_configured: bool,
    pub catalog_model_found: bool,
    pub catalog_provider: Option<String>,
    pub backend_provider: Option<String>,
    pub backend_model: Option<String>,
    pub upstream_base_url: Option<String>,
    pub proxy_running: bool,
    pub proxy_port: Option<u16>,
    pub issues: Vec<EffectiveRoutingIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRoutingIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct CatalogRoute {
    model_id: String,
    provider: Option<String>,
    backend_provider: Option<String>,
    backend_model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct UpstreamProvider {
    name: String,
    base_url: Option<String>,
    api_key_env: Option<String>,
    plaintext_api_key_present: bool,
}

#[tauri::command]
pub fn effective_routing_status(
    state: tauri::State<Arc<ProxyState>>,
) -> AppResult<EffectiveRoutingStatus> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let catalog_path = home.join(CODEX_BOX_CATALOG_PATH);
    let providers_path = home.join(CODEX_BOX_PROVIDERS_PATH);
    let inject_map_path = home.join(CODEX_BOX_INJECT_MAP_PATH);

    let config_raw = std::fs::read_to_string(&config_path).map_err(AppError::Io)?;
    let catalog_raw = std::fs::read_to_string(&catalog_path).ok();
    let providers_raw = std::fs::read_to_string(&providers_path).ok();
    let inject_map_raw = std::fs::read_to_string(&inject_map_path).ok();
    crate::proxy::lifecycle::reconcile_running_state(state.inner());
    let proxy_view = state.inner().to_view();
    let proxy_running = state.inner().status() == ProxyStatus::Running;
    let proxy_port = if proxy_view.port > 0 {
        Some(proxy_view.port)
    } else {
        None
    };

    build_effective_routing_status(
        &config_path,
        &config_raw,
        catalog_raw.as_deref(),
        providers_raw.as_deref(),
        inject_map_raw.as_deref(),
        proxy_running,
        proxy_port,
    )
}

fn build_effective_routing_status(
    config_path: &Path,
    config_raw: &str,
    catalog_raw: Option<&str>,
    providers_raw: Option<&str>,
    inject_map_raw: Option<&str>,
    proxy_running: bool,
    proxy_port: Option<u16>,
) -> AppResult<EffectiveRoutingStatus> {
    let mut issues = Vec::new();
    let config: toml::Value = toml::from_str(config_raw)?;
    let table = config
        .as_table()
        .ok_or_else(|| AppError::Command("config.toml 顶层不是 table".to_string()))?;

    let current_model = string_at(table, "model");
    let model_provider = string_at(table, "model_provider").unwrap_or_else(|| "openai".to_string());
    let catalog_path = string_at(table, "model_catalog_json");
    let catalog_configured = catalog_path.is_some();
    if !catalog_configured {
        issues.push(issue(
            "warn",
            "catalog_not_configured",
            "未配置 model_catalog_json，Codex App 不会显式加载 Codex Box 模型目录 ~/.codex/codex-box/custom_model_catalog.json。",
        ));
    }

    let (request_base_url, request_base_url_source) =
        resolve_request_base_url(table, &model_provider, &mut issues);
    if let Some(url) = request_base_url.as_deref() {
        check_local_proxy_url(url, proxy_running, proxy_port, &mut issues);
    } else {
        issues.push(issue(
            "fail",
            "request_entry_not_configured",
            format!(
                "当前 model_provider={model_provider} 没有指向 Codex Box 本地代理的请求入口，Codex App 不会把请求发到 BYOK runtime。"
            ),
        ));
    }

    let catalog = catalog_raw.map(parse_catalog_routes).unwrap_or_default();
    let catalog_match = current_model
        .as_deref()
        .and_then(|model| catalog.iter().find(|entry| entry.model_id == model));
    let catalog_model_found = catalog_match.is_some();
    if current_model.is_some() && catalog_configured && !catalog_model_found {
        issues.push(issue(
            "warn",
            "model_not_in_catalog",
            "当前 model 没有在 model_catalog_json 中找到，Codex 下拉和真实路由可能不一致。",
        ));
    }

    let catalog_provider = catalog_match.and_then(|entry| entry.provider.clone());
    let backend_provider = catalog_match
        .and_then(|entry| entry.backend_provider.clone())
        .or_else(|| catalog_provider.clone());
    let backend_model = catalog_match
        .and_then(|entry| entry.backend_model.clone())
        .or_else(|| current_model.clone());

    let upstreams = providers_raw
        .map(parse_upstream_providers)
        .unwrap_or_default();
    check_legacy_inject_map(inject_map_raw, &mut issues);

    let upstream_provider = backend_provider
        .as_deref()
        .and_then(|provider| upstreams.iter().find(|entry| entry.name == provider));
    let upstream_base_url = upstream_provider.and_then(|entry| entry.base_url.clone());

    if let Some(provider) = backend_provider.as_deref() {
        if provider != "openai" {
            if upstream_base_url.is_none() {
                issues.push(issue(
                    "fail",
                    "backend_provider_missing",
                    format!("backend_provider={provider} 没有在 Codex Box 模型来源 ~/.codex/codex-box/providers.json 中找到对应上游 API。"),
                ));
            }

            if let Some(upstream) = upstream_provider {
                check_upstream_credentials(provider, upstream, &mut issues);
            }
        }
    }

    Ok(EffectiveRoutingStatus {
        config_path: config_path.display().to_string(),
        current_model,
        model_provider,
        request_base_url,
        request_base_url_source,
        model_catalog_path: catalog_path,
        catalog_configured,
        catalog_model_found,
        catalog_provider,
        backend_provider,
        backend_model,
        upstream_base_url,
        proxy_running,
        proxy_port,
        issues,
    })
}

fn check_legacy_inject_map(raw: Option<&str>, issues: &mut Vec<EffectiveRoutingIssue>) {
    let Some(raw) = raw else {
        return;
    };
    if raw.trim().is_empty() {
        return;
    }

    let Ok(map) = serde_json::from_str::<crate::proxy::inject_map::InjectMap>(raw) else {
        issues.push(issue(
            "warn",
            "inject_map_parse_failed",
            "inject-map.json 解析失败，Codex Box 会忽略该路由缓存；建议重新执行 MultiRouter 同步生成干净状态。",
        ));
        return;
    };

    let legacy_count = map
        .providers
        .iter()
        .filter(|entry| is_legacy_inject_map_entry(entry))
        .count();
    if legacy_count == 0 {
        return;
    }

    issues.push(issue(
        "warn",
        "legacy_inject_map_pending_cleanup",
        format!(
            "发现 {legacy_count} 条旧 OpenCodex/8765 inject-map 残留；MultiRouter 同步预览会展示差异，确认后清理，避免 Codex App 继续误打到旧本地端口。"
        ),
    ));
}

fn is_legacy_inject_map_entry(entry: &crate::proxy::inject_map::InjectMapEntry) -> bool {
    entry.name.trim().eq_ignore_ascii_case("opencodex")
        || is_legacy_opencodex_url(&entry.original_base_url)
}

fn is_legacy_opencodex_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    normalized.contains("127.0.0.1:8765") || normalized.contains("localhost:8765")
}

fn check_upstream_credentials(
    provider: &str,
    upstream: &UpstreamProvider,
    issues: &mut Vec<EffectiveRoutingIssue>,
) {
    if upstream.plaintext_api_key_present {
        issues.push(issue(
            "fail",
            "upstream_api_key_plaintext_ignored",
            format!(
                "backend_provider={provider} 的 api_key 是明文。Codex Box 不会使用文件里的明文 key；请改成 ${{ENV_VAR}} / api_key_ref 并把真实 key 放进环境变量。"
            ),
        ));
    }

    if let Some(env_name) = upstream.api_key_env.as_deref() {
        if std::env::var_os(env_name).is_none() {
            issues.push(issue(
                "fail",
                "upstream_api_key_env_missing",
                format!(
                    "backend_provider={provider} 的 api_key 引用 ${env_name}，但当前 Codex Box 进程环境没有该变量。请求已到达本地代理，但上游会返回 401；设置 {env_name} 后重启 Codex Box / Codex App。"
                ),
            ));
        }
        return;
    }

    if !upstream.plaintext_api_key_present {
        issues.push(issue(
            "fail",
            "upstream_api_key_missing",
            format!(
                "backend_provider={provider} 没有配置 api_key 或 api_key_ref。请求已到达本地代理，但上游不会接受未鉴权请求。"
            ),
        ));
    }
}

fn string_at(table: &toml::map::Map<String, toml::Value>, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn resolve_request_base_url(
    table: &toml::map::Map<String, toml::Value>,
    model_provider: &str,
    issues: &mut Vec<EffectiveRoutingIssue>,
) -> (Option<String>, String) {
    if model_provider == "openai" {
        return (
            string_at(table, "openai_base_url"),
            "openai_base_url".to_string(),
        );
    }

    let base_url = table
        .get("model_providers")
        .and_then(|value| value.as_table())
        .and_then(|providers| providers.get(model_provider))
        .and_then(|entry| entry.as_table())
        .and_then(|entry| entry.get("base_url"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    if base_url.is_none() {
        issues.push(issue(
            "fail",
            "model_provider_missing_base_url",
            format!("当前 model_provider={model_provider} 没有可用 base_url。"),
        ));
    }

    (
        base_url,
        format!("model_providers.{model_provider}.base_url"),
    )
}

fn check_local_proxy_url(
    url: &str,
    proxy_running: bool,
    proxy_port: Option<u16>,
    issues: &mut Vec<EffectiveRoutingIssue>,
) {
    let Some(port) = extract_localhost_port(url) else {
        return;
    };

    if port == 0 {
        issues.push(issue(
            "fail",
            "invalid_proxy_port_zero",
            "请求入口 base_url 指向 127.0.0.1:0，这是无效端口，Codex App 不可能连上。",
        ));
        return;
    }

    if !proxy_running {
        issues.push(issue(
            "fail",
            "proxy_not_running",
            "请求入口指向本地代理，但 Codex Box 代理当前未运行。",
        ));
        return;
    }

    if let Some(active_port) = proxy_port {
        if active_port != port {
            issues.push(issue(
                "fail",
                "proxy_port_mismatch",
                format!("请求入口指向端口 {port}，但 Codex Box 代理实际运行在端口 {active_port}。"),
            ));
        }
    }
}

fn extract_localhost_port(url: &str) -> Option<u16> {
    for needle in ["127.0.0.1:", "localhost:"] {
        if let Some(rest) = url.split_once(needle).map(|(_, rest)| rest) {
            let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            return digits.parse::<u16>().ok();
        }
    }
    None
}

fn parse_catalog_routes(raw: &str) -> Vec<CatalogRoute> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let array = value
        .as_object()
        .and_then(|obj| obj.get("models"))
        .and_then(|value| value.as_array())
        .or_else(|| value.as_array());
    let Some(array) = array else {
        return Vec::new();
    };

    array
        .iter()
        .filter_map(|entry| {
            let obj = entry.as_object()?;
            let model_id = obj
                .get("model_id")
                .or_else(|| obj.get("slug"))
                .or_else(|| obj.get("model"))
                .and_then(|value| value.as_str())?
                .to_string();
            let provider = obj
                .get("provider")
                .and_then(|value| value.as_str())
                .map(normalize_catalog_provider);
            let backend_provider = obj
                .get("backend_provider")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            Some(CatalogRoute {
                model_id,
                provider,
                backend_provider,
                backend_model: obj
                    .get("backend_model")
                    .or_else(|| obj.get("model"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            })
        })
        .collect()
}

fn normalize_catalog_provider(provider: &str) -> String {
    if provider.eq_ignore_ascii_case("opencodex") {
        "codex_local_access".to_string()
    } else {
        provider.to_string()
    }
}

fn parse_upstream_providers(raw: &str) -> Vec<UpstreamProvider> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let array = value
        .as_object()
        .and_then(|obj| obj.get("providers"))
        .and_then(|value| value.as_array())
        .or_else(|| value.as_array());
    let Some(array) = array else {
        return Vec::new();
    };

    array
        .iter()
        .filter_map(|entry| {
            let obj = entry.as_object()?;
            let name = obj
                .get("name")
                .and_then(|value| value.as_str())?
                .to_string();
            let api_key = obj.get("api_key").and_then(|value| value.as_str());
            let api_key_ref = obj.get("api_key_ref").and_then(|value| value.as_str());
            let api_key_env = api_key_ref
                .or_else(|| api_key.filter(|value| value.trim_start().starts_with('$')))
                .map(normalize_env_ref)
                .filter(|value| !value.is_empty());
            let plaintext_api_key_present = api_key
                .map(|value| {
                    let trimmed = value.trim();
                    !trimmed.is_empty() && !trimmed.starts_with('$')
                })
                .unwrap_or(false);

            Some(UpstreamProvider {
                name,
                base_url: obj
                    .get("base_url")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                api_key_env,
                plaintext_api_key_present,
            })
        })
        .collect()
}

fn normalize_env_ref(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(inner) = trimmed
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    {
        return inner.trim().to_string();
    }
    trimmed.trim_start_matches('$').to_string()
}

fn issue(
    severity: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> EffectiveRoutingIssue {
    EffectiveRoutingIssue {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_missing_catalog_config() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax"
model_provider = "openai"
"#,
            None,
            None,
            None,
            false,
            None,
        )
        .unwrap();

        assert!(!status.catalog_configured);
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "catalog_not_configured"));
    }

    #[test]
    fn status_reports_invalid_zero_port() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "gpt-5.5"
model_provider = "opencodex"
model_catalog_json = "/tmp/models.json"

[model_providers.opencodex]
base_url = "http://127.0.0.1:0/v1"
            "#,
            Some(r#"{"models":[]}"#),
            Some(r#"{"providers":[]}"#),
            None,
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "invalid_proxy_port_zero"));
    }

    #[test]
    fn status_resolves_catalog_entries_that_use_model_field() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax"
model_provider = "opencodex"
model_catalog_json = "/tmp/models.json"

[model_providers.opencodex]
base_url = "http://127.0.0.1:1455/v1"
"#,
            Some(
                r#"{"models":[{"model":"minimax","provider":"opencodex","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1"}]}"#,
            ),
            None,
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status.catalog_model_found);
        assert_eq!(
            status.catalog_provider.as_deref(),
            Some("codex_local_access")
        );
        assert_eq!(status.backend_provider.as_deref(), Some("minimax"));
        assert_eq!(
            status.upstream_base_url.as_deref(),
            Some("https://api.minimaxi.com/v1")
        );
    }

    #[test]
    fn status_reports_missing_upstream_env_key() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax"
model_provider = "openai"
model_catalog_json = "/tmp/models.json"
openai_base_url = "http://127.0.0.1:1455/v1"
"#,
            Some(
                r#"{"models":[{"model":"minimax","provider":"openai","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1","api_key":"$CODEX_BOX_TEST_MISSING_MINIMAX_API_KEY_7F32F3A4"}]}"#,
            ),
            None,
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "upstream_api_key_env_missing"));
    }

    #[test]
    fn status_accepts_braced_env_ref_when_env_is_present() {
        std::env::set_var(
            "CODEX_BOX_TEST_PRESENT_MINIMAX_API_KEY_134B6E2C",
            "test-key",
        );
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax"
model_provider = "openai"
model_catalog_json = "/tmp/models.json"
openai_base_url = "http://127.0.0.1:1455/v1"
"#,
            Some(
                r#"{"models":[{"model":"minimax","provider":"openai","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1","api_key":"${CODEX_BOX_TEST_PRESENT_MINIMAX_API_KEY_134B6E2C}"}]}"#,
            ),
            None,
            true,
            Some(1455),
        )
        .unwrap();
        std::env::remove_var("CODEX_BOX_TEST_PRESENT_MINIMAX_API_KEY_134B6E2C");

        assert!(!status
            .issues
            .iter()
            .any(|issue| issue.code == "upstream_api_key_env_missing"));
    }

    #[test]
    fn status_reports_plaintext_upstream_key_as_ignored() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax"
model_provider = "openai"
model_catalog_json = "/tmp/models.json"
openai_base_url = "http://127.0.0.1:1455/v1"
"#,
            Some(
                r#"{"models":[{"model":"minimax","provider":"openai","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1","api_key":"sk-plaintext"}]}"#,
            ),
            None,
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status.issues.iter().any(|issue| {
            issue.severity == "fail" && issue.code == "upstream_api_key_plaintext_ignored"
        }));
        assert!(!status
            .issues
            .iter()
            .any(|issue| issue.code == "upstream_api_key_missing"));
    }

    #[test]
    fn status_reports_missing_request_entry() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "gpt-5.5"
model_provider = "openai"
model_catalog_json = "/tmp/models.json"
            "#,
            Some(r#"{"models":[{"model":"gpt-5.5","provider":"openai","backend_provider":"openai"}]}"#),
            Some(r#"{"providers":[]}"#),
            None,
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "request_entry_not_configured"));
    }

    #[test]
    fn status_warns_about_legacy_opencodex_inject_map() {
        let status = build_effective_routing_status(
            Path::new("/tmp/config.toml"),
            r#"
model = "minimax-m3"
model_provider = "codex_local_access"
model_catalog_json = "/tmp/models.json"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
"#,
            Some(
                r#"{"models":[{"model":"minimax-m3","provider":"codex_local_access","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1","api_key":"sk-test"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"opencodex","originalBaseUrl":"http://127.0.0.1:8765/v1","wireApi":"responses","models":["gpt-5.5"]}]}"#,
            ),
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status.issues.iter().any(|issue| {
            issue.severity == "warn" && issue.code == "legacy_inject_map_pending_cleanup"
        }));
    }
}
