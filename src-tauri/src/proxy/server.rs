// src-tauri/src/proxy/server.rs
//
// axum Router 装配: 注册 /v1/models, /v1/chat/completions, /v1/responses, /healthz
// 共享一个 reqwest::Client 实例(全局,避免每次请求新建连接池)
use crate::error::AppError;
use crate::proxy::models_endpoint::merged_models;
use crate::proxy::responses_ws::{
    chat_completion_json_to_response_body, chat_completion_json_to_response_events,
    response_event_to_sse_bytes, response_events_to_sse,
    responses_create_to_chat_request_with_options, ChatRequestOptions,
    ResponseEventPassthroughNormalizer, ResponsesWsStreamState,
};
use crate::proxy::routing::is_native_openai_catalog_model;
use crate::proxy::routing::{resolve_catalog_route, resolve_route, ResolvedRoute};
use crate::proxy::state::ProxyState;
use crate::proxy::upstream::{
    build_upstream_url, extract_model_id, forward_chat, forward_responses, inject_auth_headers,
};
use crate::proxy::vision_bridge::apply_vision_bridge;
use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use futures::stream::BoxStream;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message as TungsteniteMessage},
};

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
    uri: Uri,
) -> impl IntoResponse {
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "/v1/responses".to_string());
    ws.on_upgrade(move |socket| handle_responses_socket(socket, state, headers, path_and_query))
}

