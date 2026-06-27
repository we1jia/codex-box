use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use futures::{Sink, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AppError, AppResult};

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const DEFAULT_CATALOG_PATH: &str = ".codex/codex-box/custom_model_catalog.json";
const DEFAULT_DEBUG_PORT: u16 = 9229;
const CDP_HTTP_TIMEOUT: Duration = Duration::from_secs(2);
const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(4);
const PICKER_PATCH_KEY: &str = "__codexBoxModelPickerUnlockV1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexPickerUnlockResult {
    pub attempted_ports: Vec<u16>,
    pub debug_port: Option<u16>,
    pub target_count: usize,
    pub injected_target_count: usize,
    pub renderer_reports: Vec<PickerRendererReport>,
    pub model_count: usize,
    pub model_names: Vec<String>,
    pub injected: bool,
    pub launched: bool,
    pub codex_executable: Option<String>,
    pub status: String,
    pub message: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PickerRendererReport {
    pub port: u16,
    pub target_id: String,
    pub status: String,
    pub patch_key: Option<String>,
    pub model_count: Option<usize>,
    pub available_models: Vec<String>,
    pub error_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PickerCatalogProjection {
    default_model: Option<String>,
    model_names: Vec<String>,
    models: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct CdpTarget {
    id: String,
    #[serde(rename = "type")]
    target_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

#[tauri::command]
pub async fn codex_desktop_picker_unlock() -> AppResult<CodexPickerUnlockResult> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let catalog = load_picker_catalog_projection(&home)?;
    if catalog.model_names.is_empty() {
        return Err(AppError::Command(
            "模型目录为空；请先在模型配置页添加模型并同步 MultiRouter。".to_string(),
        ));
    }

    let desktop_status = crate::commands::system::codex_desktop_integration_status()?;
    let attempt = inject_picker_catalog(&catalog, desktop_status.codex_remote_debugging_port).await;

    if attempt.injected_target_count > 0 {
        return Ok(picker_result(
            &catalog,
            attempt,
            false,
            None,
            "injected",
            "已向 Codex Desktop renderer 注入模型下拉解锁补丁。",
        ));
    }

    let (status, message) = if desktop_status.codex_running
        && desktop_status.codex_remote_debugging_port.is_none()
    {
        (
            "needs_remote_debugging",
            "Codex Desktop 正在普通模式运行，没有 remote debugging 端口；Codex Box 不会自动重启它，请完全退出 Codex 后用带 CDP 的方式启动再解锁。",
        )
    } else if desktop_status.codex_running {
        (
            "no_injectable_target",
            "检测到了 CDP 端口，但没有找到可注入的 Codex renderer 页面。",
        )
    } else {
        (
            "codex_not_running",
            "Codex Desktop 当前未运行。可以使用“启动并解锁”显式以 remote debugging 模式启动 Desktop，然后注入下拉框补丁。",
        )
    };

    Ok(picker_result(
        &catalog, attempt, false, None, status, message,
    ))
}

#[tauri::command]
pub async fn codex_desktop_launch_with_debugging_and_unlock() -> AppResult<CodexPickerUnlockResult>
{
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let catalog = load_picker_catalog_projection(&home)?;
    if catalog.model_names.is_empty() {
        return Err(AppError::Command(
            "模型目录为空；请先在模型配置页添加模型并同步 MultiRouter。".to_string(),
        ));
    }

    let desktop_status = crate::commands::system::codex_desktop_integration_status()?;
    if desktop_status.codex_running && desktop_status.codex_remote_debugging_port.is_none() {
        let attempt =
            inject_picker_catalog(&catalog, desktop_status.codex_remote_debugging_port).await;
        return Ok(picker_result(
            &catalog,
            attempt,
            false,
            None,
            "needs_quit_first",
            "Codex Desktop 已经以普通模式运行。Codex Box 不会强制重启或杀进程；请手动完全退出 Codex Desktop 后再点击“启动并解锁”。",
        ));
    }

    if desktop_status.codex_running {
        let attempt =
            inject_picker_catalog(&catalog, desktop_status.codex_remote_debugging_port).await;
        let injected = attempt.injected_target_count > 0;
        let (status, message) = if injected {
            (
                "injected",
                "Codex Desktop 已有 remote debugging 端口，已直接注入下拉框补丁。",
            )
        } else {
            (
                "no_injectable_target",
                "检测到了 Codex Desktop remote debugging 端口，但没有找到可注入 renderer。",
            )
        };
        return Ok(picker_result(
            &catalog, attempt, false, None, status, message,
        ));
    }

    let launched_path = launch_codex_desktop_with_debug_port(DEFAULT_DEBUG_PORT)?;
    let mut last_attempt = None;
    for _ in 0..30 {
        let attempt = inject_picker_catalog(&catalog, Some(DEFAULT_DEBUG_PORT)).await;
        if attempt.injected_target_count > 0 {
            return Ok(picker_result(
                &catalog,
                attempt,
                true,
                Some(launched_path.clone()),
                "launched_and_injected",
                "已以 remote debugging 模式启动 Codex Desktop，并注入模型下拉框解锁补丁。",
            ));
        }
        last_attempt = Some(attempt);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(picker_result(
        &catalog,
        last_attempt.unwrap_or_else(|| PickerInjectionAttempt::empty(Some(DEFAULT_DEBUG_PORT))),
        true,
        Some(launched_path),
        "launched_waiting_renderer",
        "已尝试以 remote debugging 模式启动 Codex Desktop，但超时前没有发现可注入 renderer；打开 Codex Desktop 主窗口后可再次点击“解锁下拉框”。",
    ))
}

#[derive(Debug, Clone)]
struct PickerInjectionAttempt {
    attempted_ports: Vec<u16>,
    debug_port: Option<u16>,
    target_count: usize,
    injected_target_count: usize,
    renderer_reports: Vec<PickerRendererReport>,
    errors: Vec<String>,
}

impl PickerInjectionAttempt {
    fn empty(preferred: Option<u16>) -> Self {
        Self {
            attempted_ports: candidate_debug_ports(preferred),
            debug_port: preferred,
            target_count: 0,
            injected_target_count: 0,
            renderer_reports: Vec::new(),
            errors: Vec::new(),
        }
    }
}

async fn inject_picker_catalog(
    catalog: &PickerCatalogProjection,
    preferred_port: Option<u16>,
) -> PickerInjectionAttempt {
    let attempted_ports = candidate_debug_ports(preferred_port);
    let mut target_count = 0usize;
    let mut injected_target_count = 0usize;
    let mut selected_port = None;
    let mut renderer_reports = Vec::new();
    let mut errors = Vec::new();
    let script = build_picker_unlock_script(catalog);

    for port in attempted_ports.iter().copied() {
        let targets = match list_cdp_targets(port).await {
            Ok(targets) => targets,
            Err(error) => {
                errors.push(format!("{port}: {error}"));
                continue;
            }
        };
        let targets = pick_codex_page_targets(&targets, port);
        target_count += targets.len();
        if targets.is_empty() {
            errors.push(format!("{port}: no Codex renderer target"));
            continue;
        }

        for target in targets {
            let Some(websocket_url) = target.web_socket_debugger_url.as_deref() else {
                continue;
            };
            match install_picker_script(websocket_url, &script).await {
                Ok(report) => {
                    selected_port.get_or_insert(port);
                    injected_target_count += 1;
                    renderer_reports.push(PickerRendererReport {
                        port,
                        target_id: target.id.clone(),
                        status: report.status,
                        patch_key: report.patch_key,
                        model_count: report.model_count,
                        available_models: report.available_models,
                        error_count: report.error_count,
                    });
                }
                Err(error) => errors.push(format!("{port}/{}: {error}", target.id)),
            }
        }
    }

    PickerInjectionAttempt {
        attempted_ports,
        debug_port: selected_port.or(preferred_port),
        target_count,
        injected_target_count,
        renderer_reports,
        errors,
    }
}

fn picker_result(
    catalog: &PickerCatalogProjection,
    attempt: PickerInjectionAttempt,
    launched: bool,
    codex_executable: Option<String>,
    status: impl Into<String>,
    message: impl Into<String>,
) -> CodexPickerUnlockResult {
    CodexPickerUnlockResult {
        attempted_ports: attempt.attempted_ports,
        debug_port: attempt.debug_port,
        target_count: attempt.target_count,
        injected_target_count: attempt.injected_target_count,
        renderer_reports: attempt.renderer_reports,
        model_count: catalog.model_names.len(),
        model_names: catalog.model_names.clone(),
        injected: attempt.injected_target_count > 0,
        launched,
        codex_executable,
        status: status.into(),
        message: message.into(),
        errors: attempt.errors,
    }
}

fn candidate_debug_ports(preferred: Option<u16>) -> Vec<u16> {
    let mut ports = Vec::new();
    if let Some(preferred) = preferred {
        ports.push(preferred);
    }
    ports.extend([DEFAULT_DEBUG_PORT, 9222, 9223, 9230, 9231]);
    ports.sort_unstable();
    ports.dedup();
    ports
}

fn launch_codex_desktop_with_debug_port(debug_port: u16) -> AppResult<String> {
    let args = codex_debug_args(debug_port);

    #[cfg(target_os = "macos")]
    {
        let app = resolve_macos_codex_app_path();
        let mut command = Command::new("open");
        command.arg("-n");
        if let Some(app) = app.as_deref() {
            command.arg(app);
        } else {
            command.args(["-a", "Codex"]);
        }
        command.arg("--args").args(&args);
        spawn_launch_command(&mut command, "open Codex")?;
        return Ok(app.unwrap_or_else(|| "Codex.app".to_string()));
    }

    #[cfg(target_os = "windows")]
    {
        let executable = resolve_windows_codex_executable()
            .ok_or_else(|| AppError::Command("未找到 Codex Desktop 可执行文件。".to_string()))?;
        let mut command = Command::new(&executable);
        command.args(&args);
        spawn_launch_command(&mut command, "Codex.exe")?;
        return Ok(executable.display().to_string());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let mut command = Command::new("codex");
        command.args(&args);
        spawn_launch_command(&mut command, "codex")?;
        Ok("codex".to_string())
    }
}

fn codex_debug_args(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

fn spawn_launch_command(command: &mut Command, label: &str) -> AppResult<()> {
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| AppError::Command(format!("{label} 启动失败：{error}")))
}

#[cfg(target_os = "macos")]
fn resolve_macos_codex_app_path() -> Option<String> {
    ["/Applications/Codex.app", "~/Applications/Codex.app"]
        .into_iter()
        .filter_map(expand_launch_path)
        .find(|path| Path::new(path).is_dir())
}

#[cfg(target_os = "macos")]
fn expand_launch_path(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest).display().to_string());
    }
    Some(path.to_string())
}

