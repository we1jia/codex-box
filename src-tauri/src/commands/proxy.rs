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
use crate::proxy::state::{persist_runtime_state, ProxyState, ProxyStatus, ProxyStatusView};
use crate::proxy::DEFAULT_PROXY_PORT;
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

#[tauri::command]
pub fn proxy_status(state: tauri::State<Arc<ProxyState>>) -> ProxyStatusView {
    state.inner().to_view()
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

/// 改写 raw text 里的 [model_providers.<name>].base_url(跳过 subscription),
/// 同时构造新的 inject_map
pub fn rewrite_base_urls(
    raw: &str,
    _config: &crate::config::model::CodexConfig,
    port: u16,
) -> AppResult<(String, InjectMap)> {
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
