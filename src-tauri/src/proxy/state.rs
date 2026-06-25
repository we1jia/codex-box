// src-tauri/src/proxy/state.rs
//
// Codex Box 代理运行时状态: 内存中的 inject-map 副本 + 端口 + 启动时间。
//
// 由 Tauri Builder 持有一个 Arc<ProxyState>,start/stop/路由全部共享。
use crate::proxy::inject_map::InjectMap;
use chrono::Utc;
use serde::Serialize;
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
    /// 关闭信号发送端(单 sender,多 receiver)
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// 关闭信号接收端
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl ProxyState {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            inner: Arc::new(ProxyStateInner {
                inject_map: RwLock::new(InjectMap::default()),
                status: RwLock::new(ProxyStatus::Stopped),
                port: RwLock::new(0),
                started_at: RwLock::new(String::new()),
                last_error: RwLock::new(None),
                shutdown_tx: tx,
                shutdown_rx: rx,
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

    /// 触发优雅关闭信号(代理 serve task 收到后会退出 accept 循环)
    pub fn signal_shutdown(&self) {
        let _ = self.inner.shutdown_tx.send(true);
    }

    /// 当前是否已触发关闭
    pub fn is_shutdown_signaled(&self) -> bool {
        *self.inner.shutdown_rx.borrow()
    }

    /// 克隆 shutdown receiver(供 serve task 监听)
    pub fn shutdown_receiver(&self) -> tokio::sync::watch::Receiver<bool> {
        self.inner.shutdown_rx.clone()
    }

    /// 转成前端可见的视图
    pub fn to_view(&self) -> ProxyStatusView {
        let map = self.inject_map();
        ProxyStatusView {
            status: self.status().as_str().to_string(),
            port: self.port(),
            started_at: self.started_at(),
            uptime_ms: compute_uptime_ms(&self.started_at()),
            last_error: self.last_error(),
            provider_count: map.providers.len(),
            providers: map
                .providers
                .into_iter()
                .map(|p| ProxyRouteEntry {
                    name: p.name,
                    original_base_url: p.original_base_url,
                    env_key: p.env_key,
                    wire_api: p.wire_api,
                    kind: p.kind,
                    models: p.models,
                })
                .collect(),
        }
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