#[cfg(target_os = "windows")]
fn resolve_windows_codex_executable() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("OpenAI").join("Codex").join("Codex.exe"));
    let program_files = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .map(|path| path.join("WindowsApps"));

    local
        .into_iter()
        .chain(
            program_files
                .into_iter()
                .flat_map(windows_codex_package_candidates),
        )
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn windows_codex_package_candidates(root: PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("OpenAI.Codex_"))
        })
        .flat_map(|path| {
            [
                path.join("app").join("Codex.exe"),
                path.join("app").join("resources").join("Codex.exe"),
                path.join("Codex.exe"),
            ]
        })
        .collect()
}

async fn list_cdp_targets(debug_port: u16) -> Result<Vec<CdpTarget>, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(CDP_HTTP_TIMEOUT)
        .build()
        .map_err(|error| format!("build CDP client failed: {error}"))?;
    let mut errors = Vec::new();
    for url in [
        format!("http://127.0.0.1:{debug_port}/json"),
        format!("http://[::1]:{debug_port}/json"),
    ] {
        match client.get(&url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json::<Vec<CdpTarget>>().await {
                    Ok(targets) => return Ok(targets),
                    Err(error) => errors.push(format!("{url}: invalid JSON: {error}")),
                },
                Err(error) => errors.push(format!("{url}: {error}")),
            },
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(errors.join("; "))
}