async fn handle_responses_socket(
    mut socket: WebSocket,
    state: ServerState,
    headers: HeaderMap,
    path_and_query: String,
) {
    while let Some(next) = socket.next().await {
        let Ok(message) = next else {
            break;
        };
        match message {
            Message::Text(text) => {
                if let Err(err) = handle_responses_ws_message(
                    &mut socket,
                    &state,
                    &headers,
                    &path_and_query,
                    text.as_bytes(),
                )
                .await
                {
                    let _ =
                        send_ws_error(&mut socket, "ws_responses_error", &err.to_string()).await;
                }
            }
            Message::Binary(bytes) => {
                if let Err(err) = handle_responses_ws_message(
                    &mut socket,
                    &state,
                    &headers,
                    &path_and_query,
                    &bytes,
                )
                .await
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
    path_and_query: &str,
    raw: &[u8],
) -> Result<(), String> {
    let mut message: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| format!("invalid websocket JSON: {e}"))?;

    if message.get("type").and_then(|v| v.as_str()) != Some("response.create") {
        return Ok(());
    }

    let model_id = message
        .get("model")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "response.create message must include a `model` string field".to_string())?
        .to_string();

    let map = state.proxy.inject_map();
    let catalog_route = resolve_catalog_route(&model_id, &map);
    let inject_route = if catalog_route.is_none() {
        resolve_route(&model_id, &map)
    } else {
        None
    };
    let route_source = if catalog_route.is_some() {
        "catalog"
    } else if inject_route.is_some() {
        "inject-map"
    } else {
        "unresolved"
    };
    let route = catalog_route.or(inject_route);
    let Some(route) = route else {
        if is_native_openai_catalog_model(&model_id) {
            state.proxy.log_event(
                "warn",
                "request",
                format!(
                    "ws /v1/responses model={model_id} unresolved locally; fallback=native_openai"
                ),
            );
            return forward_native_openai_ws(socket, headers, path_and_query, raw).await;
        }
        state.proxy.log_event(
            "error",
            "request",
            format!("ws /v1/responses model={model_id} unresolved; returning model_not_found"),
        );
        return Err(format!(
            "model `{model_id}` is not in the Codex Box proxy route table"
        ));
    };

    state.proxy.log_event(
        "info",
        "request",
        format!(
            "ws /v1/responses model={model_id} route_source={route_source} provider={} upstream_model={} wire_api={} auth_source={} credential={}",
            route.provider_name,
            route.model_id,
            route.wire_api,
            route.auth_source.as_deref().unwrap_or("provider_config"),
            credential_status(&route)
        ),
    );

    if is_managed_codex_oauth_route(&route) {
        state.proxy.log_event(
            "warn",
            "request",
            format!("ws /v1/responses model={model_id} uses official managed Codex auth"),
        );
        return forward_managed_codex_ws(socket, headers, path_and_query, raw).await;
    }

    if route.wire_api == "responses" {
        return forward_responses_ws_to_responses(socket, state, headers, &message, &route).await;
    }

    let bridged_images = apply_vision_bridge(&state.http, &route, &mut message).await;
    if bridged_images > 0 {
        tracing::info!(
            provider = %route.provider_name,
            model = %route.model_id,
            images = %bridged_images,
            "vision bridge replaced input images before chat fallback"
        );
    }

    let mut chat = responses_create_to_chat_request_with_options(
        &message,
        &route.model_id,
        ChatRequestOptions {
            chat_reasoning: route.chat_reasoning.as_ref(),
            text_only_input: route.text_only,
        },
    )
    .map_err(|e| format!("convert responses to chat failed: {e}"))?;
    let session_id = conversation_session_id(headers, &message);
    merge_chat_history(&state.proxy, &session_id, &message, &mut chat.body);
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
        for event in stream_state.ingest_chat_completion_json(&value) {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    state
        .proxy
        .append_conversation_message(&session_id, stream_state.assistant_message());

    Ok(())
}

async fn forward_responses_ws_to_responses(
    socket: &mut WebSocket,
    state: &ServerState,
    headers: &HeaderMap,
    message: &serde_json::Value,
    route: &ResolvedRoute,
) -> Result<(), String> {
    let upstream_url =
        build_upstream_url(&route.upstream_base_url, "/responses").map_err(|e| e.to_string())?;
    let mut upstream_body = message.clone();
    let bridged_images = apply_vision_bridge(&state.http, route, &mut upstream_body).await;
    if bridged_images > 0 {
        tracing::info!(
            provider = %route.provider_name,
            model = %route.model_id,
            images = %bridged_images,
            "vision bridge replaced input images before responses websocket passthrough"
        );
    }
    if let Some(obj) = upstream_body.as_object_mut() {
        obj.insert(
            "model".to_string(),
            serde_json::Value::String(route.model_id.clone()),
        );
        obj.insert("stream".to_string(), serde_json::Value::Bool(true));
    }

    let mut request = state
        .http
        .post(&upstream_url)
        .header("content-type", "application/json")
        .json(&upstream_body);

    for (name, value) in headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if is_hop_by_hop_header(&lower) || lower == "authorization" {
            continue;
        }
        request = request.header(name.as_str(), value.as_bytes());
    }

    let mut injected = HeaderMap::new();
    inject_auth_headers(route, &mut injected).map_err(|e| e.to_string())?;
    for (name, value) in injected.iter() {
        request = request.header(name.as_str(), value.as_bytes());
    }

    tracing::info!(
        provider = %route.provider_name,
        model = %route.model_id,
        "responses websocket routed to responses upstream"
    );

    let response = request
        .send()
        .await
        .map_err(|e| format!("upstream responses send failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "upstream responses returned status {}: {}",
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

    if content_type.contains("text/event-stream") {
        stream_responses_sse_to_ws(socket, response).await
    } else {
        let value = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("read upstream responses JSON failed: {e}"))?;
        send_ws_json(
            socket,
            &serde_json::json!({
                "type": "response.completed",
                "response": value,
            }),
        )
        .await
        .map_err(|e| e.to_string())
    }
}

async fn forward_responses_http_to_chat(
    state: &ServerState,
    headers: &HeaderMap,
    body: &Bytes,
    route: &ResolvedRoute,
) -> Response {
    let mut message: serde_json::Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid JSON request body: {e}"),
            );
        }
    };
    let stream_requested = message
        .get("stream")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let bridged_images = apply_vision_bridge(&state.http, route, &mut message).await;
    if bridged_images > 0 {
        tracing::info!(
            provider = %route.provider_name,
            model = %route.model_id,
            images = %bridged_images,
            "vision bridge replaced input images before HTTP chat fallback"
        );
    }

    let mut chat = match responses_create_to_chat_request_with_options(
        &message,
        &route.model_id,
        ChatRequestOptions {
            chat_reasoning: route.chat_reasoning.as_ref(),
            text_only_input: route.text_only,
        },
    ) {
        Ok(value) => value,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("convert responses to chat failed: {e}"),
            );
        }
    };
    let session_id = conversation_session_id(headers, &message);
    merge_chat_history(&state.proxy, &session_id, &message, &mut chat.body);
    if stream_requested {
        chat.body["stream"] = serde_json::Value::Bool(true);
    } else {
        chat.body["stream"] = serde_json::Value::Bool(false);
        if let Some(obj) = chat.body.as_object_mut() {
            obj.remove("stream_options");
        }
    }

    let upstream_url = match build_upstream_url(&route.upstream_base_url, "/chat/completions") {
        Ok(url) => url,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &e.to_string(),
            );
        }
    };

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
    if let Err(e) = inject_auth_headers(route, &mut injected) {
        return openai_error_response(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string());
    }
    for (name, value) in injected.iter() {
        request = request.header(name.as_str(), value.as_bytes());
    }

    let started = std::time::Instant::now();
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &format!("upstream chat send failed: {e}"),
            );
        }
    };
    let status = upstream.status();
    let latency_ms = started.elapsed().as_millis() as u64;
    state.proxy.log_event(
        if status.is_success() { "info" } else { "warn" },
        "response",
        format!(
            "http /v1/responses model={} provider={} upstream_status={} latency_ms={} adapter=responses_to_chat",
            route.model_id,
            route.provider_name,
            status.as_u16(),
            latency_ms
        ),
    );
    tracing::info!(
        provider = %route.provider_name,
        model = %route.model_id,
        kind = "responses_to_chat",
        status = %status.as_u16(),
        latency_ms = %latency_ms,
        "upstream response"
    );

    if !status.is_success() {
        let text = upstream.text().await.unwrap_or_default();
        let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut response = Response::builder()
            .status(axum_status)
            .body(axum::body::Body::from(text))
            .unwrap_or_else(|_| {
                openai_error_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    "build upstream error response failed",
                )
            });
        response.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        return response;
    }

    let content_type = upstream
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if stream_requested && content_type.contains("text/event-stream") {
        let stream = chat_sse_to_responses_sse_stream(
            upstream,
            route.model_id.clone(),
            state.proxy.clone(),
            session_id,
        );
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| {
                openai_error_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    "build responses stream failed",
                )
            });
        response.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-cache"));
        return response;
    }

    let value = match upstream.json::<serde_json::Value>().await {
        Ok(value) => value,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &format!("read upstream chat JSON failed: {e}"),
            );
        }
    };

    if stream_requested {
        let events = chat_completion_json_to_response_events(&value, &route.model_id);
        append_chat_completion_to_history(&state.proxy, &session_id, &value);
        let sse = response_events_to_sse(&events);
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .body(axum::body::Body::from(sse))
            .unwrap_or_else(|_| {
                openai_error_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    "build responses stream failed",
                )
            });
        response.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        return response;
    }

    let body = chat_completion_json_to_response_body(&value, &route.model_id);
    append_chat_completion_to_history(&state.proxy, &session_id, &value);
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(axum::body::Body::from(body.to_string()))
        .unwrap_or_else(|_| {
            openai_error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "build responses JSON failed",
            )
        });
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

