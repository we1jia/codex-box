use crate::error::{AppError, AppResult};
use crate::proxy::state::{ProxyState, ProxyStatus};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const OPENCODEX_CATALOG_PATH: &str = ".opencodex/custom_model_catalog.json";
const OPENCODEX_PROVIDERS_PATH: &str = ".opencodex/providers.json";

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
}

#[tauri::command]
pub fn effective_routing_status(
    state: tauri::State<Arc<ProxyState>>,
) -> AppResult<EffectiveRoutingStatus> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let catalog_path = home.join(OPENCODEX_CATALOG_PATH);
    let providers_path = home.join(OPENCODEX_PROVIDERS_PATH);

    let config_raw = std::fs::read_to_string(&config_path).map_err(AppError::Io)?;
    let catalog_raw = std::fs::read_to_string(&catalog_path).ok();
    let providers_raw = std::fs::read_to_string(&providers_path).ok();
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
        proxy_running,
        proxy_port,
    )
}

fn build_effective_routing_status(
    config_path: &Path,
    config_raw: &str,
    catalog_raw: Option<&str>,
    providers_raw: Option<&str>,
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
            "未配置 model_catalog_json，Codex App 不会显式加载 ~/.opencodex/custom_model_catalog.json。",
        ));
    }

    let (request_base_url, request_base_url_source) =
        resolve_request_base_url(table, &model_provider, &mut issues);
    if let Some(url) = request_base_url.as_deref() {
        check_local_proxy_url(url, proxy_running, proxy_port, &mut issues);
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
    let upstream_base_url = backend_provider
        .as_deref()
        .and_then(|provider| upstreams.iter().find(|entry| entry.name == provider))
        .and_then(|entry| entry.base_url.clone());

    if let Some(provider) = backend_provider.as_deref() {
        if provider != "openai" && upstream_base_url.is_none() {
            issues.push(issue(
                "fail",
                "backend_provider_missing",
                format!("backend_provider={provider} 没有在 ~/.opencodex/providers.json 中找到对应上游 API。"),
            ));
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
                .and_then(|value| value.as_str())?
                .to_string();
            Some(CatalogRoute {
                model_id,
                provider: obj
                    .get("provider")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                backend_provider: obj
                    .get("backend_provider")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                backend_model: obj
                    .get("backend_model")
                    .or_else(|| obj.get("model"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            })
        })
        .collect()
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
            Some(UpstreamProvider {
                name,
                base_url: obj
                    .get("base_url")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            })
        })
        .collect()
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
    fn status_resolves_backend_provider_to_upstream() {
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
                r#"{"models":[{"slug":"minimax","provider":"opencodex","backend_provider":"minimax","backend_model":"MiniMax-M3"}]}"#,
            ),
            Some(
                r#"{"providers":[{"name":"minimax","base_url":"https://api.minimaxi.com/v1"}]}"#,
            ),
            true,
            Some(1455),
        )
        .unwrap();

        assert!(status.catalog_model_found);
        assert_eq!(status.catalog_provider.as_deref(), Some("opencodex"));
        assert_eq!(status.backend_provider.as_deref(), Some("minimax"));
        assert_eq!(status.backend_model.as_deref(), Some("MiniMax-M3"));
        assert_eq!(
            status.upstream_base_url.as_deref(),
            Some("https://api.minimaxi.com/v1")
        );
        assert!(status.issues.is_empty());
    }
}