fn pick_codex_page_targets(targets: &[CdpTarget], debug_port: u16) -> Vec<CdpTarget> {
    let pages = targets.iter().filter(|target| {
        target.target_type == "page"
            && target
                .web_socket_debugger_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty())
    });
    let mut all_pages = Vec::new();
    let mut codex_pages = Vec::new();
    for target in pages {
        all_pages.push(target.clone());
        if target_matches_codex(target) {
            codex_pages.push(target.clone());
        }
    }
    if !codex_pages.is_empty() {
        return codex_pages;
    }
    if debug_port == DEFAULT_DEBUG_PORT {
        return all_pages;
    }
    Vec::new()
}

fn target_matches_codex(target: &CdpTarget) -> bool {
    let text = format!("{} {}", target.title, target.url).to_ascii_lowercase();
    text.contains("codex") || text.contains("app://")
}

#[derive(Debug, Clone, PartialEq)]
struct PickerRuntimeReport {
    status: String,
    patch_key: Option<String>,
    model_count: Option<usize>,
    available_models: Vec<String>,
    error_count: Option<usize>,
}

async fn install_picker_script(
    websocket_url: &str,
    script: &str,
) -> Result<PickerRuntimeReport, String> {
    let (socket, _) = tokio::time::timeout(CDP_CONNECT_TIMEOUT, connect_async(websocket_url))
        .await
        .map_err(|_| "CDP websocket connect timed out".to_string())?
        .map_err(|error| format!("CDP websocket connect failed: {error}"))?;
    let mut session = CdpSession::new(socket);
    session.send_command(1, "Runtime.enable", json!({})).await?;
    session.send_command(2, "Page.enable", json!({})).await?;
    session
        .send_command(
            3,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await?;
    session
        .send_command(
            4,
            "Runtime.evaluate",
            json!({
                "expression": script,
                "awaitPromise": true,
                "returnByValue": true,
                "allowUnsafeEvalBlockedByCSP": true
            }),
        )
        .await
        .and_then(|response| parse_runtime_evaluate_report(&response))
}

struct CdpSession<S> {
    socket: S,
    responses: HashMap<u64, Value>,
}

impl<S> CdpSession<S>
where
    S: Sink<Message>
        + Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
    S::Error: std::fmt::Display,
{
    fn new(socket: S) -> Self {
        Self {
            socket,
            responses: HashMap::new(),
        }
    }

    async fn send_command(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        self.socket
            .send(Message::Text(
                json!({ "id": id, "method": method, "params": params }).to_string(),
            ))
            .await
            .map_err(|error| format!("send CDP command {method} failed: {error}"))?;
        tokio::time::timeout(CDP_COMMAND_TIMEOUT, self.wait_for_id(id, method))
            .await
            .map_err(|_| format!("waiting for CDP command {method} timed out"))?
    }

    async fn wait_for_id(&mut self, id: u64, method: &str) -> Result<Value, String> {
        loop {
            if let Some(response) = self.responses.remove(&id) {
                return cdp_command_result(response, method);
            }
            let Some(message) = self.socket.next().await else {
                return Err(format!("CDP websocket closed before {method} response"));
            };
            let message = message.map_err(|error| format!("read CDP message failed: {error}"))?;
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&text)
                .map_err(|error| format!("parse CDP message failed: {error}"))?;
            if let Some(response_id) = value.get("id").and_then(Value::as_u64) {
                if response_id == id {
                    return cdp_command_result(value, method);
                }
                self.responses.insert(response_id, value);
            }
        }
    }
}