async fn stream_responses_sse_to_ws(
    socket: &mut WebSocket,
    response: reqwest::Response,
) -> Result<(), String> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut normalizer = ResponseEventPassthroughNormalizer::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("upstream stream error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = buffer.find('\n') {
            let line = buffer[..index].trim_end_matches('\r').to_string();
            buffer = buffer[index + 1..].to_string();
            if let Some(event) = responses_sse_line_to_ws_event(&line, &mut normalizer) {
                send_ws_json(socket, &event)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    let tail = buffer.trim();
    if !tail.is_empty() {
        if let Some(event) = responses_sse_line_to_ws_event(tail, &mut normalizer) {
            send_ws_json(socket, &event)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn responses_sse_line_to_ws_event(
    line: &str,
    normalizer: &mut ResponseEventPassthroughNormalizer,
) -> Option<serde_json::Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with("data:") {
        return None;
    }
    let payload = trimmed.trim_start_matches("data:").trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    normalizer.normalize_event(value)
}

fn conversation_session_id(headers: &HeaderMap, message: &serde_json::Value) -> String {
    message
        .get("client_metadata")
        .and_then(|metadata| metadata.get("session_id"))
        .and_then(|value| value.as_str())
        .or_else(|| header_value(headers, "x-session-id"))
        .or_else(|| header_value(headers, "session-id"))
        .or_else(|| header_value(headers, "x-thread-id"))
        .or_else(|| header_value(headers, "thread-id"))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default")
        .to_string()
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn merge_chat_history(
    proxy: &Arc<ProxyState>,
    session_id: &str,
    message: &serde_json::Value,
    chat_body: &mut serde_json::Value,
) {
    let Some(messages) = chat_body
        .get("messages")
        .and_then(|value| value.as_array())
        .cloned()
    else {
        return;
    };
    let reset = message.get("previous_response_id").is_none();
    chat_body["messages"] =
        serde_json::Value::Array(proxy.merge_conversation_history(session_id, messages, reset));
}

fn append_chat_completion_to_history(
    proxy: &Arc<ProxyState>,
    session_id: &str,
    value: &serde_json::Value,
) {
    let Some(message) = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .cloned()
    else {
        return;
    };
    let mut assistant = message;
    if let Some(obj) = assistant.as_object_mut() {
        obj.insert(
            "role".to_string(),
            serde_json::Value::String("assistant".to_string()),
        );
    }
    proxy.append_conversation_message(session_id, assistant);
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

struct ChatSseToResponsesSseState {
    stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
    buffer: String,
    stream_state: ResponsesWsStreamState,
    pending: VecDeque<Result<Bytes, AppError>>,
    proxy: Arc<ProxyState>,
    session_id: String,
    finished: bool,
}

fn chat_sse_to_responses_sse_stream(
    response: reqwest::Response,
    model: String,
    proxy: Arc<ProxyState>,
    session_id: String,
) -> impl futures::Stream<Item = Result<Bytes, AppError>> + Send + 'static {
    let state = ChatSseToResponsesSseState {
        stream: response.bytes_stream().boxed(),
        buffer: String::new(),
        stream_state: ResponsesWsStreamState::new(model),
        pending: VecDeque::new(),
        proxy,
        session_id,
        finished: false,
    };

    futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(chunk) = state.pending.pop_front() {
                return Some((chunk, state));
            }

            if state.finished {
                return None;
            }

            match state.stream.next().await {
                Some(Ok(chunk)) => {
                    state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(index) = state.buffer.find('\n') {
                        let line = state.buffer[..index].trim_end_matches('\r').to_string();
                        state.buffer = state.buffer[index + 1..].to_string();
                        let events = state.stream_state.ingest_sse_line(&line);
                        push_response_events_as_sse(&mut state.pending, events);
                    }
                }
                Some(Err(e)) => {
                    state.pending.push_back(Err(AppError::Command(format!(
                        "upstream stream error: {e}"
                    ))));
                    state.finished = true;
                }
                None => {
                    let tail = state.buffer.trim().to_string();
                    if !tail.is_empty() {
                        let events = state.stream_state.ingest_sse_line(&tail);
                        push_response_events_as_sse(&mut state.pending, events);
                    }
                    if !state.stream_state.is_completed() {
                        let events = state.stream_state.finish_events();
                        push_response_events_as_sse(&mut state.pending, events);
                    }
                    state.proxy.append_conversation_message(
                        &state.session_id,
                        state.stream_state.assistant_message(),
                    );
                    state.finished = true;
                }
            }
        }
    })
}

fn push_response_events_as_sse(
    pending: &mut VecDeque<Result<Bytes, AppError>>,
    events: Vec<serde_json::Value>,
) {
    for event in events {
        pending.push_back(Ok(response_event_to_sse_bytes(&event)));
    }
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
    let catalog_route = resolve_catalog_route(&model_id, &map);
    let inject_route = if catalog_route.is_none() {
        resolve_route(&model_id, &map)
    } else {
        None
    };
    let route_source = if catalog_route.is_some() {
        "catalog"
    } else if inject_route.is_some() {
        "inject-map"
    } else {
        "unresolved"
    };
    let route = catalog_route.or(inject_route);
    let route = match route {
        Some(route) => route,
        None => {
            if is_native_openai_catalog_model(&model_id) {
                let path = if kind == "chat" {
                    "/v1/chat/completions"
                } else {
                    "/v1/responses"
                };
                state.proxy.log_event(
                    "warn",
                    "request",
                    format!(
                        "http /v1/{kind} model={model_id} unresolved locally; fallback=native_openai"
                    ),
                );
                return forward_native_openai_http(&state.http, headers, body.clone(), path).await;
            }
            state.proxy.log_event(
                "error",
                "request",
                format!("http /v1/{kind} model={model_id} unresolved; returning model_not_found"),
            );
            return openai_error_response(
                StatusCode::NOT_FOUND,
                "model_not_found",
                &format!("model `{model_id}` is not in the Codex Box proxy route table"),
            );
        }
    };

    state.proxy.log_event(
            "info",
            "request",
            format!(
                "http /v1/{kind} model={model_id} route_source={route_source} provider={} upstream_model={} wire_api={} auth_source={} credential={}",
                route.provider_name,
                route.model_id,
                route.wire_api,
                route.auth_source.as_deref().unwrap_or("provider_config"),
                credential_status(&route)
            ),
        );

    if is_managed_codex_oauth_route(&route) {
        let path = if kind == "chat" {
            "/v1/chat/completions"
        } else {
            "/v1/responses"
        };
        state.proxy.log_event(
            "warn",
            "request",
            format!("http /v1/{kind} model={model_id} uses official managed Codex auth"),
        );
        let started = std::time::Instant::now();
        let resp = forward_managed_codex_http(&state.http, headers, body.clone(), path).await;
        state.proxy.log_event(
            if resp.status().is_success() {
                "info"
            } else {
                "warn"
            },
            "response",
            format!(
                "http /v1/{kind} model={model_id} provider={} upstream_status={} latency_ms={}",
                route.provider_name,
                resp.status().as_u16(),
                started.elapsed().as_millis()
            ),
        );
        return resp;
    }

    // 如果 wire_api 与 endpoint 不匹配,返回 400
    let endpoint_kind = if kind == "chat" { "chat" } else { "responses" };
    if endpoint_kind == "responses" && route.wire_api == "chat" {
        return forward_responses_http_to_chat(state, headers, body, &route).await;
    }
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

    if endpoint_kind == "responses"
        && route.wire_api == "responses"
        && route.vision_bridge.is_some()
    {
        return forward_responses_http_to_responses(state, headers, body, &route).await;
    }

    let result = match endpoint_kind {
        "chat" => forward_chat(&state.http, &route, headers, body.clone()).await,
        "responses" => forward_responses(&state.http, &route, headers, body.clone()).await,
        _ => unreachable!(),
    };

    match result {
        Ok(resp) => {
            state.proxy.log_event(
                if resp.status().is_success() {
                    "info"
                } else {
                    "warn"
                },
                "response",
                format!(
                    "http /v1/{kind} model={model_id} provider={} upstream_status={}",
                    route.provider_name,
                    resp.status().as_u16()
                ),
            );
            resp
        }
        Err(e) => {
            state.proxy.log_event(
                "error",
                "request",
                format!(
                    "http /v1/{kind} model={model_id} upstream provider={} failed: {e}",
                    route.provider_name
                ),
            );
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

async fn forward_native_openai_http(
    client: &Client,
    headers: &HeaderMap,
    body: Bytes,
    path_and_query: &str,
) -> Response {
    forward_native_openai_http_inner(client, headers, body, path_and_query, false).await
}

async fn forward_managed_codex_http(
    client: &Client,
    headers: &HeaderMap,
    body: Bytes,
    path_and_query: &str,
) -> Response {
    forward_native_openai_http_inner(client, headers, body, path_and_query, true).await
}

async fn forward_native_openai_http_inner(
    client: &Client,
    headers: &HeaderMap,
    body: Bytes,
    path_and_query: &str,
    force_chatgpt_codex: bool,
) -> Response {
    let codex_access_token = force_chatgpt_codex.then(read_codex_access_token).flatten();
    if let Some(message) = native_openai_unresolved_auth_message(headers) {
        if codex_access_token.is_none() {
            return openai_error_response(
                StatusCode::UNAUTHORIZED,
                "native_openai_auth_unresolved",
                message,
            );
        }
    }

    if force_chatgpt_codex
        && codex_access_token.is_none()
        && !headers_have_real_authorization(headers)
    {
        return openai_error_response(
            StatusCode::UNAUTHORIZED,
            "native_openai_auth_unresolved",
            "native OpenAI/Codex auth is unresolved: ~/.codex/auth.json does not contain a usable access_token.",
        );
    }

    let target_url = native_openai_target_url(headers, path_and_query, false, force_chatgpt_codex);
    let mut request = client.post(&target_url).body(body);
    let forward_headers = native_openai_forward_headers(headers, codex_access_token.as_deref());

    for (name, value) in forward_headers.iter() {
        request = request.header(name.as_str(), value.as_bytes());
    }

    let upstream = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                "native_openai_upstream_error",
                &format!("native OpenAI forward failed: {e}"),
            );
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response_headers = HeaderMap::new();
    for (name, value) in upstream.headers().iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if is_hop_by_hop_header(&lower) {
            continue;
        }
        response_headers.insert(name.clone(), value.clone());
    }

    let stream = upstream.bytes_stream().map(|chunk| {
        chunk.map_err(|e| crate::error::AppError::Command(format!("native stream error: {e}")))
    });
    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| {
            openai_error_response(
                StatusCode::BAD_GATEWAY,
                "native_openai_upstream_error",
                "build native OpenAI response failed",
            )
        });
    *response.headers_mut() = response_headers;
    response
}

async fn forward_responses_http_to_responses(
    state: &ServerState,
    headers: &HeaderMap,
    body: &Bytes,
    route: &ResolvedRoute,
) -> Response {
    let mut message: serde_json::Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("invalid JSON request body: {e}"),
            );
        }
    };
    let bridged_images = apply_vision_bridge(&state.http, route, &mut message).await;
    if bridged_images > 0 {
        tracing::info!(
            provider = %route.provider_name,
            model = %route.model_id,
            images = %bridged_images,
            "vision bridge replaced input images before responses passthrough"
        );
    }

    let body = match serde_json::to_vec(&message) {
        Ok(value) => Bytes::from(value),
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("serialize bridged responses body failed: {e}"),
            );
        }
    };

    match forward_responses(&state.http, route, headers, body).await {
        Ok(resp) => {
            state.proxy.log_event(
                if resp.status().is_success() { "info" } else { "warn" },
                "response",
                format!(
                    "http /v1/responses model={} provider={} upstream_status={} adapter=responses_passthrough",
                    route.model_id,
                    route.provider_name,
                    resp.status().as_u16()
                ),
            );
            resp
        }
        Err(e) => {
            state.proxy.log_event(
                "error",
                "response",
                format!(
                    "http /v1/responses model={} provider={} upstream_error={e} adapter=responses_passthrough",
                    route.model_id, route.provider_name
                ),
            );
            openai_error_response(StatusCode::BAD_GATEWAY, "upstream_error", &e.to_string())
        }
    }
}

