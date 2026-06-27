use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

const DEFAULT_CODEX_CONFIG_PATH: &str = ".codex/config.toml";
const DEFAULT_CODEX_MODELS_CACHE_PATH: &str = ".codex/models_cache.json";
const DEFAULT_CODEX_AUTH_PATH: &str = ".codex/auth.json";
const DEFAULT_CODEX_BOX_CATALOG_PATH: &str = ".codex/codex-box/custom_model_catalog.json";
const DEFAULT_CODEX_BOX_PROVIDERS_PATH: &str = ".codex/codex-box/providers.json";
const CODEX_BOX_MODELS_CACHE_ETAG: &str = "codex-box-model-catalog";
const OFFICIAL_CODEX_BACKEND_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Debug, serde::Deserialize)]
pub struct OpenPathRequest {
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexDesktopIntegrationStatus {
    pub config_path: String,
    pub config_parsed: bool,
    pub config_error: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub model_catalog_json: Option<String>,
    pub custom_model_catalog_path: String,
    pub custom_model_catalog_exists: bool,
    pub custom_catalog_native_openai_model_count: usize,
    pub custom_catalog_byok_model_count: usize,
    pub official_route_configured: bool,
    pub official_route_model_count: usize,
    pub official_route_auth_source: Option<String>,
    pub official_route_base_url: Option<String>,
    pub router_provider_base_url: Option<String>,
    pub router_provider_requires_openai_auth: Option<bool>,
    pub router_provider_supports_websockets: Option<bool>,
    pub router_provider_uses_proxy_managed_bearer: Option<bool>,
    pub router_provider_models_count: Option<usize>,
    pub models_cache_path: String,
    pub models_cache_exists: bool,
    pub models_cache_owned_by_codex_box: bool,
    pub models_cache_model_count: Option<usize>,
    pub models_cache_client_version_present: bool,
    pub auth_path: String,
    pub auth_json_exists: bool,
    pub auth_mode: Option<String>,
    pub chatgpt_auth_likely: bool,
    pub openai_api_key_present_in_auth: bool,
    pub codex_running: bool,
    pub codex_remote_debugging_port: Option<u16>,
    pub codex_processes: Vec<CodexProcessView>,
    pub picker_readiness_status: String,
    pub picker_readiness_summary: String,
    pub picker_readiness_blockers: Vec<String>,
    pub picker_readiness_warnings: Vec<String>,
    pub issues: Vec<CodexDesktopIntegrationIssue>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRuntimeStatus {
    pub codex_home: String,
    pub codex_cli_path: Option<String>,
    pub codex_desktop_app_path: Option<String>,
    pub codex_desktop_version: Option<String>,
    pub desktop_installed: bool,
    pub cli_available: bool,
    pub config_readable: bool,
    pub auth_state_detected: bool,
    pub opencodex_dir: String,
    pub opencodex_dir_exists: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexProcessView {
    pub pid: Option<u32>,
    pub command: String,
    pub remote_debugging_port: Option<u16>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexDesktopIntegrationIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[tauri::command]
pub fn codex_desktop_integration_status() -> AppResult<CodexDesktopIntegrationStatus> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    codex_desktop_integration_status_in_home(&home)
}

#[tauri::command]
pub fn codex_runtime_status() -> AppResult<CodexRuntimeStatus> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let codex_home = std::env::var("CODEX_HOME")
        .ok()
        .and_then(|value| expand_home(&value).ok())
        .unwrap_or_else(|| home.join(".codex"));
    Ok(build_codex_runtime_status(
        &home,
        codex_home,
        find_codex_cli_path(),
        find_codex_desktop_app(),
    ))
}

#[tauri::command]
pub fn reveal_path(request: OpenPathRequest) -> AppResult<()> {
    let path = expand_home(&request.path)?;
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Command(format!("Invalid path: {}", request.path)))?
    };

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg("-R").arg(&target);
        run_open(&mut command, "open -R")
    }

    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer");
        command.arg(format!("/select,{}", target.display()));
        run_open(&mut command, "explorer")
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let folder = if target.is_dir() {
            target
        } else {
            target
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| AppError::Command(format!("Invalid path: {}", request.path)))?
        };
        let mut command = Command::new("xdg-open");
        command.arg(folder);
        run_open(&mut command, "xdg-open")
    }
}

#[tauri::command]
pub fn open_path(request: OpenPathRequest) -> AppResult<()> {
    let path = expand_home(&request.path)?;
    if !path.exists() {
        return Err(AppError::Command(format!(
            "Path does not exist: {}",
            path.display()
        )));
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(&path);
        run_open(&mut command, "open")
    }

    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "start"]).arg(&path);
        run_open(&mut command, "start")
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(&path);
        run_open(&mut command, "xdg-open")
    }
}

fn expand_home(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Command("Path is empty".to_string()));
    }

    if trimmed == "~" {
        return dirs::home_dir()
            .ok_or_else(|| AppError::Command("Home directory not found".to_string()));
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Command("Home directory not found".to_string()))?;
        return Ok(home.join(rest));
    }

    Ok(PathBuf::from(trimmed))
}