fn cdp_command_result(response: Value, method: &str) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        Err(format!("CDP command {method} failed: {error}"))
    } else {
        Ok(response)
    }
}

fn parse_runtime_evaluate_report(response: &Value) -> Result<PickerRuntimeReport, String> {
    if let Some(exception) = response.pointer("/result/exceptionDetails") {
        return Err(format!("Runtime.evaluate exception: {exception}"));
    }

    let value = response
        .pointer("/result/result/value")
        .ok_or_else(|| "Runtime.evaluate did not return a by-value result".to_string())?;
    let obj = value
        .as_object()
        .ok_or_else(|| "Runtime.evaluate result is not an object".to_string())?;
    let status = obj
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let patch_key = obj
        .get("patchKey")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let model_count = obj
        .get("modelCount")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let available_models = obj
        .get("available_models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let error_count = obj
        .get("errors")
        .and_then(Value::as_array)
        .map(|items| items.len());

    Ok(PickerRuntimeReport {
        status,
        patch_key,
        model_count,
        available_models,
        error_count,
    })
}

fn load_picker_catalog_projection(home: &Path) -> AppResult<PickerCatalogProjection> {
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let config_raw = std::fs::read_to_string(&config_path).unwrap_or_default();
    let catalog_path = catalog_path_from_config(home, &config_path, &config_raw);
    let catalog_raw = std::fs::read_to_string(&catalog_path).map_err(|error| {
        AppError::Command(format!(
            "读取模型目录失败 {}: {error}",
            catalog_path.display()
        ))
    })?;
    build_picker_catalog_projection(&catalog_raw, current_model_from_config(&config_raw))
}

fn catalog_path_from_config(home: &Path, config_path: &Path, raw: &str) -> PathBuf {
    let parsed = toml::from_str::<toml::Value>(raw).ok();
    let value = parsed
        .as_ref()
        .and_then(|value| value.get("model_catalog_json"))
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(value) = value else {
        return home.join(DEFAULT_CATALOG_PATH);
    };
    if let Some(rest) = value.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        config_path.parent().unwrap_or(home).join(path)
    }
}

fn current_model_from_config(raw: &str) -> Option<String> {
    toml::from_str::<toml::Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(toml::Value::as_str)
                .map(str::trim)
                .map(ToString::to_string)
        })
        .filter(|value| !value.is_empty())
}

fn build_picker_catalog_projection(
    raw: &str,
    current_model: Option<String>,
) -> AppResult<PickerCatalogProjection> {
    let value = serde_json::from_str::<Value>(raw)
        .map_err(|error| AppError::Command(format!("模型目录 JSON 解析失败: {error}")))?;
    let models = value
        .as_object()
        .and_then(|obj| obj.get("models"))
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .ok_or_else(|| AppError::Command("模型目录缺少 models 数组。".to_string()))?;

    let mut seen = BTreeSet::new();
    let mut names = Vec::new();
    let mut descriptors = Vec::new();
    for entry in models {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        if !catalog_entry_visible(obj) {
            continue;
        }
        let Some(model_id) = catalog_model_id(obj) else {
            continue;
        };
        if !seen.insert(model_id.clone()) {
            continue;
        }
        names.push(model_id.clone());
        descriptors.push(catalog_descriptor(obj, &model_id));
    }

    let default_model = current_model
        .filter(|model| seen.contains(model))
        .or_else(|| names.first().cloned());

    Ok(PickerCatalogProjection {
        default_model,
        model_names: names,
        models: descriptors,
    })
}