async fn forward_native_openai_ws(
    socket: &mut WebSocket,
    headers: &HeaderMap,
    path_and_query: &str,
    first_message: &[u8],
) -> Result<(), String> {
    if let Some(message) = native_openai_unresolved_auth_message(headers) {
        return Err(message.to_string());
    }

    let target_url = native_openai_target_url(headers, path_and_query, true, false);
    forward_native_openai_ws_to_url(socket, headers, first_message, &target_url, None).await
}

async fn forward_managed_codex_ws(
    socket: &mut WebSocket,
    headers: &HeaderMap,
    path_and_query: &str,
    first_message: &[u8],
) -> Result<(), String> {
    let codex_access_token = read_codex_access_token();
    if let Some(message) = native_openai_unresolved_auth_message(headers) {
        if codex_access_token.is_none() {
            return Err(message.to_string());
        }
    }
    if codex_access_token.is_none() && !headers_have_real_authorization(headers) {
        return Err(
            "native OpenAI/Codex auth is unresolved: ~/.codex/auth.json does not contain a usable access_token."
                .to_string(),
        );
    }

    let target_url = native_openai_target_url(headers, path_and_query, true, true);
    forward_native_openai_ws_to_url(
        socket,
        headers,
        first_message,
        &target_url,
        codex_access_token,
    )
    .await
}

