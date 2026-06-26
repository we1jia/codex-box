// src-tauri/src/proxy/server.rs
//
// axum Router 装配: 注册 /v1/models, /v1/chat/completions, /v1/responses, /healthz
// 共享一个 reqwest::Client 实例(全局,避免每次请求新建连接池)
use crate::proxy::models_endpoint::merged_models;
use crate::proxy::responses_ws::{responses_create_to_chat_request, ResponsesWsStreamState};
use crate::proxy::routing::{resolve_catalog_route, resolve_route};
use crate::proxy::state::ProxyState;
use crate::proxy::upstream::{
    build_upstream_url, extract_model_id, forward_chat, forward_responses, inject_auth_headers,
};
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use futures::StreamExt;
use reqwest::Client;
use std::sync::Arc;

/// 共享状态(由 lifecycle 注入)
#[derive(Clone)]
pub struct ServerState {
    pub proxy: Arc<ProxyState>,
    pub http: Client,
}

pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/v1/models", get(handle_models))
        .route("/v1/chat/completions", post(handle_chat))
        .route(
            "/v1/responses",
            get(handle_responses_ws).post(handle_responses),
        )
        .route("/healthz", get(handle_health))
        .with_state(state)
}

async fn handle_models(State(state): State<ServerState>) -> impl IntoResponse {
    let map = state.proxy.inject_map();
    match merged_models(&map) {
        Ok(resp) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                "x-codex-box-source",
                axum::http::HeaderValue::from_static("merged"),
            );
            (StatusCode::OK, headers, axum::Json(resp)).into_response()
        }
        Err(e) => openai_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("merge failed: {e}"),
        ),
    }
}

async fn handle_health(State(state): State<ServerState>) -> impl IntoResponse {
    let view = state.proxy.to_view();
    axum::Json(serde_json::json!({
        "status": view.status,
        "port": view.port,
        "uptime_ms": view.uptime_ms,
        "provider_count": view.provider_count,
    }))
}

async fn handle_chat(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_dispatch(&state, &headers, &body, "chat").await
}

async fn handle_responses(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_dispatch(&state, &headers, &body, "responses").await
}

async fn handle_responses_ws(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_responses_socket(socket, state, headers))
}

async fn handle_responses_socket(mut socket: WebSocket, state: ServerState, headers: HeaderMap) {
    while let Some(next) = socket.next().await {
        let Ok(message) = next else {
            break;
        };
        match message {
            Message::Text(text) => {
                if let Err(err) =
                    handle_responses_ws_message(&mut socket, &state, &headers, text.as_bytes())
                        .await
                {
                    let _ =
                        send_ws_error(&mut socket, "ws_responses_error", &err.to_string()).await;
                }
            }
            Message::Binary(bytes) => {
                if let Err(err) =
                    handle_responses_ws_message(&mut socket, &state, &headers, &bytes).await
                {
                    let _ =
                        send_ws_error(&mut socket, "ws_responses_error", &err.to_string()).await;
                }
            }
            Message::Ping(bytes) => {
                let _ = socket.send(Message::Pong(bytes)).await;
            }
            Message::Close(_) => break,
            Message::Pong(_) => {}
        }
    }
}

