// src-tauri/src/proxy/state.rs
//
// Codex Box 代理运行时状态: 内存中的 inject-map 副本 + 端口 + 启动时间。
//
// 由 Tauri Builder 持有一个 Arc<ProxyState>,start/stop/路由全部共享。
use crate::proxy::inject_map::InjectMap;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// 代理运行状态(对前端可见)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyStatus {
    Stopped,
    Starting,
    Running,
    Failed,
}

impl ProxyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProxyStatus::Stopped => "stopped",
            ProxyStatus::Starting => "starting",
            ProxyStatus::Running => "running",
            ProxyStatus::Failed => "failed",
        }
    }
}

impl Default for ProxyStatus {
    fn default() -> Self {
        ProxyStatus::Stopped
    }
}

/// 代理状态(线程安全)
#[derive(Clone)]
pub struct ProxyState {
    inner: Arc<ProxyStateInner>,
}

struct ProxyStateInner {
    /// inject-map 内存副本
    inject_map: RwLock<InjectMap>,
    /// 当前状态
    status: RwLock<ProxyStatus>,
    /// 监听端口
    port: RwLock<u16>,
    /// 启动时间 ISO 8601
    started_at: RwLock<String>,
    /// 失败时最近错误
    last_error: RwLock<Option<String>>,
    /// 关闭信号发送端。每次 start 都会重建 channel，避免 restart 继承旧 shutdown 信号。
    shutdown_tx: RwLock<tokio::sync::watch::Sender<bool>>,
    /// Chat fallback 的会话历史。Codex Desktop 对第三方模型不总是发送完整历史，
    /// 这里对齐 OpenCodex 的 customConversationHistory 行为。
    conversation_history: RwLock<BTreeMap<String, Vec<serde_json::Value>>>,
    /// 运行期诊断日志。用于判断 Codex App 请求是否真的进入本地代理，以及命中了哪条上游链路。
    runtime_events: RwLock<Vec<ProxyRuntimeEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeEvent {
    pub at: String,
    pub level: String,
    pub scope: String,
    pub message: String,
}

impl ProxyState {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(false);
        Self {
            inner: Arc::new(ProxyStateInner {
                inject_map: RwLock::new(InjectMap::default()),
                status: RwLock::new(ProxyStatus::Stopped),
                port: RwLock::new(0),
                started_at: RwLock::new(String::new()),
                last_error: RwLock::new(None),
                shutdown_tx: RwLock::new(tx),
                conversation_history: RwLock::new(BTreeMap::new()),
                runtime_events: RwLock::new(Vec::new()),
            }),
        }
    }

    pub fn default() -> Self {
        Self::new()
    }

    /// 当前 inject-map(克隆)
    pub fn inject_map(&self) -> InjectMap {
        self.inner
            .inject_map
            .read()
            .expect("inject_map lock poisoned")
            .clone()
    }

    /// 替换 inject-map(用于写入后回填)
    pub fn set_inject_map(&self, map: InjectMap) {
        let mut guard = self
            .inner
            .inject_map
            .write()
            .expect("inject_map lock poisoned");
        *guard = map;
    }

    pub fn status(&self) -> ProxyStatus {
        *self.inner.status.read().expect("status lock poisoned")
    }

    pub fn set_status(&self, status: ProxyStatus) {
        *self.inner.status.write().expect("status lock poisoned") = status;
    }

    pub fn port(&self) -> u16 {
        *self.inner.port.read().expect("port lock poisoned")
    }

    pub fn set_port(&self, port: u16) {
        *self.inner.port.write().expect("port lock poisoned") = port;
    }

    pub fn started_at(&self) -> String {
        self.inner
            .started_at
            .read()
            .expect("started_at lock poisoned")
            .clone()
    }

    pub fn set_started_at(&self, iso: String) {
        *self
            .inner
            .started_at
            .write()
            .expect("started_at lock poisoned") = iso;
    }

    pub fn last_error(&self) -> Option<String> {
        self.inner
            .last_error
            .read()
            .expect("last_error lock poisoned")
            .clone()
    }

    pub fn set_last_error(&self, err: Option<String>) {
        *self
            .inner
            .last_error
            .write()
            .expect("last_error lock poisoned") = err;
    }

    /// 触发当前启动周期的优雅关闭信号。
    pub fn signal_shutdown(&self) {
        if let Ok(sender) = self.inner.shutdown_tx.read() {
            let _ = sender.send(true);
        }
    }