fn run_open(command: &mut Command, label: &str) -> AppResult<()> {
    let output = command.output().map_err(AppError::Io)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(AppError::Command(format!(
        "{label} failed{}",
        if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        }
    )))
}

fn build_codex_runtime_status(
    home: &Path,
    codex_home: PathBuf,
    codex_cli_path: Option<PathBuf>,
    codex_desktop: Option<(PathBuf, Option<String>)>,
) -> CodexRuntimeStatus {
    let config_readable = codex_home.join("config.toml").is_file();
    let auth_raw = std::fs::read_to_string(codex_home.join("auth.json")).ok();
    let (auth_mode, chatgpt_auth_likely, openai_api_key_present_in_auth) =
        inspect_auth_json(auth_raw.as_deref());
    let auth_state_detected =
        auth_mode.is_some() || chatgpt_auth_likely || openai_api_key_present_in_auth;
    let opencodex_dir = home.join(".opencodex");
    let (codex_desktop_app_path, codex_desktop_version) = codex_desktop
        .map(|(path, version)| (Some(path.display().to_string()), version))
        .unwrap_or((None, None));
    let desktop_installed = codex_desktop_app_path.is_some();
    let cli_available = codex_cli_path.is_some();

    CodexRuntimeStatus {
        codex_home: codex_home.display().to_string(),
        codex_cli_path: codex_cli_path.map(|path| path.display().to_string()),
        codex_desktop_app_path,
        codex_desktop_version,
        desktop_installed,
        cli_available,
        config_readable,
        auth_state_detected,
        opencodex_dir: opencodex_dir.display().to_string(),
        opencodex_dir_exists: opencodex_dir.is_dir(),
    }
}

fn find_codex_cli_path() -> Option<PathBuf> {
    let output = if cfg!(target_os = "windows") {
        Command::new("where").arg("codex").output()
    } else {
        Command::new("which").arg("codex").output()
    }
    .ok()?;

    if !output.status.success() {
        return common_codex_cli_paths()
            .into_iter()
            .find(|path| path.is_file());
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(|| {
            common_codex_cli_paths()
                .into_iter()
                .find(|path| path.is_file())
        })
}

fn common_codex_cli_paths() -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        Vec::new()
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
            PathBuf::from("/usr/bin/codex"),
        ]
    }
}

fn find_codex_desktop_app() -> Option<(PathBuf, Option<String>)> {
    let candidates = codex_desktop_app_candidates();
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .map(|path| {
            let version = read_macos_app_version(&path);
            (path, version)
        })
}

fn codex_desktop_app_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Codex.app")];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/Codex.app"));
    }
    candidates
}

fn read_macos_app_version(app_path: &Path) -> Option<String> {
    let plist = std::fs::read_to_string(app_path.join("Contents/Info.plist")).ok()?;
    plist_string_value(&plist, "CFBundleShortVersionString")
        .or_else(|| plist_string_value(&plist, "CFBundleVersion"))
}

fn plist_string_value(plist: &str, key: &str) -> Option<String> {
    let mut lines = plist.lines();
    while let Some(line) = lines.next() {
        if !line.contains(&format!("<key>{key}</key>")) {
            continue;
        }
        for next in lines.by_ref() {
            let trimmed = next.trim();
            if let Some(value) = trimmed
                .strip_prefix("<string>")
                .and_then(|value| value.strip_suffix("</string>"))
            {
                return Some(value.trim().to_string());
            }
            if trimmed.starts_with("<key>") {
                break;
            }
        }
    }
    None
}