async fn forward_native_openai_ws_to_url(
    socket: &mut WebSocket,
    headers: &HeaderMap,
    first_message: &[u8],
    target_url: &str,
    codex_access_token: Option<String>,
) -> Result<(), String> {
    let mut request = target_url
        .into_client_request()
        .map_err(|e| format!("build native OpenAI websocket request failed: {e}"))?;
    let forward_headers = native_openai_forward_headers(headers, codex_access_token.as_deref());
    for (name, value) in forward_headers.iter() {
        if let (Ok(header_name), Ok(header_value)) = (
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_str().as_bytes()),
            tokio_tungstenite::tungstenite::http::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            request.headers_mut().insert(header_name, header_value);
        }
    }

    let (mut upstream, _) = connect_async(request)
        .await
        .map_err(|e| format!("connect native OpenAI websocket failed: {e}"))?;
    upstream
        .send(TungsteniteMessage::Text(
            String::from_utf8_lossy(first_message).to_string(),
        ))
        .await
        .map_err(|e| format!("send native OpenAI websocket message failed: {e}"))?;

    while let Some(next) = upstream.next().await {
        let message = next.map_err(|e| format!("native OpenAI websocket read failed: {e}"))?;
        match message {
            TungsteniteMessage::Text(text) => {
                send_ws_json(
                    socket,
                    &serde_json::from_str(&text)
                        .unwrap_or_else(|_| serde_json::json!({ "type": "message", "data": text })),
                )
                .await
                .map_err(|e| e.to_string())?;
                if text.contains(r#""type":"response.completed""#)
                    || text.contains(r#""type": "response.completed""#)
                {
                    break;
                }
            }
            TungsteniteMessage::Binary(bytes) => {
                socket
                    .send(Message::Binary(bytes))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            TungsteniteMessage::Ping(bytes) => {
                upstream
                    .send(TungsteniteMessage::Pong(bytes))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            TungsteniteMessage::Close(_) => break,
            TungsteniteMessage::Pong(_) | TungsteniteMessage::Frame(_) => {}
        }
    }
    Ok(())
}

fn native_openai_target_url(
    headers: &HeaderMap,
    path_and_query: &str,
    websocket: bool,
    force_chatgpt_codex: bool,
) -> String {
    let scheme = if websocket { "wss" } else { "https" };
    let is_chatgpt_account = headers
        .keys()
        .any(|name| name.as_str().eq_ignore_ascii_case("chatgpt-account-id"));
    if force_chatgpt_codex || is_chatgpt_account {
        let (path, query) = path_and_query
            .split_once('?')
            .map(|(path, query)| (path, format!("?{query}")))
            .unwrap_or((path_and_query, String::new()));
        let mut sub_path = path.strip_prefix("/v1/").unwrap_or(path).to_string();
        if !sub_path.starts_with("codex/") {
            sub_path = format!("codex/{sub_path}");
        }
        format!("{scheme}://chatgpt.com/backend-api/{sub_path}{query}")
    } else {
        format!("{scheme}://api.openai.com{path_and_query}")
    }
}

fn credential_status(route: &ResolvedRoute) -> &'static str {
    if is_managed_codex_oauth_route(route) {
        return if read_codex_access_token().is_some() {
            "present"
        } else {
            "missing"
        };
    }
    if route
        .api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return "present";
    }
    match route
        .env_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        Some(env_key) => match std::env::var(env_key) {
            Ok(value) if !value.trim().is_empty() => "present",
            _ => "missing",
        },
        None => {
            if route.http_headers.keys().any(|name| {
                name.eq_ignore_ascii_case("authorization")
                    || name.eq_ignore_ascii_case("x-api-key")
                    || name.eq_ignore_ascii_case("api-key")
            }) {
                "present"
            } else {
                "not_configured"
            }
        }
    }
}

fn is_managed_codex_oauth_route(route: &ResolvedRoute) -> bool {
    route.auth_source.as_deref() == Some("managed_codex_oauth")
}