    /// 为一次新的 start 创建独立 shutdown receiver，避免复用旧的 true 信号。
    pub fn fresh_shutdown_receiver(&self) -> tokio::sync::watch::Receiver<bool> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        *self
            .inner
            .shutdown_tx
            .write()
            .expect("shutdown_tx lock poisoned") = tx;
        rx
    }

    /// 转成前端可见的视图
    pub fn to_view(&self) -> ProxyStatusView {
        let map = self.inject_map();
        let providers: Vec<ProxyRouteEntry> = map
            .providers
            .into_iter()
            .filter(|p| !is_legacy_opencodex_proxy_route(&p.name, &p.original_base_url))
            .map(|p| ProxyRouteEntry {
                name: p.name,
                original_base_url: p.original_base_url,
                env_key: p.env_key,
                wire_api: p.wire_api,
                kind: p.kind,
                models: p.models,
            })
            .collect();
        ProxyStatusView {
            status: self.status().as_str().to_string(),
            port: self.port(),
            started_at: self.started_at(),
            uptime_ms: compute_uptime_ms(&self.started_at()),
            last_error: self.last_error(),
            provider_count: providers.len(),
            providers,
        }
    }

    pub fn merge_conversation_history(
        &self,
        session_id: &str,
        incoming: Vec<serde_json::Value>,
        reset: bool,
    ) -> Vec<serde_json::Value> {
        let mut guard = self
            .inner
            .conversation_history
            .write()
            .expect("conversation_history lock poisoned");
        if reset || guard.get(session_id).map(|h| h.is_empty()).unwrap_or(true) {
            guard.insert(session_id.to_string(), incoming.clone());
            return sanitize_history_messages(incoming);
        }

        let existing = guard.get(session_id).cloned().unwrap_or_default();
        let incoming_without_system = incoming
            .into_iter()
            .filter(|message| {
                message.get("role").and_then(|value| value.as_str()) != Some("system")
            })
            .collect::<Vec<_>>();
        let merged = merge_history(existing, incoming_without_system);
        guard.insert(session_id.to_string(), merged.clone());
        sanitize_history_messages(merged)
    }

    pub fn append_conversation_message(&self, session_id: &str, message: serde_json::Value) {
        let mut guard = self
            .inner
            .conversation_history
            .write()
            .expect("conversation_history lock poisoned");
        guard
            .entry(session_id.to_string())
            .or_default()
            .push(message);
    }

    pub fn log_event(&self, level: &str, scope: &str, message: impl Into<String>) {
        let event = ProxyRuntimeEvent {
            at: Utc::now().to_rfc3339(),
            level: level.to_string(),
            scope: scope.to_string(),
            message: redact_runtime_event_message(&message.into()),
        };
        {
            let mut guard = self
                .inner
                .runtime_events
                .write()
                .expect("runtime_events lock poisoned");
            guard.push(event.clone());
            if guard.len() > 300 {
                let overflow = guard.len() - 300;
                guard.drain(0..overflow);
            }
        }
        append_runtime_event_to_file(&event);
    }

    pub fn runtime_events(&self) -> Vec<ProxyRuntimeEvent> {
        self.inner
            .runtime_events
            .read()
            .expect("runtime_events lock poisoned")
            .clone()
    }
}

impl Default for ProxyState {
    fn default() -> Self {
        Self::new()
    }
}

/// 持久化的运行时状态写回
pub fn persist_runtime_state(state: &ProxyState) {
    use crate::proxy::inject_map::{write_runtime_state, RuntimeState};
    let runtime_state = RuntimeState {
        status: state.status().as_str().to_string(),
        port: state.port(),
        started_at: state.started_at(),
        last_error: state.last_error(),
        provider_count: state.inject_map().providers.len(),
    };
    if let Err(e) = write_runtime_state(&runtime_state) {
        tracing::warn!("write runtime-state failed: {e}");
    }
}

fn compute_uptime_ms(started_at: &str) -> Option<u64> {
    if started_at.is_empty() {
        return None;
    }
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let now = Utc::now();
    let delta = now.signed_duration_since(started.with_timezone(&Utc));
    Some(delta.num_milliseconds().max(0) as u64)
}

fn is_legacy_opencodex_proxy_route(name: &str, original_base_url: &str) -> bool {
    name.eq_ignore_ascii_case("opencodex") || original_base_url.contains("127.0.0.1:8765")
}