fn catalog_entry_visible(obj: &Map<String, Value>) -> bool {
    let visible = obj.get("visible").and_then(Value::as_bool).unwrap_or(true);
    let hidden = obj.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    visible && !hidden
}

fn catalog_model_id(obj: &Map<String, Value>) -> Option<String> {
    ["model_id", "modelId", "model", "slug", "id"]
        .iter()
        .find_map(|key| obj.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn catalog_display_name(obj: &Map<String, Value>, model_id: &str) -> String {
    ["display_name", "displayName", "name"]
        .iter()
        .find_map(|key| obj.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(model_id)
        .to_string()
}

fn catalog_descriptor(obj: &Map<String, Value>, model_id: &str) -> Value {
    let mut descriptor = obj.clone();
    let display_name = catalog_display_name(obj, model_id);
    descriptor.insert("model".to_string(), json!(model_id));
    descriptor.insert("id".to_string(), json!(model_id));
    descriptor.insert("slug".to_string(), json!(model_id));
    descriptor.insert("name".to_string(), json!(display_name));
    descriptor.insert("displayName".to_string(), json!(display_name));
    descriptor.insert("hidden".to_string(), json!(false));
    descriptor.insert("defaultReasoningEffort".to_string(), json!("medium"));
    Value::Object(descriptor)
}

fn build_picker_unlock_script(catalog: &PickerCatalogProjection) -> String {
    let payload = serde_json::to_string(catalog).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"
(async () => {{
  const payload = {payload};
  const patchKey = "{PICKER_PATCH_KEY}";
  const state = window[patchKey] || {{}};
  state.payload = payload;
  state.pendingModelListRequests = state.pendingModelListRequests || new Set();
  state.modulePromises = state.modulePromises || new Map();
  state.errors = state.errors || [];
  window[patchKey] = state;

  const modelNames = () => Array.from(new Set([payload.defaultModel, ...(payload.modelNames || [])].filter((name) => typeof name === "string" && name.trim()).map((name) => name.trim())));
  const descriptorFor = (name) => {{
    const existing = (payload.models || []).find((model) => model && (model.model === name || model.id === name || model.slug === name)) || {{}};
    return {{
      ...existing,
      model: name,
      id: name,
      slug: name,
      name: existing.name || existing.displayName || name,
      displayName: existing.displayName || existing.display_name || existing.name || name,
      hidden: false,
      defaultReasoningEffort: existing.defaultReasoningEffort || existing.default_reasoning_effort || "medium",
    }};
  }};
  const patchModelArray = (items, allowEmpty = false) => {{
    if (!Array.isArray(items) || (!allowEmpty && items.length === 0)) return false;
    let changed = false;
    const byName = new Set();
    for (const item of items) {{
      if (typeof item === "string") {{
        byName.add(item);
        continue;
      }}
      if (!item || typeof item !== "object") continue;
      const name = item.model || item.id || item.slug;
      if (typeof name === "string") byName.add(name);
      if (modelNames().includes(name) && item.hidden !== false) {{
        item.hidden = false;
        changed = true;
      }}
    }}
    for (const name of modelNames()) {{
      if (!byName.has(name)) {{
        items.push(descriptorFor(name));
        changed = true;
      }}
    }}
    return changed;
  }};
  const patchNameArray = (items) => {{
    if (!Array.isArray(items) || !items.every((item) => typeof item === "string")) return false;
    let changed = false;
    for (const name of modelNames()) {{
      if (!items.includes(name)) {{
        items.push(name);
        changed = true;
      }}
    }}
    return changed;
  }};
  const patchNameSet = (items) => {{
    if (!(items instanceof Set)) return false;
    let changed = false;
    for (const name of modelNames()) {{
      if (!items.has(name)) {{
        items.add(name);
        changed = true;
      }}
    }}
    return changed;
  }};
  const removeHidden = (value, key) => {{
    if (!Array.isArray(value?.[key])) return false;
    const names = new Set(modelNames());
    const before = value[key].length;
    value[key] = value[key].filter((name) => !names.has(name));
    return before !== value[key].length;
  }};
  const patchContainer = (value) => {{
    if (!value || typeof value !== "object") return false;
    let changed = false;
    const looksLikePickerGate = "available_models" in value || "availableModels" in value || "use_hidden_models" in value || "useHiddenModels" in value || "default_model" in value || "defaultModel" in value;
    if (patchModelArray(value.models, looksLikePickerGate)) changed = true;
    if (patchNameArray(value.models)) changed = true;
    if (patchModelArray(value.data)) changed = true;
    if (patchModelArray(value.result)) changed = true;
    if (patchModelArray(value.pages?.[0]?.data)) changed = true;
    if (patchModelArray(value.result?.data)) changed = true;
    if (patchModelArray(value.result?.models)) changed = true;
    if (patchModelArray(value.message?.result?.data)) changed = true;
    if (patchModelArray(value.message?.result?.models)) changed = true;
    if (patchNameArray(value.available_models)) changed = true;
    if (patchNameArray(value.availableModels)) changed = true;
    if (patchNameSet(value.available_models)) changed = true;
    if (patchNameSet(value.availableModels)) changed = true;
    if (removeHidden(value, "hidden_models")) changed = true;
    if (removeHidden(value, "hiddenModels")) changed = true;
    if (looksLikePickerGate && value.use_hidden_models !== false) {{
      value.use_hidden_models = false;
      changed = true;
    }}
    if (looksLikePickerGate && value.useHiddenModels !== false) {{
      value.useHiddenModels = false;
      changed = true;
    }}
    if (looksLikePickerGate && modelNames()[0] && typeof value.default_model === "string" && !modelNames().includes(value.default_model)) {{
      value.default_model = modelNames()[0];
      changed = true;
    }}
    if (looksLikePickerGate && value.defaultModel == null && modelNames()[0]) {{
      value.defaultModel = descriptorFor(modelNames()[0]);
      changed = true;
    }}
    return changed;
  }};
  const patchGraph = (root, seen = new WeakSet(), depth = 0) => {{
    if (!root || typeof root !== "object" || seen.has(root) || depth > 5) return false;
    seen.add(root);
    let changed = patchContainer(root);
    if (root instanceof Element || root === window || root === document || root === document.body || root === document.documentElement) return changed;
    for (const key of Object.keys(root)) {{
      if (["ownerDocument", "parentElement", "parentNode", "children", "childNodes"].includes(key)) continue;
      try {{
        if (patchGraph(root[key], seen, depth + 1)) changed = true;
      }} catch {{}}
    }}
    return changed;
  }};
  const isPickerStatsigConfig = (name, value) => {{
    const configName = String(name || value?.name || value?.configName || value?.key || "");
    return configName === "107580212" || configName.includes("107580212") || !!(value && typeof value === "object" && ("available_models" in value || "availableModels" in value || "use_hidden_models" in value || "useHiddenModels" in value));
  }};
  const patchStatsigConfig = (config, name) => {{
    const value = config?.value;
    if (!value || typeof value !== "object") return config;
    if (!isPickerStatsigConfig(name, value)) return config;
    const next = {{ ...value }};
    next.available_models = Array.isArray(next.available_models) ? [...next.available_models] : [];
    next.availableModels = Array.isArray(next.availableModels) ? [...next.availableModels] : [];
    patchNameArray(next.available_models);
    patchNameArray(next.availableModels);
    next.use_hidden_models = false;
    next.useHiddenModels = false;
    if (modelNames()[0]) next.default_model = modelNames()[0];
    if (modelNames()[0]) next.defaultModel = descriptorFor(modelNames()[0]);
    try {{
      config.value = next;
      return config;
    }} catch {{
      return {{ ...config, value: next }};
    }}
  }};
  const patchStatsig = () => {{
    const root = window.__STATSIG__ || globalThis.__STATSIG__;
    if (!root || typeof root !== "object") return;
    const clients = [root.firstInstance, typeof root.instance === "function" ? root.instance() : null, ...(root.instances ? Object.values(root.instances) : [])].filter(Boolean);
    for (const client of clients) {{
      if (!client || typeof client.getDynamicConfig !== "function" || client.__codexBoxPickerPatched) continue;
      const original = client.getDynamicConfig.bind(client);
      client.getDynamicConfig = (name, options) => patchStatsigConfig(original(name, options), name);
      client.__codexBoxPickerPatched = true;
      try {{ patchStatsigConfig(client.getDynamicConfig("107580212", {{ disableExposureLog: true }}), "107580212"); }} catch {{}}
    }}
  }};
  const assetUrl = (namePart) => {{
    const urls = [
      ...Array.from(document.scripts || []).map((script) => script.src),
      ...Array.from(document.querySelectorAll("link[href]") || []).map((link) => link.href),
      ...performance.getEntriesByType("resource").map((entry) => entry.name),
    ].filter(Boolean);
    return urls.find((url) => url.includes("/assets/") && url.includes(namePart) && url.split("?")[0].endsWith(".js")) || "";
  }};
  const loadAppModule = async (namePart) => {{
    if (!state.modulePromises.has(namePart)) {{
      state.modulePromises.set(namePart, Promise.resolve().then(async () => {{
        const url = assetUrl(namePart);
        if (!url) throw new Error("Codex App asset not found: " + namePart);
        return await import(url);
      }}).catch((error) => {{
        state.modulePromises.delete(namePart);
        throw error;
      }}));
    }}
    return await state.modulePromises.get(namePart);
  }};
  const appServerMethod = (method, params) => method === "send-cli-request-for-host" && params?.method ? String(params.method) : String(method || "");
  const patchAppServerResult = (method, result) => {{
    if (method !== "list-models-for-host") return result;
    if (Array.isArray(result)) patchModelArray(result, true);
    if (Array.isArray(result?.data)) patchModelArray(result.data, true);
    if (Array.isArray(result?.models)) patchModelArray(result.models, true);
    patchContainer(result);
    patchGraph(result);
    return result;
  }};
  const patchRequestClient = (client) => {{
    if (!client || typeof client.sendRequest !== "function") return false;
    if (client.__codexBoxPickerRequestPatched) return true;
    const original = client.__codexBoxOriginalSendRequest || client.sendRequest.bind(client);
    client.__codexBoxOriginalSendRequest = original;
    client.sendRequest = async function codexBoxPickerSendRequest(method, params, options) {{
      const result = await original(method, params, options);
      return patchAppServerResult(appServerMethod(method, params), result);
    }};
    client.__codexBoxPickerRequestPatched = true;
    return true;
  }};
  const installAppServerPatch = async () => {{
    try {{
      const module = await loadAppModule("app-server-manager-signals-");
      for (const candidate of Object.values(module).filter((item) => item && typeof item === "object")) {{
        patchRequestClient(candidate);
        if (typeof candidate.sendRequest !== "function" && typeof candidate.get === "function") {{
          try {{ patchRequestClient(candidate.get()); }} catch {{}}
        }}
      }}
    }} catch (error) {{
      state.errors.push(String(error?.message || error));
    }}
  }};
  const installResponsePatch = () => {{
    if (state.responsePatchInstalled || typeof Response === "undefined") return;
    state.responsePatchInstalled = true;
    const originalJson = Response.prototype.json;
    Response.prototype.json = async function codexBoxPickerJson(...args) {{
      const data = await originalJson.apply(this, args);
      try {{ patchContainer(data); patchGraph(data); }} catch (error) {{ state.errors.push(String(error?.message || error)); }}
      return data;
    }};
  }};
  const patchModelListMessage = (data) => {{
    if (!data || typeof data !== "object") return false;
    if (data.type === "mcp-response") {{
      return patchContainer(data) || patchContainer(data.message) || patchContainer(data.message?.result) || patchContainer(data.message?.result?.data);
    }}
    return patchContainer(data);
  }};
  const installMessagePatch = () => {{
    if (state.messagePatchInstalled) return;
    state.messagePatchInstalled = true;
    const originalDispatch = window.dispatchEvent;
    window.dispatchEvent = function codexBoxPickerDispatch(event) {{
      try {{
        const detail = event?.detail;
        const request = detail?.request;
        if (event?.type === "codex-message-from-view" && detail?.type === "mcp-request" && request?.method === "model/list") {{
          request.params = {{ ...(request.params || {{}}), includeHidden: true }};
          if (request.id != null) state.pendingModelListRequests.add(String(request.id));
        }}
        if (event?.type === "message") patchModelListMessage(event.data);
      }} catch (error) {{ state.errors.push(String(error?.message || error)); }}
      return originalDispatch.call(this, event);
    }};
    window.addEventListener("message", (event) => {{
      try {{ patchModelListMessage(event?.data); }} catch (error) {{ state.errors.push(String(error?.message || error)); }}
    }}, true);
  }};
  const reactFiberKeys = (element) => Object.keys(element || {{}}).filter((key) => key.startsWith("__reactFiber") || key.startsWith("__reactInternalInstance") || key.startsWith("__reactProps"));
  const authContextValueFrom = (element) => {{
    for (const key of reactFiberKeys(element)) {{
      for (let fiber = element?.[key]; fiber; fiber = fiber.return) {{
        for (const value of [fiber.memoizedProps?.value, fiber.pendingProps?.value]) {{
          if (value && typeof value === "object" && typeof value.setAuthMethod === "function" && "authMethod" in value) return value;
        }}
      }}
    }}
    return null;
  }};
  const spoofChatGPTAuthMethod = (element) => {{
    const auth = authContextValueFrom(element);
    if (!auth || auth.authMethod === "chatgpt") return false;
    try {{
      auth.setAuthMethod("chatgpt");
      return true;
    }} catch (error) {{
      state.errors.push(String(error?.message || error));
      return false;
    }}
  }};
  const patchReactState = () => {{
    const seen = new WeakSet();
    const nodes = [document.body, ...document.querySelectorAll("button, [role='menu'], [role='dialog'], [data-radix-popper-content-wrapper]")].filter(Boolean).slice(0, 220);
    for (const node of nodes) {{
      spoofChatGPTAuthMethod(node);
      for (const key of reactFiberKeys(node)) patchGraph(node[key], seen);
    }}
  }};
  const run = () => {{
    installResponsePatch();
    installMessagePatch();
    void installAppServerPatch();
    patchStatsig();
    patchReactState();
  }};
  run();
  if (!state.interval) state.interval = setInterval(run, 1500);
  return {{ status: "ok", patchKey, modelCount: modelNames().length, available_models: modelNames() }};
}})()
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_keeps_visible_native_and_byok_models() {
        let raw = r#"{
          "models": [
            { "model": "gpt-5.5", "provider": "openai", "display_name": "GPT-5.5" },
            { "model_id": "minimax-m3", "provider": "codex_model_router_v2", "backend_provider": "minimax", "displayName": "MiniMax-M3" },
            { "model": "hidden", "visible": false }
          ]
        }"#;
        let projection =
            build_picker_catalog_projection(raw, Some("minimax-m3".to_string())).unwrap();

        assert_eq!(projection.default_model.as_deref(), Some("minimax-m3"));
        assert_eq!(projection.model_names, vec!["gpt-5.5", "minimax-m3"]);
        assert_eq!(projection.models.len(), 2);
        assert_eq!(projection.models[1]["displayName"], "MiniMax-M3");
    }

    #[test]
    fn picker_unlock_script_contains_renderer_patch_points() {
        let projection = PickerCatalogProjection {
            default_model: Some("minimax-m3".to_string()),
            model_names: vec!["gpt-5.5".to_string(), "minimax-m3".to_string()],
            models: vec![json!({ "model": "minimax-m3", "displayName": "MiniMax-M3" })],
        };
        let script = build_picker_unlock_script(&projection);

        assert!(script.contains(PICKER_PATCH_KEY));
        assert!(script.contains("available_models"));
        assert!(script.contains("availableModels"));
        assert!(script.contains("use_hidden_models"));
        assert!(script.contains("useHiddenModels"));
        assert!(script.contains("107580212"));
        assert!(script.contains("isPickerStatsigConfig"));
        assert!(script.contains("patchStatsigConfig(original(name, options), name)"));
        assert!(script.contains("model/list"));
        assert!(script.contains("Response.prototype.json"));
        assert!(script.contains("list-models-for-host"));
        assert!(script.contains("app-server-manager-signals-"));
        assert!(script.contains("send-cli-request-for-host"));
        assert!(script.contains("auth.setAuthMethod(\"chatgpt\")"));
        assert!(script.contains("defaultModel"));
    }

    #[test]
    fn runtime_evaluate_report_reads_renderer_result() {
        let response = json!({
            "id": 4,
            "result": {
                "result": {
                    "type": "object",
                    "value": {
                        "status": "ok",
                        "patchKey": PICKER_PATCH_KEY,
                        "modelCount": 2,
                        "available_models": ["gpt-5.5", "minimax-m3"],
                        "errors": ["late module"]
                    }
                }
            }
        });

        let report = parse_runtime_evaluate_report(&response).unwrap();

        assert_eq!(report.status, "ok");
        assert_eq!(report.patch_key.as_deref(), Some(PICKER_PATCH_KEY));
        assert_eq!(report.model_count, Some(2));
        assert_eq!(report.available_models, vec!["gpt-5.5", "minimax-m3"]);
        assert_eq!(report.error_count, Some(1));
    }

    #[test]
    fn runtime_evaluate_report_rejects_script_exception() {
        let response = json!({
            "id": 4,
            "result": {
                "result": { "type": "object" },
                "exceptionDetails": { "text": "Uncaught", "exception": { "description": "boom" } }
            }
        });

        let error = parse_runtime_evaluate_report(&response).unwrap_err();

        assert!(error.contains("Runtime.evaluate exception"));
        assert!(error.contains("boom"));
    }

    #[test]
    fn launch_debug_args_match_codex_desktop_cdp_contract() {
        let args = codex_debug_args(9229);

        assert_eq!(
            args,
            vec![
                "--remote-debugging-port=9229".to_string(),
                "--remote-allow-origins=http://127.0.0.1:9229".to_string(),
            ]
        );
    }

    #[test]
    fn shared_ports_require_codex_target_but_default_port_can_fallback() {
        let targets = vec![CdpTarget {
            id: "1".to_string(),
            target_type: "page".to_string(),
            title: "Chrome".to_string(),
            url: "https://example.com".to_string(),
            web_socket_debugger_url: Some("ws://127.0.0.1/devtools/page/1".to_string()),
        }];

        assert!(pick_codex_page_targets(&targets, 9222).is_empty());
        assert_eq!(
            pick_codex_page_targets(&targets, DEFAULT_DEBUG_PORT).len(),
            1
        );
    }
}
