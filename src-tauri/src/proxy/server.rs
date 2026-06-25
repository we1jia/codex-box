// src-tauri/src/proxy/server.rs
//
// axum Router 装配: 注册 /v1/models, /v1/chat/completions, /v1/responses, /healthz
// 共享一个 reqwest::Client 实例(全局,避免每次请求新建连接池)
use crate::proxy::models_endpoint::merged_models;
use crate::proxy::routing::{resolve_catalog_route, resolve_route};
use crate::proxy::state::ProxyState;
use crate::proxy::upstream::{extract_model_id, forward_chat, forward_responses};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
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
        .route("/v1/responses", post(handle_responses))
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