fn runtime_events_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".codex/codex-box/logs/runtime-events.jsonl"))
}

fn append_runtime_event_to_file(event: &ProxyRuntimeEvent) {
    let Some(path) = runtime_events_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn redact_runtime_event_message(message: &str) -> String {
    let mut redacted = message.to_string();
    for marker in ["sk-", "Bearer "] {
        while let Some(start) = redacted.find(marker) {
            let tail = &redacted[start..];
            let end = tail
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                .unwrap_or(tail.len());
            redacted.replace_range(start..start + end, "<redacted>");
        }
    }
    redacted
}

fn merge_history(
    mut existing: Vec<serde_json::Value>,
    incoming: Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    let mut overlap = 0;
    let max = existing.len().min(incoming.len());
    for len in 1..=max {
        let start = existing.len() - len;
        let mut matches = true;
        for i in 0..len {
            if !same_message(&existing[start + i], &incoming[i]) {
                matches = false;
                break;
            }
        }
        if matches {
            overlap = len;
        }
    }
    existing.extend(incoming.into_iter().skip(overlap));
    existing
}

fn same_message(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    if left.get("role") != right.get("role") {
        return false;
    }
    match left.get("role").and_then(|value| value.as_str()) {
        Some("tool") => {
            left.get("tool_call_id") == right.get("tool_call_id")
                && left.get("content") == right.get("content")
        }
        Some("assistant") => {
            left.get("content") == right.get("content")
                && left.get("tool_calls") == right.get("tool_calls")
        }
        _ => left.get("content") == right.get("content"),
    }
}

fn sanitize_history_messages(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    messages
        .into_iter()
        .map(|mut message| {
            if message.get("role").and_then(|value| value.as_str()) == Some("assistant")
                && message
                    .get("content")
                    .map(|value| value.is_null() || value.as_str() == Some(""))
                    .unwrap_or(true)
                && message.get("tool_calls").is_none()
            {
                if let Some(obj) = message.as_object_mut() {
                    obj.insert(
                        "content".to_string(),
                        serde_json::Value::String(" ".to_string()),
                    );
                }
            }
            message
        })
        .collect()
}

/// 前端可见的代理状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatusView {
    pub status: String,
    pub port: u16,
    pub started_at: String,
    pub uptime_ms: Option<u64>,
    pub last_error: Option<String>,
    pub provider_count: usize,
    pub providers: Vec<ProxyRouteEntry>,
}

/// 前端可见的路由表条目
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRouteEntry {
    pub name: String,
    pub original_base_url: String,
    pub env_key: Option<String>,
    pub wire_api: String,
    pub kind: String,
    pub models: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_history_merges_followup_without_repeating_system() {
        let state = ProxyState::new();
        let first = vec![
            serde_json::json!({ "role": "system", "content": "sys" }),
            serde_json::json!({ "role": "user", "content": "one" }),
        ];
        let merged_first = state.merge_conversation_history("s1", first, true);
        assert_eq!(merged_first.len(), 2);

        state.append_conversation_message(
            "s1",
            serde_json::json!({ "role": "assistant", "content": "answer one" }),
        );

        let second = vec![
            serde_json::json!({ "role": "system", "content": "sys" }),
            serde_json::json!({ "role": "user", "content": "two" }),
        ];
        let merged_second = state.merge_conversation_history("s1", second, false);

        assert_eq!(
            merged_second,
            vec![
                serde_json::json!({ "role": "system", "content": "sys" }),
                serde_json::json!({ "role": "user", "content": "one" }),
                serde_json::json!({ "role": "assistant", "content": "answer one" }),
                serde_json::json!({ "role": "user", "content": "two" }),
            ]
        );
    }

    #[test]
    fn conversation_history_uses_overlap_to_avoid_duplicate_messages() {
        let state = ProxyState::new();
        let first = vec![
            serde_json::json!({ "role": "user", "content": "one" }),
            serde_json::json!({ "role": "assistant", "content": "answer one" }),
        ];
        state.merge_conversation_history("s1", first, true);

        let second = vec![
            serde_json::json!({ "role": "assistant", "content": "answer one" }),
            serde_json::json!({ "role": "user", "content": "two" }),
        ];
        let merged = state.merge_conversation_history("s1", second, false);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[2]["content"], "two");
    }
}