fn read_codex_access_token() -> Option<String> {
    let path = dirs::home_dir()?.join(".codex").join("auth.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    value
        .get("tokens")
        .and_then(|tokens| tokens.get("access_token"))
        .and_then(|token| token.as_str())
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
}

fn headers_have_real_authorization(headers: &HeaderMap) -> bool {
    headers
        .get_all("authorization")
        .iter()
        .any(|value| !is_proxy_placeholder_authorization(value))
}

fn native_openai_forward_headers(
    headers: &HeaderMap,
    codex_access_token: Option<&str>,
) -> HeaderMap {
    let mut out = HeaderMap::new();
    let mut forwarded_real_authorization = false;

    for (name, value) in headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if is_hop_by_hop_header(&lower)
            || lower == "accept-encoding"
            || lower == "connection"
            || lower == "upgrade"
            || lower.starts_with("sec-websocket-")
        {
            continue;
        }
        if lower == "authorization" && is_proxy_placeholder_authorization(value) {
            continue;
        }
        if lower == "authorization" {
            forwarded_real_authorization = true;
        }
        out.insert(name.clone(), value.clone());
    }

    if !forwarded_real_authorization {
        if let Some(token) = codex_access_token
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
                out.insert("authorization", value);
            }
        }
    }

    out
}

fn native_openai_unresolved_auth_message(headers: &HeaderMap) -> Option<&'static str> {
    let has_placeholder_auth = headers
        .get_all("authorization")
        .iter()
        .any(is_proxy_placeholder_authorization);
    let has_real_auth = headers_have_real_authorization(headers);
    let has_chatgpt_context = headers
        .keys()
        .any(|name| name.as_str().eq_ignore_ascii_case("chatgpt-account-id"));

    (has_placeholder_auth && !has_real_auth && !has_chatgpt_context).then_some(
        "native OpenAI/Codex auth is unresolved: PROXY_MANAGED/dummy is only a local placeholder and must not be sent upstream. Keep Codex Desktop official auth available, or route this model through an explicit OpenAI API key provider.",
    )
}

