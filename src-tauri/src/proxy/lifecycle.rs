// src-tauri/src/proxy/lifecycle.rs
//
// 代理生命周期: start / stop / port probe
//
// 关键约束:
//   - 仅监听 127.0.0.1(绝不绑 LAN)
//   - port 冲突时 +1 探测,最多 PORT_PROBE_MAX 次
//   - start 返回后,代理在后台 tokio task 里跑
//   - stop 调 shutdown_tx,等 2s,强制 abort
use crate::error::{AppError, AppResult};
use crate::proxy::server::{build_router, ServerState};
use crate::proxy::state::{persist_runtime_state, ProxyState, ProxyStatus};
use chrono::Utc;
use reqwest::Client;
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

/// 启动失败原因(对外)
#[derive(Debug, Clone)]
pub enum ProxyError {
    PortInUse(u16),
    BindFailed(String),
    ServeFailed(String),
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::PortInUse(p) => write!(f, "port {p} is in use"),
            ProxyError::BindFailed(s) => write!(f, "bind failed: {s}"),
            ProxyError::ServeFailed(s) => write!(f, "serve failed: {s}"),
        }
    }
}

impl std::error::Error for ProxyError {}

/// 探测可用端口(start 调用前先调用)
pub async fn probe_port(start: u16) -> AppResult<u16> {
    use crate::proxy::PORT_PROBE_MAX;
    for offset in 0..PORT_PROBE_MAX {
        let port = start.saturating_add(offset as u16);
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .map_err(|e| AppError::Command(format!("invalid port: {e}")))?;
        match TcpListener::bind(addr).await {
            Ok(_listener) => {
                // 立刻 drop,让 start 时还能绑
                return Ok(port);
            }
            Err(_) => continue,
        }
    }
    Err(AppError::Command(format!(
        "no free port in [{start}, {}+{}); aborting",
        start, PORT_PROBE_MAX
    )))
}

/// 用真实 socket 判断本机端口是否可连接。
pub fn is_port_accepting(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    let Ok(addr) = format!("127.0.0.1:{port}").parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_ok()
}

/// 修正“状态显示 running，但端口没有 listener”的漂移。
pub fn reconcile_running_state(state: &ProxyState) {
    if state.status() != ProxyStatus::Running {
        return;
    }
    let port = state.port();
    if is_port_accepting(port) {
        return;
    }
    state.set_status(ProxyStatus::Failed);
    state.set_last_error(Some(format!(
        "proxy state was running, but 127.0.0.1:{port} is not accepting connections"
    )));
    persist_runtime_state(state);
}

/// 启动代理(后台 tokio task)
pub async fn start(state: Arc<ProxyState>, requested_port: u16) -> Result<u16, ProxyError> {
    reconcile_running_state(&state);
    let existing_port = state.port();
    if state.status() == ProxyStatus::Running && is_port_accepting(existing_port) {
        state.log_event(
            "info",
            "runtime",
            format!("proxy already running on 127.0.0.1:{existing_port}"),
        );
        return Ok(existing_port);
    }

    state.set_status(ProxyStatus::Starting);
    state.set_last_error(None);

    let port = match probe_port(requested_port).await {
        Ok(p) => p,
        Err(e) => {
            state.set_status(ProxyStatus::Failed);
            state.set_last_error(Some(e.to_string()));
            persist_runtime_state(&state);
            return Err(ProxyError::BindFailed(e.to_string()));
        }
    };

    let addr: SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(a) => a,
        Err(e) => {
            state.set_status(ProxyStatus::Failed);
            state.set_last_error(Some(format!("invalid addr: {e}")));
            persist_runtime_state(&state);
            return Err(ProxyError::BindFailed(format!("invalid addr: {e}")));
        }
    };

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            state.set_status(ProxyStatus::Failed);
            state.set_last_error(Some(format!("bind {addr}: {e}")));
            persist_runtime_state(&state);
            return Err(ProxyError::BindFailed(e.to_string()));
        }
    };

    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| Client::new());

    let server_state = ServerState {
        proxy: state.clone(),
        http,
    };

    let app = build_router(server_state);

    state.set_port(port);
    state.set_started_at(Utc::now().to_rfc3339());
    state.set_status(ProxyStatus::Running);
    state.log_event(
        "info",
        "runtime",
        format!(
            "proxy started on 127.0.0.1:{port}; inject-map entries={}; catalog routes are resolved from providers.json/custom_model_catalog.json",
            state.inject_map().providers.len()
        ),
    );
    persist_runtime_state(&state);

    let mut shutdown_rx = state.fresh_shutdown_receiver();

    // serve task
    tokio::spawn(async move {
        let serve = axum::serve(listener, app);
        let result = serve
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await;

        if let Err(e) = result {
            tracing::error!("proxy serve task failed: {e}");
        }
    });

    Ok(port)
}

/// 停止代理(发信号)
pub fn stop(state: &ProxyState) {
    state.signal_shutdown();
    state.set_status(ProxyStatus::Stopped);
    state.set_started_at(String::new());
    state.set_last_error(None);
    state.log_event("info", "runtime", "proxy stopped");
    persist_runtime_state(state);
}

/// 重启代理
pub async fn restart(state: Arc<ProxyState>, requested_port: u16) -> Result<u16, ProxyError> {
    stop(&state);
    // 等 500ms 让上一个 serve task 优雅退出
    tokio::time::sleep(Duration::from_millis(500)).await;
    start(state, requested_port).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::DEFAULT_PROXY_PORT;

    #[tokio::test]
    async fn probe_port_returns_a_free_port() {
        let p = probe_port(DEFAULT_PROXY_PORT).await.unwrap();
        assert!(p >= DEFAULT_PROXY_PORT);
    }
}
