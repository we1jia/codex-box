// src-tauri/src/commands/proxy.rs
//
// Tauri command 边界: 把 Codex Box 本地代理 runtime 暴露给前端。
//
// 命令清单:
//   - proxy_status        : 读 ProxyState 当前视图
//   - proxy_start         : 启动后台代理(写入 runtime-state)
//   - proxy_stop          : 发送 shutdown 信号
//   - proxy_restart       : stop + start
//   - proxy_models_preview: 调本机代理 /v1/models 返回 JSON
//   - proxy_inject_base_url: 把 [model_providers.*] 的 base_url 改成 127.0.0.1:{port}/v1,写 inject-map
//   - proxy_restore_base_url: 从 inject-map 反向恢复 base_url
//
// 所有写入都走 backup → diff → confirm → atomic write → rollback。
use crate::config::loader;
use crate::config::model::{BackupRecord, DiffLine};
use crate::config::parser as cfg_parser;
use crate::config::writer as cfg_writer;
use crate::error::{AppError, AppResult};
use crate::proxy::inject_map::{self, InjectMap, InjectMapEntry, InjectMapWriteResult};
use crate::proxy::responses_ws::{
    responses_create_to_chat_request_with_options, ChatRequestOptions,
};
use crate::proxy::routing::{resolve_catalog_route, resolve_route, ResolvedRoute};
use crate::proxy::state::{persist_runtime_state, ProxyState, ProxyStatus, ProxyStatusView};
use crate::proxy::upstream::{build_upstream_url, inject_auth_headers};
use crate::proxy::vision_bridge::{apply_vision_bridge, replace_images_with_vision_bridge_marker};
use crate::proxy::DEFAULT_PROXY_PORT;
use axum::http::HeaderMap;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const BACKUP_DIR: &str = ".codex/codex-box/backups";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyModelsPreview {
    pub base_url: String,
    pub raw_json: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRouteTestRequest {
    pub model_id: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub include_image: Option<bool>,
    #[serde(default)]
    pub perform_upstream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRouteTestStep {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRouteTestResult {
    pub status: String,
    pub model_id: String,
    pub provider_name: Option<String>,
    pub upstream_model: Option<String>,
    pub upstream_base_url: Option<String>,
    pub wire_api: Option<String>,
    pub auth_source: Option<String>,
    pub text_only: bool,
    pub used_chat_fallback: bool,
    pub image_part_sent_to_chat: bool,
    pub upstream_status_code: Option<u16>,
    pub upstream_latency_ms: Option<u64>,
    pub chat_request_preview: Option<serde_json::Value>,
    pub steps: Vec<ProxyRouteTestStep>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeLogEntry {
    pub at: String,
    pub level: String,
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeLogs {
    pub redacted: bool,
    pub items: Vec<ProxyRuntimeLogEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxySessionEntry {
    pub id: String,
    pub label: String,
    pub status: String,
    pub provider_count: usize,
    pub model_count: usize,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxySessionsView {
    pub active_session_id: Option<String>,
    pub sessions: Vec<ProxySessionEntry>,
}

#[tauri::command]
pub fn proxy_status(state: tauri::State<Arc<ProxyState>>) -> ProxyStatusView {
    crate::proxy::lifecycle::reconcile_running_state(state.inner());
    state.inner().to_view()
}

#[tauri::command]
pub fn proxy_runtime_logs(state: tauri::State<Arc<ProxyState>>) -> ProxyRuntimeLogs {
    crate::proxy::lifecycle::reconcile_running_state(state.inner());
    build_runtime_logs(state.inner(), state.inner().to_view())
}

#[tauri::command]
pub fn proxy_sessions(state: tauri::State<Arc<ProxyState>>) -> ProxySessionsView {
    crate::proxy::lifecycle::reconcile_running_state(state.inner());
    build_sessions(state.inner().to_view())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStartRequest {
    /// 可选;缺省走 DEFAULT_PROXY_PORT
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStartResult {
    pub port: u16,
    pub status: String,
}

#[tauri::command]
pub async fn proxy_start(
    request: ProxyStartRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ProxyStartResult> {
    // 启动前先 reload inject-map 到内存(覆盖之前的)
    let map = inject_map::read_inject_map()?;
    state.inner().set_inject_map(map);

    let port = request.port.unwrap_or(DEFAULT_PROXY_PORT);
    validate_proxy_port(port)?;
    let port = crate::proxy::lifecycle::start(state.inner().clone(), port)
        .await
        .map_err(|e| AppError::Proxy(e.to_string()))?;
    Ok(ProxyStartResult {
        port,
        status: ProxyStatus::Running.as_str().to_string(),
    })
}

#[tauri::command]
pub fn proxy_stop(state: tauri::State<Arc<ProxyState>>) -> AppResult<ProxyStatusView> {
    crate::proxy::lifecycle::stop(&state);
    Ok(state.to_view())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRestartRequest {
    pub port: Option<u16>,
}

#[tauri::command]
pub async fn proxy_restart(
    request: ProxyRestartRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ProxyStartResult> {
    let port = request.port.unwrap_or(DEFAULT_PROXY_PORT);
    validate_proxy_port(port)?;
    let port = crate::proxy::lifecycle::restart(state.inner().clone(), port)
        .await
        .map_err(|e| AppError::Proxy(e.to_string()))?;
    Ok(ProxyStartResult {
        port,
        status: ProxyStatus::Running.as_str().to_string(),
    })
}

#[tauri::command]
pub async fn proxy_models_preview(
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ProxyModelsPreview> {
    crate::proxy::lifecycle::reconcile_running_state(state.inner());
    let port = state.port();
    let base_url = format!("http://127.0.0.1:{port}/v1/models");
    if state.status() != ProxyStatus::Running {
        return Err(AppError::Proxy("proxy is not running".to_string()));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Proxy(format!("client build: {e}")))?;
    let resp = client
        .get(&base_url)
        .send()
        .await
        .map_err(|e| AppError::Proxy(format!("GET {base_url}: {e}")))?;
    let raw_json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Proxy(format!("parse response: {e}")))?;
    Ok(ProxyModelsPreview { base_url, raw_json })
}

#[tauri::command]
pub async fn proxy_route_test(
    request: ProxyRouteTestRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ProxyRouteTestResult> {
    let model_id = request.model_id.trim();
    if model_id.is_empty() {
        return Err(AppError::Command("model_id 不能为空".to_string()));
    }

    let mut map = state.inner().inject_map();
    if map.providers.is_empty() {
        map = inject_map::read_inject_map().unwrap_or_default();
    }
    let mut result = build_proxy_route_test_result(&request, &map)?;
    if request.perform_upstream.unwrap_or(false) && result.status != "failed" {
        if let Some(route) =
            resolve_catalog_route(model_id, &map).or_else(|| resolve_route(model_id, &map))
        {
            apply_upstream_route_test(&mut result, &request, &route).await?;
        }
    }
    Ok(result)
}

// ============================================================
// Inject / Restore: 改写 ~/.codex/config.toml 的 base_url
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectBaseUrlRequest {
    /// 代理监听端口
    pub port: u16,
    /// 期望的 content_hash(可选,缺省不校验)
    #[serde(default)]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectBaseUrlPreview {
    pub new_config_text: String,
    pub new_hash: String,
    pub diff: Vec<DiffLine>,
    pub insertions: usize,
    pub deletions: usize,
    pub inject_map: InjectMap,
    pub inject_map_hash: String,
    pub backup_id: String,
}

#[tauri::command]
pub fn proxy_inject_base_url_preview(
    request: InjectBaseUrlRequest,
) -> AppResult<InjectBaseUrlPreview> {
    validate_proxy_port(request.port)?;
    let path = resolve_config_path()?;
    let old_text = loader::read_raw(&path)?;
    let old_hash = loader::metadata(&path)?.content_hash;

    if let Some(ref expected) = request.expected_hash {
        if expected != &old_hash {
            return Err(AppError::Command(
                "config.toml 已变化,请重新读取后再注入".to_string(),
            ));
        }
    }

    let config = cfg_parser::parse(&old_text)?;
    let (new_text, inject_map) = rewrite_base_urls(&old_text, &config, request.port)?;
    let new_hash = format!("sha256-{}", short_hash(&new_text));
    let diff_lines = crate::config::diff::between(&old_text, &new_text);
    let (_, insertions, deletions) = crate::config::diff::count_by_kind(&diff_lines);

    let inject_map_hash = current_inject_map_hash()?;

    Ok(InjectBaseUrlPreview {
        new_config_text: new_text,
        new_hash,
        diff: diff_lines,
        insertions,
        deletions,
        inject_map,
        inject_map_hash,
        backup_id: String::new(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyInjectRequest {
    pub preview: InjectBaseUrlPreview,
    /// 必须为 true,表示用户已确认 diff
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyInjectResult {
    pub new_config_hash: String,
    pub inject_map_write: InjectMapWriteResult,
    pub backup: BackupRecord,
}

#[tauri::command]
pub fn proxy_inject_base_url_apply(
    request: ApplyInjectRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ApplyInjectResult> {
    if !request.confirmed {
        return Err(AppError::Command("应用注入需要 confirmed=true".to_string()));
    }

    let config_path = resolve_config_path()?;
    let backup_dir_path = backup_dir()?;

    // 1) 写 ~/.codex/config.toml
    let cfg_backup = crate::config::backup::create_backup(
        &config_path,
        &backup_dir_path,
        crate::config::model::BackupReason::PreWrite,
    )?;
    cfg_writer::atomic_write(&config_path, &request.preview.new_config_text)?;

    // 2) 写 inject-map.json(同 backup 闭环)
    let inject_result = inject_map::write_inject_map(
        &request.preview.inject_map,
        &request.preview.inject_map_hash,
        Some("inject base_url → Codex Box proxy"),
    )?;

    // 3) 同步到内存 + 持久化 runtime-state
    state
        .inner()
        .set_inject_map(request.preview.inject_map.clone());
    persist_runtime_state(state.inner());

    // 4) 算新 hash
    let new_cfg_hash = format!("sha256-{}", short_hash(&request.preview.new_config_text));

    Ok(ApplyInjectResult {
        new_config_hash: new_cfg_hash,
        inject_map_write: inject_result,
        backup: cfg_backup,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBaseUrlRequest {
    /// 期望的 config.toml hash(可选,缺省不校验)
    #[serde(default)]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBaseUrlPreview {
    pub new_config_text: String,
    pub new_hash: String,
    pub diff: Vec<DiffLine>,
    pub insertions: usize,
    pub deletions: usize,
    pub restored_count: usize,
}

#[tauri::command]
pub fn proxy_restore_base_url_preview(
    request: RestoreBaseUrlRequest,
) -> AppResult<RestoreBaseUrlPreview> {
    let map = inject_map::read_inject_map()?;
    if map.providers.is_empty() {
        return Err(AppError::Command(
            "没有可还原的 inject-map(inject-map 为空)".to_string(),
        ));
    }
    let path = resolve_config_path()?;
    let old_text = loader::read_raw(&path)?;
    let old_hash = loader::metadata(&path)?.content_hash;
    if let Some(ref expected) = request.expected_hash {
        if expected != &old_hash {
            return Err(AppError::Command(
                "config.toml 已变化,请重新读取后再还原".to_string(),
            ));
        }
    }

    let new_text = restore_base_urls(&old_text, &map.providers);
    let new_hash = format!("sha256-{}", short_hash(&new_text));
    let diff_lines = crate::config::diff::between(&old_text, &new_text);
    let (_, insertions, deletions) = crate::config::diff::count_by_kind(&diff_lines);

    Ok(RestoreBaseUrlPreview {
        new_config_text: new_text,
        new_hash,
        diff: diff_lines,
        insertions,
        deletions,
        restored_count: map.providers.len(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRestoreRequest {
    pub preview: RestoreBaseUrlPreview,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRestoreResult {
    pub new_config_hash: String,
    pub backup: BackupRecord,
    pub inject_map: InjectMap,
    pub cleared_inject_map_hash: String,
}

#[tauri::command]
pub fn proxy_restore_base_url_apply(
    request: ApplyRestoreRequest,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> AppResult<ApplyRestoreResult> {
    if !request.confirmed {
        return Err(AppError::Command("应用还原需要 confirmed=true".to_string()));
    }

    let config_path = resolve_config_path()?;
    let backup_dir_path = backup_dir()?;

    let cfg_backup = crate::config::backup::create_backup(
        &config_path,
        &backup_dir_path,
        crate::config::model::BackupReason::PreWrite,
    )?;
    cfg_writer::atomic_write(&config_path, &request.preview.new_config_text)?;

    // 清空 inject-map.json(走 backup 闭环)
    let empty = InjectMap::default();
    let path = inject_map::inject_map_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let empty_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&empty).unwrap_or_default()
    );
    // 如果当前 inject-map 存在,先 backup
    if path.exists() {
        let _ = crate::config::backup::create_backup_with_extension(
            &path,
            &backup_dir()?,
            crate::config::model::BackupReason::PreWrite,
            "json",
        );
    }
    cfg_writer::atomic_write(&path, &empty_text)?;
    let cleared_inject_map_hash = format!("sha256-{}", short_hash(&empty_text));

    state.inner().set_inject_map(empty.clone());
    persist_runtime_state(state.inner());

    let new_cfg_hash = format!("sha256-{}", short_hash(&request.preview.new_config_text));
    Ok(ApplyRestoreResult {
        new_config_hash: new_cfg_hash,
        backup: cfg_backup,
        inject_map: empty,
        cleared_inject_map_hash,
    })
}

// ============================================================
// 内部辅助
// ============================================================

fn build_proxy_route_test_result(
    request: &ProxyRouteTestRequest,
    map: &InjectMap,
) -> AppResult<ProxyRouteTestResult> {
    let model_id = request.model_id.trim();
    let include_image = request.include_image.unwrap_or(true);
    let perform_upstream = request.perform_upstream.unwrap_or(false);
    let route = resolve_catalog_route(model_id, map).or_else(|| resolve_route(model_id, map));
    let Some(route) = route else {
        return Ok(ProxyRouteTestResult {
            status: "failed".to_string(),
            model_id: model_id.to_string(),
            provider_name: None,
            upstream_model: None,
            upstream_base_url: None,
            wire_api: None,
            auth_source: None,
            text_only: false,
            used_chat_fallback: false,
            image_part_sent_to_chat: false,
            upstream_status_code: None,
            upstream_latency_ms: None,
            chat_request_preview: None,
            steps: vec![ProxyRouteTestStep {
                id: "resolver".to_string(),
                label: "Resolver".to_string(),
                status: "failed".to_string(),
                detail: format!("模型 `{model_id}` 没有命中 catalog、MultiRouter 或 inject-map。"),
            }],
            warnings: Vec::new(),
        });
    };

    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    steps.push(ProxyRouteTestStep {
        id: "resolver".to_string(),
        label: "Resolver".to_string(),
        status: "passed".to_string(),
        detail: format!(
            "{} -> {} ({})",
            model_id, route.provider_name, route.model_id
        ),
    });

    let route_uses_chat_fallback = route_uses_chat_fallback(&route);
    let mut message = build_route_test_response_create(
        model_id,
        request.prompt.as_deref().unwrap_or("Codex Box route test"),
        include_image,
    );
    if include_image && route.vision_bridge.is_some() {
        let bridged = replace_images_with_vision_bridge_marker(
            &mut message,
            "route test dry-run vision bridge placeholder",
        );
        if bridged > 0 {
            steps.push(ProxyRouteTestStep {
                id: "vision_bridge".to_string(),
                label: "Vision bridge".to_string(),
                status: "passed".to_string(),
                detail: format!(
                    "dry-run 已模拟把 {bridged} 个 input_image 替换为截图描述占位；真实请求会先调用配置的视觉上游。"
                ),
            });
        }
    }

    let mut used_chat_fallback = false;
    let mut image_part_sent_to_chat = false;
    let mut chat_request_preview = None;

    if route_uses_chat_fallback {
        used_chat_fallback = true;
        let chat = responses_create_to_chat_request_with_options(
            &message,
            &route.model_id,
            ChatRequestOptions {
                chat_reasoning: route.chat_reasoning.as_ref(),
                text_only_input: route.text_only,
            },
        )?;
        image_part_sent_to_chat = contains_chat_image_url(&chat.body);
        chat_request_preview = Some(redact_route_test_preview(chat.body));
        steps.push(ProxyRouteTestStep {
            id: "protocol_transform".to_string(),
            label: "Responses -> Chat".to_string(),
            status: "passed".to_string(),
            detail: "response.create 已转换成 OpenAI-compatible chat/completions 请求。"
                .to_string(),
        });
    } else {
        steps.push(ProxyRouteTestStep {
            id: "protocol_transform".to_string(),
            label: "Responses passthrough".to_string(),
            status: "skipped".to_string(),
            detail: "该 route 声明为 openai_responses，dry-run 不改写请求体。".to_string(),
        });
    }

    let mut failed = false;
    if include_image && route.text_only {
        if used_chat_fallback {
            if image_part_sent_to_chat {
                failed = true;
                steps.push(ProxyRouteTestStep {
                    id: "text_only_guard".to_string(),
                    label: "Text-only guard".to_string(),
                    status: "failed".to_string(),
                    detail: "该 route 标记 textOnly，但转换后的 chat 请求仍包含 image_url。"
                        .to_string(),
                });
            } else {
                steps.push(ProxyRouteTestStep {
                    id: "text_only_guard".to_string(),
                    label: "Text-only guard".to_string(),
                    status: "passed".to_string(),
                    detail: "截图输入已降级为文本占位，没有向 chat 上游发送 image_url。"
                        .to_string(),
                });
            }
        } else {
            let detail = if route.vision_bridge.is_some() {
                "该 route 标记 textOnly 且配置了 vision bridge，但 wire_api=responses passthrough 不会进入 Chat fallback；若上游不是原生 Responses 视觉模型，请改成 chat。"
            } else {
                "该 route 标记 textOnly，但 wire_api=responses 会 passthrough；若上游不是原生 Responses 视觉模型，请改成 chat 或配置 vision bridge。"
            };
            warnings.push(detail.to_string());
            steps.push(ProxyRouteTestStep {
                id: "text_only_guard".to_string(),
                label: "Text-only guard".to_string(),
                status: "warning".to_string(),
                detail: "responses passthrough 不会在 dry-run 中移除 input_image。".to_string(),
            });
        }
    } else if include_image && used_chat_fallback && image_part_sent_to_chat {
        steps.push(ProxyRouteTestStep {
            id: "text_only_guard".to_string(),
            label: "Image passthrough".to_string(),
            status: "passed".to_string(),
            detail: "该 route 未声明 textOnly，chat 请求保留 image_url。".to_string(),
        });
    } else {
        steps.push(ProxyRouteTestStep {
            id: "text_only_guard".to_string(),
            label: "Input modality".to_string(),
            status: "skipped".to_string(),
            detail: "本次 dry-run 未包含需要降级的图片输入。".to_string(),
        });
    }

    if !perform_upstream {
        steps.push(ProxyRouteTestStep {
            id: "upstream_request".to_string(),
            label: "Upstream request".to_string(),
            status: "skipped".to_string(),
            detail: "默认不请求真实上游，避免消耗 API 额度或触发工具调用。".to_string(),
        });
    }

    Ok(ProxyRouteTestResult {
        status: if failed { "failed" } else { "passed" }.to_string(),
        model_id: model_id.to_string(),
        provider_name: Some(route.provider_name.clone()),
        upstream_model: Some(route.model_id.clone()),
        upstream_base_url: Some(route.upstream_base_url.clone()),
        wire_api: Some(route.wire_api.clone()),
        auth_source: route.auth_source.clone(),
        text_only: route.text_only,
        used_chat_fallback,
        image_part_sent_to_chat,
        upstream_status_code: None,
        upstream_latency_ms: None,
        chat_request_preview,
        steps,
        warnings,
    })
}

async fn apply_upstream_route_test(
    result: &mut ProxyRouteTestResult,
    request: &ProxyRouteTestRequest,
    route: &ResolvedRoute,
) -> AppResult<()> {
    if route.auth_source.as_deref() == Some("managed_codex_oauth") {
        result.warnings.push(
            "官方订阅 route 需要 Codex Desktop 当前请求里的 ChatGPT 登录上下文，Codex Box 不能单独代打真实上游。"
                .to_string(),
        );
        result.steps.push(ProxyRouteTestStep {
            id: "upstream_request".to_string(),
            label: "Upstream request".to_string(),
            status: "skipped".to_string(),
            detail: "managed_codex_oauth route 已跳过真实上游测试。".to_string(),
        });
        return Ok(());
    }

    if let Some(env_key) = route.env_key.as_deref().filter(|value| !value.is_empty()) {
        if std::env::var(env_key).is_err() && route.api_key.is_none() {
            result.status = "failed".to_string();
            result.steps.push(ProxyRouteTestStep {
                id: "upstream_request".to_string(),
                label: "Upstream request".to_string(),
                status: "failed".to_string(),
                detail: format!("环境变量 {env_key} 未设置，无法进行真实上游测试。"),
            });
            return Ok(());
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Proxy(format!("route test client build: {e}")))?;
    let mut message = build_route_test_response_create(
        request.model_id.trim(),
        request.prompt.as_deref().unwrap_or("Codex Box route test"),
        request.include_image.unwrap_or(true),
    );
    if request.include_image.unwrap_or(true) && route.vision_bridge.is_some() {
        let bridged = apply_vision_bridge(&client, route, &mut message).await;
        if bridged > 0 {
            let mode = if route_uses_chat_fallback(route) {
                "chat/completions"
            } else {
                "responses"
            };
            result.steps.push(ProxyRouteTestStep {
                id: "vision_bridge".to_string(),
                label: "Vision bridge".to_string(),
                status: "passed".to_string(),
                detail: format!(
                    "真实上游测试已先处理 {bridged} 个 input_image，再发给目标 {mode} 上游。"
                ),
            });
        } else {
            result.steps.push(ProxyRouteTestStep {
                id: "vision_bridge".to_string(),
                label: "Vision bridge".to_string(),
                status: "warning".to_string(),
                detail: "已配置 vision bridge，但本次测试请求没有找到可替换的图片输入。"
                    .to_string(),
            });
        }
    }

    let (url, body, mode) = build_upstream_route_test_request_from_message(route, message)?;
    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body);

    let mut injected = HeaderMap::new();
    inject_auth_headers(route, &mut injected)?;
    for (name, value) in injected.iter() {
        req = req.header(name.as_str(), value.as_bytes());
    }

    let started = std::time::Instant::now();
    let response = req
        .send()
        .await
        .map_err(|e| AppError::Proxy(format!("真实上游测试发送失败: {e}")))?;
    let status = response.status();
    let latency_ms = started.elapsed().as_millis() as u64;
    result.upstream_status_code = Some(status.as_u16());
    result.upstream_latency_ms = Some(latency_ms);

    if status.is_success() {
        result.steps.push(ProxyRouteTestStep {
            id: "upstream_request".to_string(),
            label: "Upstream request".to_string(),
            status: "passed".to_string(),
            detail: format!(
                "{mode} 上游返回 {}，耗时 {}ms。响应正文未读取或展示。",
                status.as_u16(),
                latency_ms
            ),
        });
    } else {
        result.status = "failed".to_string();
        result.steps.push(ProxyRouteTestStep {
            id: "upstream_request".to_string(),
            label: "Upstream request".to_string(),
            status: "failed".to_string(),
            detail: format!(
                "{mode} 上游返回 {}，耗时 {}ms。响应正文未展示，避免泄露敏感信息。",
                status.as_u16(),
                latency_ms
            ),
        });
    }

    Ok(())
}

fn build_upstream_route_test_request(
    route: &ResolvedRoute,
    request: &ProxyRouteTestRequest,
) -> AppResult<(String, serde_json::Value, &'static str)> {
    let message = build_route_test_response_create(
        request.model_id.trim(),
        request.prompt.as_deref().unwrap_or("Codex Box route test"),
        request.include_image.unwrap_or(true),
    );

    build_upstream_route_test_request_from_message(route, message)
}

fn build_upstream_route_test_request_from_message(
    route: &ResolvedRoute,
    mut message: serde_json::Value,
) -> AppResult<(String, serde_json::Value, &'static str)> {
    if route_uses_chat_fallback(route) {
        let mut chat = responses_create_to_chat_request_with_options(
            &message,
            &route.model_id,
            ChatRequestOptions {
                chat_reasoning: route.chat_reasoning.as_ref(),
                text_only_input: route.text_only,
            },
        )?
        .body;
        chat["stream"] = serde_json::Value::Bool(false);
        if chat.get("max_tokens").is_none() {
            chat["max_tokens"] = serde_json::json!(16);
        }
        let url = build_upstream_url(&route.upstream_base_url, "/chat/completions")?;
        return Ok((url, chat, "chat/completions"));
    }

    message["model"] = serde_json::Value::String(route.model_id.clone());
    message["stream"] = serde_json::Value::Bool(false);
    if message.get("max_output_tokens").is_none() {
        message["max_output_tokens"] = serde_json::json!(16);
    }
    let url = build_upstream_url(&route.upstream_base_url, "/responses")?;
    Ok((url, message, "responses"))
}

fn route_uses_chat_fallback(route: &ResolvedRoute) -> bool {
    matches!(route.wire_api.as_str(), "chat" | "sse_stream" | "custom")
}

fn build_route_test_response_create(
    model_id: &str,
    prompt: &str,
    include_image: bool,
) -> serde_json::Value {
    let mut content = vec![serde_json::json!({
        "type": "input_text",
        "text": prompt
    })];
    if include_image {
        content.push(serde_json::json!({
            "type": "input_image",
            "image_url": "data:image/png;base64,codex-box-route-test"
        }));
    }
    serde_json::json!({
        "type": "response.create",
        "model": model_id,
        "stream": true,
        "reasoning": { "effort": "medium" },
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": content
            }
        ]
    })
}

fn contains_chat_image_url(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("type").and_then(|value| value.as_str()) == Some("image_url")
                || map.values().any(|value| contains_chat_image_url(value))
        }
        serde_json::Value::Array(values) => values.iter().any(contains_chat_image_url),
        _ => false,
    }
}

fn redact_route_test_preview(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(messages) = value
        .get_mut("messages")
        .and_then(|value| value.as_array_mut())
    {
        for message in messages {
            redact_large_data_urls(message);
        }
    }
    value
}

fn redact_large_data_urls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(url) = map.get_mut("url").and_then(|value| value.as_str()) {
                if url.starts_with("data:") {
                    map.insert(
                        "url".to_string(),
                        serde_json::Value::String("data:<redacted-route-test-image>".to_string()),
                    );
                }
            }
            for value in map.values_mut() {
                redact_large_data_urls(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_large_data_urls(value);
            }
        }
        _ => {}
    }
}

fn resolve_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(DEFAULT_CONFIG_PATH))
}

fn backup_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    let dir = home.join(BACKUP_DIR);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn short_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn current_inject_map_hash() -> AppResult<String> {
    let path = inject_map::inject_map_path()?;
    if !path.exists() {
        return Ok(String::new());
    }
    let raw = std::fs::read_to_string(path).map_err(AppError::Io)?;
    Ok(format!("sha256-{}", short_hash(&raw)))
}

fn validate_proxy_port(port: u16) -> AppResult<()> {
    if port == 0 {
        return Err(AppError::Command(
            "代理端口不能为 0,请先启动代理或使用默认端口 1455".to_string(),
        ));
    }
    Ok(())
}

fn build_runtime_logs(state: &ProxyState, view: ProxyStatusView) -> ProxyRuntimeLogs {
    let now = Utc::now().to_rfc3339();
    let model_count: usize = view.providers.iter().map(|p| p.models.len()).sum();
    let mut items = vec![ProxyRuntimeLogEntry {
        at: now.clone(),
        level: "info".to_string(),
        scope: "runtime".to_string(),
        message: format!("代理状态: {}", view.status),
    }];

    if view.port > 0 {
        items.push(ProxyRuntimeLogEntry {
            at: now.clone(),
            level: "info".to_string(),
            scope: "runtime".to_string(),
            message: format!("本地端口已配置: {}", view.port),
        });
    }

    let catalog_summary = current_codex_box_catalog_summary();

    if view.providers.is_empty() {
        items.push(ProxyRuntimeLogEntry {
            at: now.clone(),
            level: if catalog_summary.is_some() {
                "info"
            } else {
                "warn"
            }
            .to_string(),
            scope: "routes".to_string(),
            message: catalog_summary
                .clone()
                .unwrap_or_else(|| "尚未启用任何模型来源".to_string()),
        });
    } else {
        items.push(ProxyRuntimeLogEntry {
            at: now.clone(),
            level: "info".to_string(),
            scope: "routes".to_string(),
            message: format!(
                "已加载 {} 个模型来源, {} 个可路由模型",
                view.provider_count, model_count
            ),
        });
    }

    if let Some(err) = view.last_error.as_deref().filter(|s| !s.trim().is_empty()) {
        items.push(ProxyRuntimeLogEntry {
            at: now.clone(),
            level: "error".to_string(),
            scope: "runtime".to_string(),
            message: redact_runtime_message(err),
        });
    }

    items.push(ProxyRuntimeLogEntry {
        at: now,
        level: "info".to_string(),
        scope: "security".to_string(),
        message: "日志已脱敏,不会输出 API Key 或请求密钥".to_string(),
    });

    if let Some(config_line) = current_codex_config_log_line() {
        items.push(ProxyRuntimeLogEntry {
            at: Utc::now().to_rfc3339(),
            level: "info".to_string(),
            scope: "config".to_string(),
            message: config_line,
        });
    }

    for event in state.runtime_events() {
        items.push(ProxyRuntimeLogEntry {
            at: event.at,
            level: event.level,
            scope: event.scope,
            message: event.message,
        });
    }

    if items.iter().any(|item| {
        item.scope == "request"
            && (item.message.contains("uses official managed Codex auth")
                || item.message.contains("native_openai_auth_unresolved"))
    }) {
        items.push(ProxyRuntimeLogEntry {
            at: Utc::now().to_rfc3339(),
            level: "warn".to_string(),
            scope: "diagnosis".to_string(),
            message: "检测到请求进入官方订阅登录态链路；如果 Codex App 显示 refresh token 错误，请先确认官方登录态是否可刷新。这类错误发生在第三方上游请求之前，不代表 MiniMax/DeepSeek 等 API 一定失败。".to_string(),
        });
    }

    ProxyRuntimeLogs {
        redacted: true,
        items,
    }
}

fn current_codex_config_log_line() -> Option<String> {
    let raw = std::fs::read_to_string(resolve_config_path().ok()?).ok()?;
    let value = raw.parse::<toml::Value>().ok()?;
    let table = value.as_table()?;
    let model = table
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("-");
    let provider = table
        .get("model_provider")
        .and_then(|value| value.as_str())
        .unwrap_or("-");
    let catalog = table
        .get("model_catalog_json")
        .and_then(|value| value.as_str())
        .unwrap_or("-");
    Some(format!(
        "当前 Codex config: model={model}, model_provider={provider}, model_catalog_json={catalog}"
    ))
}

fn current_codex_box_catalog_summary() -> Option<String> {
    let cfg = crate::commands::opencodex::opencodex_config_read().ok()?;
    let visible_models = cfg.catalog.iter().filter(|entry| entry.visible).count();
    Some(format!(
        "Codex Box catalog 已加载: {} 个 API 服务, {} 个下拉框模型",
        cfg.providers.len(),
        visible_models
    ))
}

fn build_sessions(view: ProxyStatusView) -> ProxySessionsView {
    if view.providers.is_empty() {
        return ProxySessionsView {
            active_session_id: Some("default".to_string()),
            sessions: vec![ProxySessionEntry {
                id: "default".to_string(),
                label: "默认会话".to_string(),
                status: if view.status == "running" {
                    "active".to_string()
                } else {
                    "idle".to_string()
                },
                provider_count: 0,
                model_count: 0,
                last_used_at: view.started_at,
            }],
        };
    }

    let active_session_id = view
        .providers
        .first()
        .map(|p| format!("provider:{}", p.name));
    let sessions = view
        .providers
        .iter()
        .enumerate()
        .map(|(index, provider)| ProxySessionEntry {
            id: format!("provider:{}", provider.name),
            label: provider.name.clone(),
            status: if index == 0 && view.status == "running" {
                "active".to_string()
            } else {
                "idle".to_string()
            },
            provider_count: 1,
            model_count: provider.models.len(),
            last_used_at: view.started_at.clone(),
        })
        .collect();

    ProxySessionsView {
        active_session_id,
        sessions,
    }
}

fn redact_runtime_message(message: &str) -> String {
    message
        .split_whitespace()
        .map(|part| {
            if part.starts_with("sk-")
                || part.contains("api_key=")
                || part.contains("Authorization:")
            {
                "[REDACTED]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 改写 raw text 里的 [model_providers.<name>].base_url(跳过 subscription),
/// 同时构造新的 inject_map
pub fn rewrite_base_urls(
    raw: &str,
    _config: &crate::config::model::CodexConfig,
    port: u16,
) -> AppResult<(String, InjectMap)> {
    validate_proxy_port(port)?;
    use toml::Value;
    let mut value: Value = toml::from_str(raw)?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| AppError::Command("config.toml 顶层不是 table".to_string()))?;

    let mut inject_map = InjectMap {
        updated_at: chrono::Utc::now().to_rfc3339(),
        port,
        providers: Vec::new(),
    };
    let new_base = format!("http://127.0.0.1:{port}/v1");

    if let Some(Value::Table(mp)) = table.get_mut("model_providers") {
        for (name, entry_value) in mp.iter_mut() {
            let Value::Table(entry) = entry_value else {
                continue;
            };

            // 跳过订阅类
            let kind = entry
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("compatible_api")
                .to_string();
            if kind == "openai_subscription" || kind == "subscription" {
                continue;
            }

            let original_base_url = entry
                .get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if original_base_url.is_empty() {
                continue;
            }
            if is_legacy_opencodex_proxy_route(name, &original_base_url) {
                continue;
            }

            // 收集原配置
            let env_key = entry
                .get("api_key_env")
                .and_then(|v| v.as_str())
                .map(String::from);
            let http_headers = entry
                .get("http_headers")
                .and_then(|v| v.as_table())
                .map(|t| {
                    t.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect::<std::collections::BTreeMap<String, String>>()
                })
                .unwrap_or_default();
            let wire_api = entry
                .get("wire_api")
                .and_then(|v| v.as_str())
                .unwrap_or("chat")
                .to_string();
            let models: Vec<String> = entry
                .get("models")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            inject_map.providers.push(InjectMapEntry {
                name: name.clone(),
                original_base_url: original_base_url.clone(),
                env_key,
                http_headers,
                wire_api,
                models,
                kind: kind.clone(),
                extra: Default::default(),
            });

            // 重写 base_url
            entry.insert("base_url".to_string(), Value::String(new_base.clone()));
        }
    }

    let new_text = toml::to_string_pretty(&value)
        .map_err(|e| AppError::Command(format!("serialize config: {e}")))?;
    Ok((new_text, inject_map))
}

fn is_legacy_opencodex_proxy_route(name: &str, original_base_url: &str) -> bool {
    name.eq_ignore_ascii_case("opencodex") || original_base_url.contains("127.0.0.1:8765")
}

/// 从 inject-map 还原 raw text 的 base_url(用于 Stop 后)
pub fn restore_base_urls(raw: &str, entries: &[InjectMapEntry]) -> String {
    use toml::Value;
    let mut value: Value = match toml::from_str(raw) {
        Ok(v) => v,
        Err(_) => return raw.to_string(),
    };
    if let Some(table) = value.as_table_mut() {
        if let Some(Value::Table(mp)) = table.get_mut("model_providers") {
            for entry in entries {
                if let Some(Value::Table(t)) = mp.get_mut(&entry.name) {
                    t.insert(
                        "base_url".to_string(),
                        Value::String(entry.original_base_url.clone()),
                    );
                }
            }
        }
    }
    toml::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::state::ProxyRouteEntry;

    fn sample_view() -> ProxyStatusView {
        ProxyStatusView {
            status: "running".to_string(),
            port: 1455,
            started_at: "2026-06-25T12:00:00Z".to_string(),
            uptime_ms: Some(1000),
            last_error: Some("upstream failed sk-secret-token Authorization: Bearer x".to_string()),
            provider_count: 1,
            providers: vec![ProxyRouteEntry {
                name: "deepseek".to_string(),
                original_base_url: "https://api.deepseek.com/v1".to_string(),
                env_key: Some("DEEPSEEK_API_KEY".to_string()),
                wire_api: "chat".to_string(),
                kind: "compatible_api".to_string(),
                models: vec!["deepseek-chat".to_string(), "deepseek-reasoner".to_string()],
            }],
        }
    }

    #[test]
    fn runtime_logs_are_redacted_and_derived_from_status() {
        let state = ProxyState::new();
        let logs = build_runtime_logs(&state, sample_view());
        let text = logs
            .items
            .iter()
            .map(|item| item.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(logs.redacted);
        assert!(text.contains("已加载 1 个模型来源, 2 个可路由模型"));
        assert!(!text.contains("sk-secret-token"));
        assert!(!text.contains("Authorization:"));
    }

    #[test]
    fn sessions_are_derived_from_routed_providers() {
        let sessions = build_sessions(sample_view());

        assert_eq!(
            sessions.active_session_id.as_deref(),
            Some("provider:deepseek")
        );
        assert_eq!(sessions.sessions.len(), 1);
        assert_eq!(sessions.sessions[0].label, "deepseek");
        assert_eq!(sessions.sessions[0].status, "active");
        assert_eq!(sessions.sessions[0].model_count, 2);
    }

    #[test]
    fn rewrite_base_urls_rejects_zero_port() {
        let raw = r#"
model = "gpt-5.5"

[model_providers.opencodex]
name = "OpenCodex"
base_url = "http://127.0.0.1:8765/v1"
wire_api = "responses"
"#;
        let config = cfg_parser::parse(raw).unwrap();

        let error = rewrite_base_urls(raw, &config, 0).unwrap_err();

        assert!(
            error.to_string().contains("端口") && error.to_string().contains("0"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rewrite_base_urls_skips_legacy_opencodex_provider() {
        let raw = r#"
model = "minimax"

[model_providers.opencodex]
name = "OpenCodex"
base_url = "http://127.0.0.1:8765/v1"
wire_api = "responses"

[model_providers.codex_local_access]
name = "Codex API Service"
base_url = "https://api.minimaxi.com/v1"
wire_api = "chat"
"#;
        let config = cfg_parser::parse(raw).unwrap();

        let (_next, map) = rewrite_base_urls(raw, &config, 1455).unwrap();

        assert_eq!(map.providers.len(), 1);
        assert_eq!(map.providers[0].name, "codex_local_access");
        assert!(!map
            .providers
            .iter()
            .any(|provider| provider.name == "opencodex"));
    }

    #[test]
    fn route_test_replaces_text_only_minimax_image_with_marker() {
        let map = InjectMap {
            updated_at: "2026-06-27T00:00:00Z".to_string(),
            port: 1455,
            providers: vec![InjectMapEntry {
                name: "minimax".to_string(),
                original_base_url: "https://api.minimaxi.com/v1".to_string(),
                env_key: Some("MINIMAX_API_KEY".to_string()),
                http_headers: Default::default(),
                wire_api: "chat".to_string(),
                models: vec!["minimax-route-test-unit".to_string()],
                kind: "compatible_api".to_string(),
                extra: std::collections::BTreeMap::from([
                    ("textOnly".to_string(), serde_json::json!(true)),
                    (
                        "codexChatReasoning".to_string(),
                        serde_json::json!({
                            "supportsThinking": true,
                            "thinkingParam": "reasoning_split",
                            "outputFormat": "reasoning_details"
                        }),
                    ),
                ]),
            }],
        };
        let result = build_proxy_route_test_result(
            &ProxyRouteTestRequest {
                model_id: "minimax-route-test-unit".to_string(),
                prompt: Some("看图".to_string()),
                include_image: Some(true),
                perform_upstream: Some(false),
            },
            &map,
        )
        .unwrap();

        assert_eq!(result.status, "passed");
        assert_eq!(result.provider_name.as_deref(), Some("minimax"));
        assert!(result.text_only);
        assert!(result.used_chat_fallback);
        assert!(!result.image_part_sent_to_chat);
        assert!(result
            .steps
            .iter()
            .any(|step| step.id == "text_only_guard" && step.status == "passed"));
    }

    #[test]
    fn route_test_simulates_vision_bridge_without_upstream_call() {
        let map = InjectMap {
            updated_at: "2026-06-27T00:00:00Z".to_string(),
            port: 1455,
            providers: vec![InjectMapEntry {
                name: "minimax".to_string(),
                original_base_url: "https://api.minimaxi.com/v1".to_string(),
                env_key: Some("MINIMAX_API_KEY".to_string()),
                http_headers: Default::default(),
                wire_api: "chat".to_string(),
                models: vec!["minimax-vision-bridge-unit".to_string()],
                kind: "compatible_api".to_string(),
                extra: std::collections::BTreeMap::from([(
                    "visionBridge".to_string(),
                    serde_json::json!({
                        "enabled": true,
                        "baseUrl": "http://127.0.0.1:65535/v1",
                        "model": "vision-test"
                    }),
                )]),
            }],
        };
        let result = build_proxy_route_test_result(
            &ProxyRouteTestRequest {
                model_id: "minimax-vision-bridge-unit".to_string(),
                prompt: Some("看图".to_string()),
                include_image: Some(true),
                perform_upstream: Some(false),
            },
            &map,
        )
        .unwrap();
        let preview_text = serde_json::to_string(&result.chat_request_preview).unwrap();

        assert_eq!(result.status, "passed");
        assert!(result.used_chat_fallback);
        assert!(!result.image_part_sent_to_chat);
        assert!(preview_text.contains("截图描述"));
        assert!(!preview_text.contains("image_url"));
        assert!(result
            .steps
            .iter()
            .any(|step| step.id == "vision_bridge" && step.status == "passed"));
    }

    #[test]
    fn upstream_route_test_chat_body_is_small_and_non_streaming() {
        let route = ResolvedRoute {
            provider_name: "minimax".to_string(),
            model_id: "MiniMax-M3".to_string(),
            upstream_base_url: "https://api.minimaxi.com/v1".to_string(),
            wire_api: "chat".to_string(),
            auth_source: None,
            env_key: Some("MINIMAX_API_KEY".to_string()),
            api_key: None,
            http_headers: Default::default(),
            chat_reasoning: None,
            text_only: true,
            vision_bridge: None,
        };
        let (_url, body, mode) = build_upstream_route_test_request(
            &route,
            &ProxyRouteTestRequest {
                model_id: "minimax-route-test-unit".to_string(),
                prompt: Some("Reply OK".to_string()),
                include_image: Some(true),
                perform_upstream: Some(true),
            },
        )
        .unwrap();

        assert_eq!(mode, "chat/completions");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 16);
        assert!(!contains_chat_image_url(&body));
    }

    #[test]
    fn upstream_route_test_chat_body_accepts_vision_bridge_marker() {
        let route = ResolvedRoute {
            provider_name: "minimax".to_string(),
            model_id: "MiniMax-M3".to_string(),
            upstream_base_url: "https://api.minimaxi.com/v1".to_string(),
            wire_api: "chat".to_string(),
            auth_source: None,
            env_key: Some("MINIMAX_API_KEY".to_string()),
            api_key: None,
            http_headers: Default::default(),
            chat_reasoning: None,
            text_only: false,
            vision_bridge: Some(crate::proxy::routing::VisionBridgeConfig {
                base_url: "http://127.0.0.1:65535/v1".to_string(),
                model: "vision-test".to_string(),
                env_key: None,
            }),
        };
        let mut message = build_route_test_response_create("minimax-m3", "看图", true);
        let replaced = replace_images_with_vision_bridge_marker(&mut message, "截图里有 502 错误");
        let (_url, body, mode) =
            build_upstream_route_test_request_from_message(&route, message).unwrap();
        let body_text = serde_json::to_string(&body).unwrap();

        assert_eq!(replaced, 1);
        assert_eq!(mode, "chat/completions");
        assert!(!contains_chat_image_url(&body));
        assert!(body_text.contains("截图里有 502 错误"));
    }

    #[test]
    fn upstream_route_test_responses_body_accepts_vision_bridge_marker() {
        let route = ResolvedRoute {
            provider_name: "minimax".to_string(),
            model_id: "MiniMax-M3".to_string(),
            upstream_base_url: "https://api.minimaxi.com/v1".to_string(),
            wire_api: "responses".to_string(),
            auth_source: None,
            env_key: Some("MINIMAX_API_KEY".to_string()),
            api_key: None,
            http_headers: Default::default(),
            chat_reasoning: None,
            text_only: false,
            vision_bridge: Some(crate::proxy::routing::VisionBridgeConfig {
                base_url: "http://127.0.0.1:65535/v1".to_string(),
                model: "vision-test".to_string(),
                env_key: None,
            }),
        };
        let mut message = build_route_test_response_create("minimax-m3", "看图", true);
        let replaced = replace_images_with_vision_bridge_marker(&mut message, "截图里有 502 错误");
        let (_url, body, mode) =
            build_upstream_route_test_request_from_message(&route, message).unwrap();
        let body_text = serde_json::to_string(&body).unwrap();

        assert_eq!(replaced, 1);
        assert_eq!(mode, "responses");
        assert!(!body_text.contains("input_image"));
        assert!(body_text.contains("截图里有 502 错误"));
    }

    #[tokio::test]
    async fn upstream_route_test_skips_managed_codex_oauth() {
        let route = ResolvedRoute {
            provider_name: "openai-official".to_string(),
            model_id: "gpt-5.5".to_string(),
            upstream_base_url: "https://chatgpt.com/backend-api/codex".to_string(),
            wire_api: "responses".to_string(),
            auth_source: Some("managed_codex_oauth".to_string()),
            env_key: None,
            api_key: None,
            http_headers: Default::default(),
            chat_reasoning: None,
            text_only: false,
            vision_bridge: None,
        };
        let mut result = ProxyRouteTestResult {
            status: "passed".to_string(),
            model_id: "gpt-5.5".to_string(),
            provider_name: Some("openai-official".to_string()),
            upstream_model: Some("gpt-5.5".to_string()),
            upstream_base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
            wire_api: Some("responses".to_string()),
            auth_source: Some("managed_codex_oauth".to_string()),
            text_only: false,
            used_chat_fallback: false,
            image_part_sent_to_chat: false,
            upstream_status_code: None,
            upstream_latency_ms: None,
            chat_request_preview: None,
            steps: Vec::new(),
            warnings: Vec::new(),
        };

        apply_upstream_route_test(
            &mut result,
            &ProxyRouteTestRequest {
                model_id: "gpt-5.5".to_string(),
                prompt: None,
                include_image: Some(false),
                perform_upstream: Some(true),
            },
            &route,
        )
        .await
        .unwrap();

        assert_eq!(result.status, "passed");
        assert_eq!(result.upstream_status_code, None);
        assert!(result
            .steps
            .iter()
            .any(|step| step.id == "upstream_request" && step.status == "skipped"));
    }
}