fn codex_desktop_integration_status_in_home(
    home: &Path,
) -> AppResult<CodexDesktopIntegrationStatus> {
    let config_path = home.join(DEFAULT_CODEX_CONFIG_PATH);
    let models_cache_path = home.join(DEFAULT_CODEX_MODELS_CACHE_PATH);
    let auth_path = home.join(DEFAULT_CODEX_AUTH_PATH);

    let config_raw = std::fs::read_to_string(&config_path).unwrap_or_default();
    let config_value = if config_raw.trim().is_empty() {
        Ok(toml::Value::Table(Default::default()))
    } else {
        toml::from_str::<toml::Value>(&config_raw)
    };

    let mut issues = Vec::new();
    let mut model = None;
    let mut model_provider = None;
    let mut model_catalog_json = None;
    let mut router_provider_base_url = None;
    let mut router_provider_requires_openai_auth = None;
    let mut router_provider_supports_websockets = None;
    let mut router_provider_uses_proxy_managed_bearer = None;
    let mut router_provider_models_count = None;
    let mut config_error = None;

    match config_value {
        Ok(value) => {
            if let Some(table) = value.as_table() {
                model = string_field(table, "model");
                model_provider =
                    string_field(table, "model_provider").or(Some("openai".to_string()));
                model_catalog_json = string_field(table, "model_catalog_json");

                if let Some(provider_id) = model_provider.as_deref() {
                    if provider_id == "openai" {
                        router_provider_base_url = string_field(table, "openai_base_url");
                    } else if let Some(provider_table) = table
                        .get("model_providers")
                        .and_then(|value| value.as_table())
                        .and_then(|providers| providers.get(provider_id))
                        .and_then(|value| value.as_table())
                    {
                        router_provider_base_url = string_field(provider_table, "base_url");
                        router_provider_requires_openai_auth =
                            bool_field(provider_table, "requires_openai_auth");
                        router_provider_supports_websockets =
                            bool_field(provider_table, "supports_websockets");
                        router_provider_uses_proxy_managed_bearer =
                            string_field(provider_table, "experimental_bearer_token")
                                .map(|value| value.eq_ignore_ascii_case("PROXY_MANAGED"));
                        router_provider_models_count = provider_table
                            .get("models")
                            .and_then(|value| value.as_array())
                            .map(|models| models.len());

                        if router_provider_requires_openai_auth == Some(false) {
                            issues.push(integration_issue(
                                "warn",
                                "router_requires_openai_auth_false",
                                "当前本地 router provider 设置了 requires_openai_auth=false，Codex Desktop 可能隐藏 ChatGPT 登录态、额度信息和部分官方 GPT 能力。",
                            ));
                        }
                        if router_provider_base_url
                            .as_deref()
                            .is_some_and(points_to_loopback)
                            && router_provider_uses_proxy_managed_bearer != Some(true)
                        {
                            issues.push(integration_issue(
                                "warn",
                                "router_proxy_managed_bearer_missing",
                                "当前本地 router provider 没有 experimental_bearer_token=\"PROXY_MANAGED\"，Codex Desktop 到本地代理的官方认证上下文可能不完整。",
                            ));
                        }
                    }
                }

                if model_catalog_json.is_none() {
                    issues.push(integration_issue(
                        "warn",
                        "model_catalog_json_missing",
                        "当前 config.toml 没有 model_catalog_json，Codex Desktop 下拉框不会显式读取 Codex Box 合并模型目录。",
                    ));
                }
            }
        }
        Err(error) => {
            config_error = Some(error.to_string());
            issues.push(integration_issue(
                "fail",
                "config_toml_parse_failed",
                format!("config.toml 无法解析：{error}"),
            ));
        }
    }

    let custom_model_catalog_path = model_catalog_json
        .as_deref()
        .map(|path| resolve_model_catalog_path(home, &config_path, path))
        .unwrap_or_else(|| home.join(DEFAULT_CODEX_BOX_CATALOG_PATH));
    let custom_catalog_raw = std::fs::read_to_string(&custom_model_catalog_path).ok();
    let (custom_catalog_native_openai_model_count, custom_catalog_byok_model_count) =
        inspect_custom_model_catalog(custom_catalog_raw.as_deref());
    let custom_catalog_visible_model_count =
        custom_catalog_native_openai_model_count + custom_catalog_byok_model_count;
    let custom_model_catalog_exists = custom_model_catalog_path.exists();
    if !custom_model_catalog_exists {
        issues.push(integration_issue(
            "warn",
            "custom_model_catalog_missing",
            format!(
                "当前 model_catalog_json 指向的模型目录不存在：{}。Codex Desktop 下拉框无法从该文件读取 GPT/BYOK 合并模型。",
                custom_model_catalog_path.display()
            ),
        ));
    } else if custom_catalog_visible_model_count == 0 {
        issues.push(integration_issue(
            "warn",
            "custom_model_catalog_empty",
            "Codex Box 模型目录存在，但没有可见的官方 GPT 或 BYOK 模型条目。",
        ));
    }
    let providers_path = home.join(DEFAULT_CODEX_BOX_PROVIDERS_PATH);
    let providers_raw = std::fs::read_to_string(&providers_path).ok();
    let official_route = inspect_official_route(providers_raw.as_deref());
    if custom_catalog_native_openai_model_count > 0 {
        if !official_route.configured {
            issues.push(integration_issue(
                "warn",
                "official_managed_route_missing",
                "模型目录包含官方 GPT 条目，但 providers.json 没有 openai-official managed route。官方 GPT 请求可能落到 fallback，无法达到 MultiRouter 等价链路。",
            ));
        } else {
            if official_route.auth_source.as_deref() != Some("managed_codex_oauth") {
                issues.push(integration_issue(
                    "warn",
                    "official_managed_route_auth_unmanaged",
                    "openai-official route 没有使用 auth.source=\"managed_codex_oauth\"，官方 GPT 请求不会按托管官方路线处理。",
                ));
            }
            if official_route.base_url.as_deref() != Some(OFFICIAL_CODEX_BACKEND_URL) {
                issues.push(integration_issue(
                    "warn",
                    "official_managed_route_base_url_unexpected",
                    "openai-official route 没有指向 ChatGPT Codex backend，官方 GPT 请求目标可能不正确。",
                ));
            }
        }
    }

    let models_cache_raw = std::fs::read_to_string(&models_cache_path).ok();
    let (
        models_cache_owned_by_codex_box,
        models_cache_model_count,
        models_cache_client_version_present,
    ) = inspect_models_cache(models_cache_raw.as_deref());
    let models_cache_exists = models_cache_path.exists();
    if !models_cache_exists {
        issues.push(integration_issue(
            "warn",
            "models_cache_missing",
            "未找到 ~/.codex/models_cache.json。Codex Desktop 可能需要重启或重新拉取模型缓存后才会刷新下拉框。",
        ));
    } else {
        if custom_catalog_visible_model_count > 0 && !models_cache_owned_by_codex_box {
            issues.push(integration_issue(
                "warn",
                "models_cache_not_codex_box_owned",
                "当前 ~/.codex/models_cache.json 不是 Codex Box 生成的合并模型缓存。Codex Desktop 可能仍在使用旧缓存，导致下拉框缺少 GPT/BYOK 模型。",
            ));
        }
        if let Some(cache_count) = models_cache_model_count {
            if custom_catalog_visible_model_count > 0
                && cache_count < custom_catalog_visible_model_count
            {
                issues.push(integration_issue(
                    "warn",
                    "models_cache_model_count_behind_catalog",
                    format!(
                        "models_cache.json 只有 {cache_count} 个模型，但 Codex Box catalog 有 {custom_catalog_visible_model_count} 个可见模型；下拉框可能没有刷新到完整候选。",
                    ),
                ));
            }
        }
        if custom_catalog_visible_model_count > 0 && !models_cache_client_version_present {
            issues.push(integration_issue(
                "warn",
                "models_cache_client_version_missing",
                "models_cache.json 缺少 client_version。Codex Desktop 可能拒绝或重建该缓存，建议通过 MultiRouter 同步重新生成。",
            ));
        }
    }

    let auth_raw = std::fs::read_to_string(&auth_path).ok();
    let (auth_mode, chatgpt_auth_likely, openai_api_key_present_in_auth) =
        inspect_auth_json(auth_raw.as_deref());
    if !auth_path.exists() {
        issues.push(integration_issue(
            "warn",
            "auth_json_missing",
            "未找到 ~/.codex/auth.json。官方订阅 GPT 模型需要 Codex Desktop 自己持有有效登录态。",
        ));
    }

    let codex_processes = detect_codex_processes();
    let codex_running = !codex_processes.is_empty();
    let codex_remote_debugging_port = codex_processes
        .iter()
        .find_map(|process| process.remote_debugging_port);
    let visible_model_count = router_provider_models_count.or(models_cache_model_count);
    if codex_running
        && codex_remote_debugging_port.is_none()
        && visible_model_count.unwrap_or(0) > 3
    {
        issues.push(integration_issue(
            "warn",
            "codex_renderer_picker_filter_risk",
            "Codex Desktop 已普通启动且没有 remote debugging 端口；即使 config/catalog/cache 正确，renderer 仍可能被远端模型白名单过滤，导致下拉框只显示少量官方模型。",
        ));
    }

    if (models_cache_owned_by_codex_box || custom_catalog_native_openai_model_count > 0)
        && !chatgpt_auth_likely
    {
        issues.push(integration_issue(
            "warn",
            "native_openai_auth_unverified",
            "模型目录包含官方 GPT 条目，但未确认 ChatGPT 登录态。官方订阅 GPT 请求需要 Codex Desktop 携带真实官方认证上下文，Codex Box 不会读取或托管官方 token。",
        ));
    }

    let picker_readiness = build_picker_readiness(&issues);

    Ok(CodexDesktopIntegrationStatus {
        config_path: config_path.display().to_string(),
        config_parsed: config_error.is_none(),
        config_error,
        model,
        model_provider,
        model_catalog_json,
        custom_model_catalog_path: custom_model_catalog_path.display().to_string(),
        custom_model_catalog_exists,
        custom_catalog_native_openai_model_count,
        custom_catalog_byok_model_count,
        official_route_configured: official_route.configured,
        official_route_model_count: official_route.model_count,
        official_route_auth_source: official_route.auth_source,
        official_route_base_url: official_route.base_url,
        router_provider_base_url,
        router_provider_requires_openai_auth,
        router_provider_supports_websockets,
        router_provider_uses_proxy_managed_bearer,
        router_provider_models_count,
        models_cache_path: models_cache_path.display().to_string(),
        models_cache_exists,
        models_cache_owned_by_codex_box,
        models_cache_model_count,
        models_cache_client_version_present,
        auth_path: auth_path.display().to_string(),
        auth_json_exists: auth_path.exists(),
        auth_mode,
        chatgpt_auth_likely,
        openai_api_key_present_in_auth,
        codex_running,
        codex_remote_debugging_port,
        codex_processes,
        picker_readiness_status: picker_readiness.status,
        picker_readiness_summary: picker_readiness.summary,
        picker_readiness_blockers: picker_readiness.blockers,
        picker_readiness_warnings: picker_readiness.warnings,
        issues,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct PickerReadiness {
    status: String,
    summary: String,
    blockers: Vec<String>,
    warnings: Vec<String>,
}

fn build_picker_readiness(issues: &[CodexDesktopIntegrationIssue]) -> PickerReadiness {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    for issue in issues {
        if PICKER_BLOCKER_CODES.contains(&issue.code.as_str()) || issue.severity == "fail" {
            blockers.push(issue.code.clone());
        } else if PICKER_WARNING_CODES.contains(&issue.code.as_str()) || issue.severity == "warn" {
            warnings.push(issue.code.clone());
        }
    }
    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();

    let (status, summary) = if !blockers.is_empty() {
        (
            "blocked",
            format!("Picker 尚未就绪：{} 个阻断项需要先处理。", blockers.len()),
        )
    } else if !warnings.is_empty() {
        (
            "needs_attention",
            format!(
                "Picker 基础链路可继续，但有 {} 个风险项可能导致 Codex App 下拉框不完整或切换失败。",
                warnings.len()
            ),
        )
    } else {
        (
            "ready",
            "Picker 前置条件已就绪，可以同步 MultiRouter 后执行下拉框解锁。".to_string(),
        )
    };

    PickerReadiness {
        status: status.to_string(),
        summary,
        blockers,
        warnings,
    }
}

const PICKER_BLOCKER_CODES: &[&str] = &[
    "config_toml_parse_failed",
    "model_catalog_json_missing",
    "custom_model_catalog_missing",
    "custom_model_catalog_empty",
];

const PICKER_WARNING_CODES: &[&str] = &[
    "auth_json_missing",
    "codex_renderer_picker_filter_risk",
    "models_cache_client_version_missing",
    "models_cache_missing",
    "models_cache_model_count_behind_catalog",
    "models_cache_not_codex_box_owned",
    "native_openai_auth_unverified",
    "official_managed_route_auth_unmanaged",
    "official_managed_route_base_url_unexpected",
    "official_managed_route_missing",
    "router_proxy_managed_bearer_missing",
    "router_requires_openai_auth_false",
];

fn string_field(table: &toml::map::Map<String, toml::Value>, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn bool_field(table: &toml::map::Map<String, toml::Value>, key: &str) -> Option<bool> {
    table.get(key).and_then(|value| value.as_bool())
}

fn points_to_loopback(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    normalized.contains("127.0.0.1") || normalized.contains("localhost")
}

fn resolve_model_catalog_path(home: &Path, config_path: &Path, value: &str) -> PathBuf {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        return path;
    }
    config_path.parent().unwrap_or(home).join(path)
}

fn inspect_custom_model_catalog(raw: Option<&str>) -> (usize, usize) {
    let Some(raw) = raw else {
        return (0, 0);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (0, 0);
    };
    let array = value
        .as_object()
        .and_then(|obj| obj.get("models"))
        .and_then(|value| value.as_array())
        .or_else(|| value.as_array());
    let Some(array) = array else {
        return (0, 0);
    };

    let mut native_openai = 0;
    let mut byok = 0;
    for entry in array {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        if !catalog_entry_visible(obj) {
            continue;
        }
        let provider = obj
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let backend_provider = obj
            .get("backend_provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model_id_present = obj
            .get("model_id")
            .or_else(|| obj.get("model"))
            .or_else(|| obj.get("slug"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !model_id_present {
            continue;
        }

        let is_native_openai = provider.eq_ignore_ascii_case("openai")
            && backend_provider
                .map(|value| value.eq_ignore_ascii_case("openai"))
                .unwrap_or(true);
        if is_native_openai {
            native_openai += 1;
        } else if !provider.is_empty() || backend_provider.is_some() {
            byok += 1;
        }
    }
    (native_openai, byok)
}

#[derive(Debug, Clone, Default)]
struct OfficialRouteInspection {
    configured: bool,
    model_count: usize,
    auth_source: Option<String>,
    base_url: Option<String>,
}

fn inspect_official_route(raw: Option<&str>) -> OfficialRouteInspection {
    let Some(raw) = raw else {
        return OfficialRouteInspection::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return OfficialRouteInspection::default();
    };

    let providers = value
        .as_object()
        .and_then(|obj| obj.get("providers"))
        .and_then(|value| value.as_array())
        .or_else(|| value.as_array());
    let Some(providers) = providers else {
        return OfficialRouteInspection::default();
    };

    for provider in providers {
        let Some(provider) = provider.as_object() else {
            continue;
        };
        let routing = provider
            .get("codexRouting")
            .or_else(|| provider.get("codex_routing"))
            .and_then(|value| value.as_object());
        let Some(routing) = routing else {
            continue;
        };
        if routing.get("enabled").and_then(|value| value.as_bool()) == Some(false) {
            continue;
        }
        let Some(routes) = routing.get("routes").and_then(|value| value.as_array()) else {
            continue;
        };

        for route in routes {
            if route.get("enabled").and_then(|value| value.as_bool()) == Some(false) {
                continue;
            }
            if !route_looks_official(route) {
                continue;
            }

            let model_count = route
                .get("match")
                .or_else(|| route.get("matchRule"))
                .or_else(|| route.get("match_rule"))
                .and_then(|value| value.as_object())
                .and_then(|match_rule| match_rule.get("models"))
                .and_then(|value| value.as_array())
                .map(|models| models.len())
                .unwrap_or(0);

            return OfficialRouteInspection {
                configured: true,
                model_count,
                auth_source: official_route_auth_source(route),
                base_url: official_route_base_url(route),
            };
        }
    }

    OfficialRouteInspection::default()
}

fn route_looks_official(route: &serde_json::Value) -> bool {
    let id = route
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    if id == "openai-official" {
        return true;
    }

    official_route_auth_source(route)
        .as_deref()
        .is_some_and(|source| source == "managed_codex_oauth")
        || official_route_base_url(route)
            .as_deref()
            .is_some_and(|base_url| {
                same_url_without_trailing_slash(base_url, OFFICIAL_CODEX_BACKEND_URL)
            })
}

fn official_route_auth_source(route: &serde_json::Value) -> Option<String> {
    let upstream = route.get("upstream").and_then(|value| value.as_object())?;
    upstream
        .get("auth")
        .and_then(|value| value.as_object())
        .and_then(|auth| json_string_field(auth, &["source", "authProvider", "auth_provider"]))
        .or_else(|| {
            json_string_field(
                upstream,
                &["authSource", "auth_source", "authProvider", "auth_provider"],
            )
        })
}

fn official_route_base_url(route: &serde_json::Value) -> Option<String> {
    route
        .get("upstream")
        .and_then(|value| value.as_object())
        .and_then(|upstream| json_string_field(upstream, &["baseUrl", "base_url"]))
}

fn json_string_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        obj.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn same_url_without_trailing_slash(left: &str, right: &str) -> bool {
    left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/')
}

fn catalog_entry_visible(obj: &serde_json::Map<String, serde_json::Value>) -> bool {
    if obj.get("visible").and_then(|value| value.as_bool()) == Some(false) {
        return false;
    }
    if obj
        .get("visibility")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("hidden"))
    {
        return false;
    }
    true
}

fn inspect_models_cache(raw: Option<&str>) -> (bool, Option<usize>, bool) {
    let Some(raw) = raw else {
        return (false, None, false);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (false, None, false);
    };
    let owned = value
        .get("etag")
        .and_then(|value| value.as_str())
        .map(|etag| etag == CODEX_BOX_MODELS_CACHE_ETAG)
        .unwrap_or(false);
    let count = value
        .get("models")
        .and_then(|value| value.as_array())
        .map(|models| models.len());
    let has_client_version = value
        .get("client_version")
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    (owned, count, has_client_version)
}

fn inspect_auth_json(raw: Option<&str>) -> (Option<String>, bool, bool) {
    let Some(raw) = raw else {
        return (None, false, false);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (None, false, false);
    };
    let auth_mode = value
        .get("auth_mode")
        .or_else(|| value.get("authMode"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let chatgpt_auth_likely = auth_mode
        .as_deref()
        .map(|mode| mode.eq_ignore_ascii_case("chatgpt"))
        .unwrap_or(false)
        || value.get("tokens").is_some()
        || value.get("chatgpt_account_id").is_some()
        || value.get("chatgptAccountId").is_some();
    let openai_api_key_present = value.get("OPENAI_API_KEY").is_some()
        || value.get("openai_api_key").is_some()
        || value.get("api_key").is_some();
    (auth_mode, chatgpt_auth_likely, openai_api_key_present)
}

fn detect_codex_processes() -> Vec<CodexProcessView> {
    let output = if cfg!(target_os = "windows") {
        Command::new("wmic")
            .args(["process", "get", "ProcessId,CommandLine", "/FORMAT:LIST"])
            .output()
    } else {
        Command::new("ps")
            .args(["-axo", "pid=,comm=,args="])
            .output()
    };

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter(|line| is_codex_process_line(line))
        .take(12)
        .map(|line| CodexProcessView {
            pid: parse_pid(line),
            command: redact_process_line(line),
            remote_debugging_port: parse_remote_debugging_port(line),
        })
        .collect()
}

fn is_codex_process_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("codex") {
        return false;
    }
    if lower.contains("codex box")
        || lower.contains("codex-box")
        || lower.contains("/codex box/")
        || lower.contains("\\codex box\\")
    {
        return false;
    }
    lower.contains("openai.codex")
        || lower.contains("codex.app")
        || lower.contains("codex.exe")
        || lower.contains("codex desktop")
        || lower.contains("app-server")
        || lower.contains("--remote-debugging-port=")
}

fn parse_pid(line: &str) -> Option<u32> {
    line.split_whitespace().next()?.parse::<u32>().ok()
}

fn parse_remote_debugging_port(line: &str) -> Option<u16> {
    line.split_whitespace().find_map(|part| {
        part.strip_prefix("--remote-debugging-port=")
            .and_then(|port| port.parse::<u16>().ok())
    })
}

fn redact_process_line(line: &str) -> String {
    line.split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.contains("token=")
                || lower.contains("api_key=")
                || lower.contains("apikey=")
                || lower.contains("authorization=")
            {
                "<redacted>"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn integration_issue(
    severity: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> CodexDesktopIntegrationIssue {
    CodexDesktopIntegrationIssue {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_status_reads_codex_home_and_auth_without_starting_codex() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_home).unwrap();
        std::fs::write(codex_home.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
        std::fs::write(
            codex_home.join("auth.json"),
            r#"{ "auth_mode": "chatgpt" }"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".opencodex")).unwrap();
        let cli = dir.path().join("bin/codex");
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, "").unwrap();

        let status = build_codex_runtime_status(dir.path(), codex_home, Some(cli.clone()), None);

        assert!(status.config_readable);
        assert!(status.auth_state_detected);
        assert!(status.opencodex_dir_exists);
        assert!(status.cli_available);
        assert_eq!(
            status.codex_cli_path.as_deref(),
            Some(cli.to_str().unwrap())
        );
        assert!(!status.desktop_installed);
    }

    #[test]
    fn runtime_status_extracts_macos_desktop_version() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Codex.app");
        let contents = app.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleShortVersionString</key>
  <string>1.2.3</string>
</dict>
</plist>
"#,
        )
        .unwrap();

        let status = build_codex_runtime_status(
            dir.path(),
            dir.path().join(".codex"),
            None,
            Some((app, read_macos_app_version(&dir.path().join("Codex.app")))),
        );

        assert!(status.desktop_installed);
        assert_eq!(status.codex_desktop_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn detects_renderer_filter_risk_when_codex_runs_without_cdp() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            r#"
model = "minimax"
model_provider = "codex_local_access"
model_catalog_json = "/tmp/catalog.json"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
models = ["gpt-5.5", "gpt-5.4", "minimax", "deepseek"]
"#,
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();
        assert_eq!(status.router_provider_models_count, Some(4));
        assert!(!status.config_parsed || status.config_error.is_none());
    }

    #[test]
    fn reports_custom_catalog_composition_and_native_auth_requirement() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        let box_dir = codex_dir.join("codex-box");
        std::fs::create_dir_all(&box_dir).unwrap();
        let catalog_path = box_dir.join("custom_model_catalog.json");
        std::fs::write(
            &catalog_path,
            r#"{
  "models": [
    { "model": "gpt-5.5", "provider": "openai", "visible": true },
    { "model": "minimax-m3", "provider": "codex_local_access", "backend_provider": "minimax", "visible": true },
    { "model": "hidden-model", "provider": "minimax", "visible": false }
  ]
}"#,
        )
        .unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            format!(
                r#"
model = "minimax-m3"
model_provider = "codex_local_access"
model_catalog_json = "{}"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
supports_websockets = false
experimental_bearer_token = "PROXY_MANAGED"
"#,
                catalog_path.display()
            ),
        )
        .unwrap();
        std::fs::write(
            box_dir.join("providers.json"),
            r#"{
  "providers": [
    {
      "id": "codex_local_access",
      "codexRouting": {
        "enabled": true,
        "routes": [
          {
            "id": "openai-official",
            "enabled": true,
            "match": { "models": ["gpt-5.5"], "prefixes": ["gpt-"] },
            "upstream": {
              "baseUrl": "https://chatgpt.com/backend-api/codex",
              "apiFormat": "openai_responses",
              "auth": { "source": "managed_codex_oauth" }
            }
          }
        ]
      }
    }
  ]
}"#,
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();

        assert!(status.custom_model_catalog_exists);
        assert_eq!(status.custom_catalog_native_openai_model_count, 1);
        assert_eq!(status.custom_catalog_byok_model_count, 1);
        assert!(status.official_route_configured);
        assert_eq!(status.official_route_model_count, 1);
        assert_eq!(
            status.official_route_auth_source.as_deref(),
            Some("managed_codex_oauth")
        );
        assert_eq!(
            status.official_route_base_url.as_deref(),
            Some(OFFICIAL_CODEX_BACKEND_URL)
        );
        assert_eq!(status.router_provider_uses_proxy_managed_bearer, Some(true));
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "native_openai_auth_unverified"));
        assert!(!status
            .issues
            .iter()
            .any(|issue| issue.code == "official_managed_route_missing"));
        assert!(!status
            .issues
            .iter()
            .any(|issue| issue.code == "router_proxy_managed_bearer_missing"));
    }

    #[test]
    fn warns_when_native_catalog_missing_official_managed_route() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        let box_dir = codex_dir.join("codex-box");
        std::fs::create_dir_all(&box_dir).unwrap();
        let catalog_path = box_dir.join("custom_model_catalog.json");
        std::fs::write(
            &catalog_path,
            r#"{
  "models": [
    { "model": "gpt-5.5", "provider": "openai", "visible": true }
  ]
}"#,
        )
        .unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            format!(
                r#"
model = "gpt-5.5"
model_provider = "codex_local_access"
model_catalog_json = "{}"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
supports_websockets = false
experimental_bearer_token = "PROXY_MANAGED"
"#,
                catalog_path.display()
            ),
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();

        assert!(!status.official_route_configured);
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "official_managed_route_missing"));
    }

    #[test]
    fn warns_when_config_points_to_missing_custom_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let catalog_path = codex_dir.join("codex-box/missing-catalog.json");
        std::fs::write(
            codex_dir.join("config.toml"),
            format!(
                r#"
model = "gpt-5.5"
model_provider = "codex_model_router_v2"
model_catalog_json = "{}"

[model_providers.codex_model_router_v2]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
supports_websockets = false
experimental_bearer_token = "PROXY_MANAGED"
models = ["gpt-5.5"]
"#,
                catalog_path.display()
            ),
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();

        assert!(!status.custom_model_catalog_exists);
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "custom_model_catalog_missing"));
    }

    #[test]
    fn warns_when_models_cache_is_stale_or_not_codex_box_owned() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        let box_dir = codex_dir.join("codex-box");
        std::fs::create_dir_all(&box_dir).unwrap();
        let catalog_path = box_dir.join("custom_model_catalog.json");
        std::fs::write(
            &catalog_path,
            r#"{
  "models": [
    { "model": "gpt-5.5", "provider": "openai", "visible": true },
    { "model": "gpt-5.4", "provider": "openai", "visible": true },
    { "model": "minimax-m3", "provider": "codex_model_router_v2", "backend_provider": "minimax", "visible": true }
  ]
}"#,
        )
        .unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            format!(
                r#"
model = "minimax-m3"
model_provider = "codex_model_router_v2"
model_catalog_json = "{}"

[model_providers.codex_model_router_v2]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
supports_websockets = false
experimental_bearer_token = "PROXY_MANAGED"
models = ["gpt-5.5", "gpt-5.4", "minimax-m3"]
"#,
                catalog_path.display()
            ),
        )
        .unwrap();
        std::fs::write(
            codex_dir.join("models_cache.json"),
            r#"{ "client_version": "", "etag": "native-cache", "models": [{ "slug": "gpt-5.5" }] }"#,
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();

        assert_eq!(status.models_cache_model_count, Some(1));
        assert!(!status.models_cache_owned_by_codex_box);
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "models_cache_not_codex_box_owned"));
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "models_cache_model_count_behind_catalog"));
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "models_cache_client_version_missing"));
        assert_eq!(status.picker_readiness_status, "needs_attention");
        assert!(status
            .picker_readiness_warnings
            .contains(&"models_cache_not_codex_box_owned".to_string()));
    }

    #[test]
    fn warns_when_local_router_missing_proxy_managed_bearer() {
        let dir = tempfile::tempdir().unwrap();
        let codex_dir = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            r#"
model = "minimax-m3"
model_provider = "codex_local_access"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
requires_openai_auth = true
"#,
        )
        .unwrap();

        let status = codex_desktop_integration_status_in_home(dir.path()).unwrap();

        assert_eq!(status.router_provider_uses_proxy_managed_bearer, None);
        assert!(status
            .issues
            .iter()
            .any(|issue| issue.code == "router_proxy_managed_bearer_missing"));
    }

    #[test]
    fn picker_readiness_distinguishes_blocked_warn_and_ready() {
        let blocked = build_picker_readiness(&[
            integration_issue("warn", "custom_model_catalog_missing", "missing"),
            integration_issue("warn", "models_cache_missing", "cache"),
        ]);
        assert_eq!(blocked.status, "blocked");
        assert!(blocked
            .blockers
            .contains(&"custom_model_catalog_missing".to_string()));
        assert!(blocked
            .warnings
            .contains(&"models_cache_missing".to_string()));

        let warning = build_picker_readiness(&[integration_issue(
            "warn",
            "codex_renderer_picker_filter_risk",
            "renderer",
        )]);
        assert_eq!(warning.status, "needs_attention");
        assert!(warning.blockers.is_empty());
        assert_eq!(
            warning.warnings,
            vec!["codex_renderer_picker_filter_risk".to_string()]
        );

        let ready = build_picker_readiness(&[]);
        assert_eq!(ready.status, "ready");
        assert!(ready.blockers.is_empty());
        assert!(ready.warnings.is_empty());
    }

    #[test]
    fn parses_remote_debugging_port_from_process_line() {
        assert_eq!(
            parse_remote_debugging_port(
                "123 /Applications/Codex.app/Contents/MacOS/Codex --remote-debugging-port=9229"
            ),
            Some(9229)
        );
    }

    #[test]
    fn excludes_codex_box_process_lines() {
        assert!(!is_codex_process_line(
            "123 /Users/me/Desktop/AI/Codex Box/src-tauri/target/debug/codex-box"
        ));
        assert!(is_codex_process_line(
            "456 /Applications/Codex.app/Contents/MacOS/Codex --remote-debugging-port=9229"
        ));
    }
}