async fn handle_responses_ws_message(
    socket: &mut WebSocket,
    state: &ServerState,
    headers: &HeaderMap,
    raw: &[u8],
) -> Result<(), String> {
    let message: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| format!("invalid websocket JSON: {e}"))?;

    if message.get("type").and_then(|v| v.as_str()) != Some("response.create") {
        return Ok(());
    }

    let model_id = message
        .get("model")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "response.create message must include a `model` string field".to_string())?;

    let map = state.proxy.inject_map();
    let route = resolve_route(model_id, &map)
        .or_else(|| resolve_catalog_route(model_id, &map))
        .ok_or_else(|| format!("model `{model_id}` is not in the Codex Box proxy route table"))?;

    let chat = responses_create_to_chat_request(&message, &route.model_id)
        .map_err(|e| format!("convert responses to chat failed: {e}"))?;
    let upstream_url = build_upstream_url(&route.upstream_base_url, "/chat/completions")
        .map_err(|e| e.to_string())?;

    let mut request = state
        .http
        .post(&upstream_url)
        .header("content-type", "application/json")
        .json(&chat.body);

    for (name, value) in headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if is_hop_by_hop_header(&lower) || lower == "authorization" {
            continue;
        }
        request = request.header(name.as_str(), value.as_bytes());
    }

    let mut injected = HeaderMap::new();
    inject_auth_headers(&route, &mut injected).map_err(|e| e.to_string())?;
    for (name, value) in injected.iter() {
        request = request.header(name.as_str(), value.as_bytes());
    }

    tracing::info!(
        provider = %route.provider_name,
        model = %route.model_id,
        "responses websocket routed to chat completions upstream"
    );

    let response = request
        .send()
        .await
        .map_err(|e| format!("upstream chat send failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "upstream chat returned status {}: {}",
            status.as_u16(),
            text
        ));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut stream_state = ResponsesWsStreamState::new(model_id);
    send_ws_json(socket, &stream_state.started_event())
        .await
        .map_err(|e| e.to_string())?;

    if content_type.contains("text/event-stream") {
        stream_chat_sse_to_ws(socket, response, &mut stream_state).await?;
    } else {
        let value = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("read upstream chat JSON failed: {e}"))?;
        let content = value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .unwrap_or(" ");
        let synthetic = serde_json::json!({
            "choices": [{ "delta": { "content": content } }]
        });
        let line = format!("data: {}", synthetic);
        for event in stream_state.ingest_sse_line(&line) {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
        for event in stream_state.finish_events() {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

async fn stream_chat_sse_to_ws(
    socket: &mut WebSocket,
    response: reqwest::Response,
    stream_state: &mut ResponsesWsStreamState,
) -> Result<(), String> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("upstream stream error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = buffer.find('\n') {
            let line = buffer[..index].trim_end_matches('\r').to_string();
            buffer = buffer[index + 1..].to_string();
            for event in stream_state.ingest_sse_line(&line) {
                send_ws_json(socket, &event)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    let tail = buffer.trim();
    if !tail.is_empty() {
        for event in stream_state.ingest_sse_line(tail) {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    if !stream_state.is_completed() {
        for event in stream_state.finish_events() {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn send_ws_json(
    socket: &mut WebSocket,
    value: &serde_json::Value,
) -> Result<(), axum::Error> {
    socket.send(Message::Text(value.to_string())).await
}

async fn send_ws_error(
    socket: &mut WebSocket,
    code: &str,
    message: &str,
) -> Result<(), axum::Error> {
    send_ws_json(
        socket,
        &serde_json::json!({
            "type": "error",
            "error": {
                "type": code,
                "code": code,
                "message": message,
            }
        }),
    )
    .await
}

async fn proxy_dispatch(
    state: &ServerState,
    headers: &HeaderMap,
    body: &Bytes,
    kind: &str,
) -> Response {
    let model_id = match extract_model_id(body) {
        Some(m) => m,
        None => {
            return openai_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "request body must include a `model` string field",
            );
        }
    };

    let map = state.proxy.inject_map();
    let route =
        match resolve_route(&model_id, &map).or_else(|| resolve_catalog_route(&model_id, &map)) {
            Some(r) => r,
            None => {
                return openai_error_response(
                    StatusCode::NOT_FOUND,
                    "model_not_found",
                    &format!("model `{model_id}` is not in the Codex Box proxy route table"),
                );
            }
        };

    // 如果 wire_api 与 endpoint 不匹配,返回 400
    let endpoint_kind = if kind == "chat" { "chat" } else { "responses" };
    if !wire_api_matches(&route.wire_api, endpoint_kind) {
        return openai_error_response(
            StatusCode::BAD_REQUEST,
            "wire_api_mismatch",
            &format!(
                "provider `{}` wire_api={} cannot be reached via /v1/{}",
                route.provider_name, route.wire_api, endpoint_kind
            ),
        );
    }

    let result = match endpoint_kind {
        "chat" => forward_chat(&state.http, &route, headers, body.clone()).await,
        "responses" => forward_responses(&state.http, &route, headers, body.clone()).await,
        _ => unreachable!(),
    };

    match result {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!(
                provider = %route.provider_name,
                model = %route.model_id,
                kind = %endpoint_kind,
                error = %e,
                "upstream forward failed"
            );
            openai_error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &format!("upstream call failed: {e}"),
            )
        }
    }
}

fn wire_api_matches(wire_api: &str, endpoint: &str) -> bool {
    match (wire_api, endpoint) {
        ("chat", "chat") => true,
        ("responses", "responses") => true,
        // custom / sse_stream 允许尝试匹配(用户自行负责)
        ("custom", _) => true,
        ("sse_stream", _) => true,
        _ => false,
    }
}

fn is_hop_by_hop_header(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "sec-websocket-key"
            | "sec-websocket-version"
            | "sec-websocket-extensions"
            | "sec-websocket-protocol"
    )
}

pub fn openai_error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": code,
            "code": code,
        }
    });
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::inject_map::{InjectMap, InjectMapEntry};
    use crate::proxy::upstream::build_reqwest_client;
    use futures::{SinkExt, StreamExt};
    use std::collections::BTreeMap;
    use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

    #[tokio::test]
    async fn responses_websocket_routes_custom_model_to_chat_sse_upstream() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream = Router::new().route("/v1/chat/completions", post(mock_chat_sse));
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap {
            updated_at: "2026-06-26T00:00:00Z".to_string(),
            port: 0,
            providers: vec![InjectMapEntry {
                name: "local".to_string(),
                original_base_url: format!("http://{upstream_addr}/v1"),
                env_key: None,
                http_headers: BTreeMap::new(),
                wire_api: "chat".to_string(),
                models: vec!["demo".to_string()],
                kind: "compatible_api".to_string(),
                extra: BTreeMap::new(),
            }],
        });

        let app = build_router(ServerState {
            proxy,
            http: build_reqwest_client(),
        });
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, app).await.unwrap();
        });

        let (mut ws, _) = connect_async(format!("ws://{proxy_addr}/v1/responses"))
            .await
            .unwrap();
        ws.send(TungsteniteMessage::Text(
            serde_json::json!({
                "type": "response.create",
                "model": "local/demo",
                "input": [
                    {
                        "type": "message",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": "ping" }]
                    }
                ],
                "stream": true
            })
            .to_string(),
        ))
        .await
        .unwrap();

        let mut saw_delta = false;
        let mut saw_completed = false;
        for _ in 0..10 {
            let Some(Ok(TungsteniteMessage::Text(text))) = ws.next().await else {
                continue;
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            match value.get("type").and_then(|value| value.as_str()) {
                Some("response.output_text.delta") => {
                    if value.get("delta").and_then(|value| value.as_str()) == Some("pong") {
                        saw_delta = true;
                    }
                }
                Some("response.completed") => {
                    saw_completed = true;
                    break;
                }
                Some("error") => panic!("unexpected ws error: {value}"),
                _ => {}
            }
        }

        assert!(
            saw_delta,
            "expected response.output_text.delta from upstream SSE"
        );
        assert!(
            saw_completed,
            "expected response.completed from upstream SSE"
        );
    }

    async fn mock_chat_sse() -> impl IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"choices\":[{\"delta\":{\"content\":\"pong\"}}]}\n\ndata: [DONE]\n\n",
        )
    }
}