fn is_proxy_placeholder_authorization(value: &HeaderValue) -> bool {
    value
        .to_str()
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower.contains("proxy_managed")
                || lower.contains("dummy")
                || lower.contains("opencodex")
        })
        .unwrap_or(false)
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
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

    static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct ScopedTestHome {
        path: tempfile::TempDir,
        previous_home: Option<OsString>,
        previous_minimax_key: Option<OsString>,
        _guard: MutexGuard<'static, ()>,
    }

    impl ScopedTestHome {
        fn new() -> Self {
            let guard = TEST_ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("test env lock poisoned");
            let previous_home = std::env::var_os("HOME");
            let previous_minimax_key = std::env::var_os("MINIMAX_E2E_API_KEY");
            let path = tempfile::tempdir().expect("create temp home");
            std::env::set_var("HOME", path.path());
            std::env::set_var("MINIMAX_E2E_API_KEY", "sk-test-minimax");
            Self {
                path,
                previous_home,
                previous_minimax_key,
                _guard: guard,
            }
        }

        fn path(&self) -> &std::path::Path {
            self.path.path()
        }
    }

    impl Drop for ScopedTestHome {
        fn drop(&mut self) {
            if let Some(value) = &self.previous_home {
                std::env::set_var("HOME", value);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(value) = &self.previous_minimax_key {
                std::env::set_var("MINIMAX_E2E_API_KEY", value);
            } else {
                std::env::remove_var("MINIMAX_E2E_API_KEY");
            }
        }
    }

    #[test]
    fn native_openai_auth_guard_rejects_proxy_managed_placeholder() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );

        let message = native_openai_unresolved_auth_message(&headers)
            .expect("placeholder auth should be rejected before upstream");

        assert!(message.contains("PROXY_MANAGED"));
    }

    #[test]
    fn native_openai_auth_guard_allows_placeholder_with_chatgpt_context() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );
        headers.insert("chatgpt-account-id", HeaderValue::from_static("acct_test"));

        assert!(native_openai_unresolved_auth_message(&headers).is_none());
        assert!(headers
            .get_all("authorization")
            .iter()
            .any(is_proxy_placeholder_authorization));
    }

    #[test]
    fn native_openai_auth_guard_allows_non_placeholder_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer real-token"),
        );

        assert!(native_openai_unresolved_auth_message(&headers).is_none());
    }

    #[test]
    fn reads_codex_access_token_from_auth_json() {
        let home = ScopedTestHome::new();
        let codex_dir = home.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("auth.json"),
            r#"{"tokens":{"access_token":"real-codex-token","refresh_token":"refresh"}}"#,
        )
        .unwrap();

        assert_eq!(
            read_codex_access_token().as_deref(),
            Some("real-codex-token")
        );
    }

    #[test]
    fn managed_codex_headers_replace_placeholder_with_auth_json_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer PROXY_MANAGED"),
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let forwarded = native_openai_forward_headers(&headers, Some("real-codex-token"));

        assert_eq!(
            forwarded
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer real-codex-token")
        );
        assert_eq!(
            forwarded
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

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

    #[tokio::test]
    async fn responses_websocket_chat_json_preserves_reasoning_and_tool_calls() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream = Router::new().route("/v1/chat/completions", post(mock_chat_json_tool_call));
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap {
            updated_at: "2026-06-27T00:00:00Z".to_string(),
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
                        "content": [{ "type": "input_text", "text": "看截图" }]
                    }
                ],
                "stream": false
            })
            .to_string(),
        ))
        .await
        .unwrap();

        let mut saw_reasoning = false;
        let mut saw_text = false;
        let mut saw_tool_added = false;
        let mut saw_tool_done = false;
        let mut saw_completed = false;

        for _ in 0..20 {
            let Ok(Some(Ok(TungsteniteMessage::Text(text)))) =
                tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await
            else {
                break;
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            match value.get("type").and_then(|value| value.as_str()) {
                Some("response.reasoning_summary_text.delta") => {
                    if value.get("delta").and_then(|value| value.as_str())
                        == Some("先检查截图路径。")
                    {
                        saw_reasoning = true;
                    }
                }
                Some("response.output_text.delta") => {
                    if value.get("delta").and_then(|value| value.as_str()) == Some("我来检查截图。")
                    {
                        saw_text = true;
                    }
                }
                Some("response.output_item.added") => {
                    let item = value.get("item").unwrap_or(&serde_json::Value::Null);
                    if item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                        && item.get("name").and_then(|value| value.as_str()) == Some("shell_cmd")
                    {
                        saw_tool_added = true;
                    }
                }
                Some("response.function_call_arguments.done") => {
                    let arguments = value
                        .get("arguments")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if arguments.contains("/tmp/a.png") {
                        saw_tool_done = true;
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

        assert!(saw_reasoning, "expected reasoning delta from chat JSON");
        assert!(saw_text, "expected output text delta from chat JSON");
        assert!(saw_tool_added, "expected function_call item from chat JSON");
        assert!(
            saw_tool_done,
            "expected function_call arguments from chat JSON"
        );
        assert!(saw_completed, "expected response.completed from chat JSON");
    }

    #[tokio::test]
    async fn responses_websocket_routes_responses_provider_to_responses_upstream() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream = Router::new().route("/v1/responses", post(mock_responses_sse));
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap {
            updated_at: "2026-06-26T00:00:00Z".to_string(),
            port: 0,
            providers: vec![InjectMapEntry {
                name: "minimax".to_string(),
                original_base_url: format!("http://{upstream_addr}/v1"),
                env_key: None,
                http_headers: BTreeMap::new(),
                wire_api: "responses".to_string(),
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
                "model": "minimax/demo",
                "input": "ping",
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
            "expected response.output_text.delta from upstream responses SSE"
        );
        assert!(
            saw_completed,
            "expected response.completed from upstream responses SSE"
        );
    }

    #[tokio::test]
    async fn responses_http_passthrough_applies_vision_bridge_before_upstream() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let captured = Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
        let captured_for_handler = captured.clone();
        let upstream = Router::new()
            .route("/v1/chat/completions", post(mock_vision_json))
            .route(
                "/v1/responses",
                post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                    let captured = captured_for_handler.clone();
                    async move {
                        *captured.lock().unwrap() = Some(body);
                        axum::Json(serde_json::json!({
                            "id": "resp_1",
                            "object": "response",
                            "status": "completed",
                            "output": []
                        }))
                    }
                }),
            );
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap {
            updated_at: "2026-06-26T00:00:00Z".to_string(),
            port: 0,
            providers: vec![InjectMapEntry {
                name: "minimax".to_string(),
                original_base_url: format!("http://{upstream_addr}/v1"),
                env_key: None,
                http_headers: BTreeMap::new(),
                wire_api: "responses".to_string(),
                models: vec!["demo".to_string()],
                kind: "compatible_api".to_string(),
                extra: BTreeMap::from([(
                    "visionBridge".to_string(),
                    serde_json::json!({
                        "baseUrl": format!("http://{upstream_addr}/v1"),
                        "model": "vision-demo"
                    }),
                )]),
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

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/responses"))
            .json(&serde_json::json!({
                "model": "minimax/demo",
                "stream": false,
                "input": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "input_text", "text": "看图" },
                            { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                        ]
                    }
                ]
            }))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        let body = captured.lock().unwrap().clone().unwrap();
        let body_text = serde_json::to_string(&body).unwrap();
        assert_eq!(body["model"], "demo");
        assert!(!body_text.contains("input_image"));
        assert!(body_text.contains("[截图描述: 该截图显示 502 Bad Gateway]"));
    }

    #[tokio::test]
    async fn multirouter_models_endpoint_and_responses_share_catalog_routes() {
        let scoped_home = ScopedTestHome::new();
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let captured_body = Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
        let captured_auth = Arc::new(std::sync::Mutex::new(None::<String>));
        let captured_body_for_handler = captured_body.clone();
        let captured_auth_for_handler = captured_auth.clone();
        let upstream = Router::new().route(
            "/v1/responses",
            post(
                move |headers: HeaderMap, axum::Json(body): axum::Json<serde_json::Value>| {
                    let captured_body = captured_body_for_handler.clone();
                    let captured_auth = captured_auth_for_handler.clone();
                    async move {
                        *captured_body.lock().unwrap() = Some(body);
                        *captured_auth.lock().unwrap() = headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string);
                        axum::Json(serde_json::json!({
                            "id": "resp_multirouter_e2e",
                            "object": "response",
                            "status": "completed",
                            "output": [{
                                "type": "message",
                                "role": "assistant",
                                "content": [{ "type": "output_text", "text": "pong" }]
                            }]
                        }))
                    }
                },
            ),
        );
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let codex_box_dir = scoped_home.path().join(".codex").join("codex-box");
        std::fs::create_dir_all(&codex_box_dir).unwrap();
        std::fs::write(
            codex_box_dir.join("providers.json"),
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "name": "minimax",
                    "base_url": format!("http://{upstream_addr}/v1"),
                    "wire_api": "responses",
                    "api_key": "$MINIMAX_E2E_API_KEY",
                    "enabled": true
                },
                {
                    "name": "codex_model_router_v2",
                    "base_url": "http://127.0.0.1:1455/v1",
                    "wire_api": "responses",
                    "enabled": true,
                    "codexRouting": {
                        "enabled": true,
                        "defaultRouteId": "minimax",
                        "routes": [
                            {
                                "id": "openai-official",
                                "label": "OpenAI Official",
                                "enabled": true,
                                "match": { "models": ["e2e-gpt-native"], "prefixes": [] },
                                "upstream": {
                                    "baseUrl": "https://chatgpt.com/backend-api/codex",
                                    "apiFormat": "openai_responses",
                                    "auth": { "source": "managed_codex_oauth" },
                                    "modelMap": { "e2e-gpt-native": "gpt-5.5" }
                                }
                            },
                            {
                                "id": "minimax",
                                "label": "minimax",
                                "enabled": true,
                                "targetProviderId": "minimax",
                                "match": { "models": ["e2e-minimax-m3"], "prefixes": [] },
                                "upstream": {
                                    "apiFormat": "openai_responses",
                                    "auth": { "source": "provider_config" },
                                    "modelMap": { "e2e-minimax-m3": "MiniMax-M3" }
                                }
                            }
                        ]
                    }
                }
            ]))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            codex_box_dir.join("custom_model_catalog.json"),
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "model_id": "e2e-gpt-native",
                    "display_name": "GPT Native E2E",
                    "provider": "openai",
                    "backend_model": "gpt-5.5",
                    "visible": true
                },
                {
                    "model_id": "e2e-minimax-m3",
                    "display_name": "MiniMax E2E",
                    "provider": "codex_model_router_v2",
                    "backend_provider": "codex_model_router_v2",
                    "backend_model": "MiniMax-M3",
                    "visible": true,
                    "targetProvider": "minimax",
                    "target_provider": "minimax"
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap::default());
        let app = build_router(ServerState {
            proxy,
            http: build_reqwest_client(),
        });
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(proxy_listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let models_response = client
            .get(format!("http://{proxy_addr}/v1/models"))
            .send()
            .await
            .unwrap();
        assert_eq!(models_response.status(), reqwest::StatusCode::OK);
        let models_json: serde_json::Value = models_response.json().await.unwrap();
        let models = models_json["data"].as_array().expect("models data");
        let native = models
            .iter()
            .find(|model| model["id"] == "e2e-gpt-native")
            .expect("native model in /v1/models");
        assert_eq!(native["provider"], "openai");
        assert_eq!(native["displayName"], "GPT Native E2E");
        let minimax = models
            .iter()
            .find(|model| model["id"] == "e2e-minimax-m3")
            .expect("minimax model in /v1/models");
        assert_eq!(minimax["provider"], "codex_model_router_v2");
        assert_eq!(minimax["backendProvider"], "minimax");
        assert_eq!(minimax["routerProvider"], "codex_model_router_v2");

        let response = client
            .post(format!("http://{proxy_addr}/v1/responses"))
            .json(&serde_json::json!({
                "model": "e2e-minimax-m3",
                "input": "ping",
                "stream": false
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let response_json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(response_json["id"], "resp_multirouter_e2e");

        let upstream_body = captured_body.lock().unwrap().clone().unwrap();
        assert_eq!(upstream_body["model"], "MiniMax-M3");
        assert_eq!(
            captured_auth.lock().unwrap().as_deref(),
            Some("Bearer sk-test-minimax")
        );
    }

    #[tokio::test]
    async fn responses_http_stream_routes_chat_upstream_as_streaming_sse() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream = Router::new().route(
            "/v1/chat/completions",
            post(mock_chat_sse_requires_stream_body),
        );
        tokio::spawn(async move {
            axum::serve(upstream_listener, upstream).await.unwrap();
        });

        let proxy = Arc::new(ProxyState::new());
        proxy.set_inject_map(InjectMap {
            updated_at: "2026-06-27T00:00:00Z".to_string(),
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

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/responses"))
            .json(&serde_json::json!({
                "model": "local/demo",
                "input": [
                    {
                        "type": "message",
                        "role": "user",
                        "content": [{ "type": "input_text", "text": "ping" }]
                    }
                ],
                "stream": true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.contains("text/event-stream"));
        let body = response.text().await.unwrap();

        assert!(body.contains("event: response.output_text.delta"));
        assert!(body.contains("\"delta\":\"pong\""));
        assert!(body.contains("event: response.completed"));
        assert!(body.contains("\"input_tokens\":2"));
    }

    async fn mock_chat_sse() -> impl IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"choices\":[{\"delta\":{\"content\":\"pong\"}}]}\n\ndata: [DONE]\n\n",
        )
    }

    async fn mock_chat_sse_requires_stream_body(
        axum::Json(body): axum::Json<serde_json::Value>,
    ) -> impl IntoResponse {
        assert_eq!(body["model"], "demo");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        (
            [("content-type", "text/event-stream")],
            "data: {\"choices\":[{\"delta\":{\"content\":\"pong\"}}]}\n\ndata: {\"choices\":[{\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3,\"completion_tokens_details\":{\"reasoning_tokens\":0}}}\n\ndata: [DONE]\n\n",
        )
    }

    async fn mock_chat_json_tool_call(
        axum::Json(body): axum::Json<serde_json::Value>,
    ) -> impl IntoResponse {
        assert_eq!(body["model"], "demo");
        assert_ne!(body["stream"], true);
        axum::Json(serde_json::json!({
            "id": "chatcmpl_test",
            "object": "chat.completion",
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "reasoning_content": "先检查截图路径。",
                        "content": "我来检查截图。",
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "shell_cmd",
                                    "arguments": "{\"command\":\"ls -la /tmp/a.png\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 2,
                "completion_tokens": 3,
                "total_tokens": 5,
                "completion_tokens_details": { "reasoning_tokens": 1 }
            }
        }))
    }

    async fn mock_vision_json(
        axum::Json(body): axum::Json<serde_json::Value>,
    ) -> impl IntoResponse {
        assert_eq!(body["model"], "vision-demo");
        axum::Json(serde_json::json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "该截图显示 502 Bad Gateway"
                    }
                }
            ]
        }))
    }

    async fn mock_responses_sse() -> impl IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"pong\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n",
        )
    }
}
