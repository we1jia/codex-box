// src-tauri/src/proxy/upstream.rs
//
// 上游请求转发: Codex App → Codex Box 代理 (127.0.0.1:1455) → upstream provider
//
// 支持:
//   - POST /v1/chat/completions  →  upstream {base_url}/chat/completions
//   - POST /v1/responses         →  upstream {base_url}/responses
//
// 鉴权: env_key 对应的环境变量值,注入到 Authorization: Bearer <key> (如果有),
//       或者注入到 provider.http_headers 里的其他 auth 头(支持国产模型的特殊 header)。
//       **绝不**把明文 key 写入日志或写回响应。
//
// SSE: 透传 reqwest 的 bytes_stream 给 axum body,不做协议转换(v1 范围)。
use crate::error::{AppError, AppResult};
use crate::proxy::routing::ResolvedRoute;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

/// 解析 body 里的 model 字段(尽力而为,失败返回 400)
pub fn extract_model_id(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("model")
        .and_then(|m| m.as_str())
        .map(String::from)
}

/// 拼装上游 URL
///
/// 约定: base_url 必须以 "/v1" 结尾,然后追加 "/chat/completions" 或 "/responses"。
/// 例: "https://api.openai.com/v1" + "/chat/completions" = "https://api.openai.com/v1/chat/completions"
pub fn build_upstream_url(base_url: &str, suffix: &str) -> AppResult<String> {
    let trimmed = base_url.trim_end_matches('/');
    if !trimmed.ends_with("/v1") && !trimmed.contains("/v1/") {
        return Err(AppError::Command(format!(
            "provider base_url 必须以 /v1 结尾,当前: {base_url}"
        )));
    }
    // 如果 base_url 已经是 /v1/xxx 形式,不再追加 /v1
    let prefix = if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        trimmed.to_string()
    };
    Ok(format!("{prefix}{suffix}"))
}

fn bearer_header_value(raw: &str) -> Option<HeaderValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value = if trimmed.to_ascii_lowercase().starts_with("bearer ") {
        trimmed.to_string()
    } else {
        format!("Bearer {trimmed}")
    };
    HeaderValue::from_str(&value).ok()
}

/// 注入 auth 头(Authorization 来自 env_key 或运行时 direct api_key,其他自定义 header 来自 provider.http_headers)
pub fn inject_auth_headers(route: &ResolvedRoute, headers: &mut HeaderMap) -> AppResult<()> {
    let mut authorization_injected = false;

    // 1. env_key
    if let Some(env_key) = &route.env_key {
        if !env_key.is_empty() {
            match std::env::var(env_key) {
                Ok(value) => {
                    if let Some(v) = bearer_header_value(&value) {
                        headers.insert("Authorization", v);
                        authorization_injected = true;
                    }
                }
                Err(_) => {
                    tracing::warn!(
                        provider = %route.provider_name,
                        env_key = %env_key,
                        "env var not set; no file plaintext api_key fallback will be used"
                    );
                }
            }
        }
    }

    // 2. 运行时 direct api_key。只进内存请求头,不写日志、不回写文件。
    if !authorization_injected {
        if let Some(api_key) = &route.api_key {
            if let Some(v) = bearer_header_value(api_key) {
                headers.insert("Authorization", v);
                authorization_injected = true;
            }
        }
    }

    if !authorization_injected {
        tracing::warn!(
            provider = %route.provider_name,
            "no auth credential available; request will likely fail with 401"
        );
    }

    // 3. provider 自定义 header(http_headers 里)
    for (k, v) in &route.http_headers {
        if HOP_BY_HOP.contains(&k.to_ascii_lowercase().as_str()) {
            continue;
        }
        // 跳过 Authorization(它走 env_key 注入)
        if k.eq_ignore_ascii_case("authorization") {
            continue;
        }
        let name = match axum::http::HeaderName::from_bytes(k.as_bytes()) {
            Ok(n) => n,
            Err(_) => continue,
        };
        if let Ok(hv) = HeaderValue::from_str(v) {
            headers.insert(name, hv);
        }
    }

    Ok(())
}

/// 转发 chat 请求
pub async fn forward_chat(
    client: &Client,
    route: &ResolvedRoute,
    original_headers: &HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let url = build_upstream_url(&route.upstream_base_url, "/chat/completions")?;
    forward_request(client, route, original_headers, body, &url, "chat").await
}

/// 转发 responses 请求
pub async fn forward_responses(
    client: &Client,
    route: &ResolvedRoute,
    original_headers: &HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let url = build_upstream_url(&route.upstream_base_url, "/responses")?;
    forward_request(client, route, original_headers, body, &url, "responses").await
}

async fn forward_request(
    client: &Client,
    route: &ResolvedRoute,
    original_headers: &HeaderMap,
    body: Bytes,
    url: &str,
    kind: &str,
) -> AppResult<Response> {
    let upstream_body = rewrite_model_field(body, &route.model_id);

    // 构造上游请求
    let mut req = client.post(url).body(upstream_body);

    // 透传非 hop-by-hop 头(不放 Content-Length/Host,让 reqwest 处理)
    for (name, value) in original_headers.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.contains(&lower.as_str()) || lower == "authorization" {
            continue;
        }
        req = req.header(name.as_str(), value.as_bytes());
    }

    // 注入鉴权
    let mut injected = HeaderMap::new();
    inject_auth_headers(route, &mut injected)?;
    for (name, value) in injected.iter() {
        req = req.header(name.as_str(), value.as_bytes());
    }

    // 发起请求
    let started = std::time::Instant::now();
    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Command(format!("upstream {kind} send failed: {e}")))?;

    let status = resp.status();
    let latency_ms = started.elapsed().as_millis() as u64;

    // 日志: provider + status + latency,**不记** body / header value
    tracing::info!(
        provider = %route.provider_name,
        model = %route.model_id,
        kind = %kind,
        status = %status.as_u16(),
        latency_ms = %latency_ms,
        "upstream response"
    );

    // 构造 axum Response
    let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response_headers = HeaderMap::new();
    for (name, value) in resp.headers().iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.contains(&lower.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            axum::http::HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            response_headers.insert(n, v);
        }
    }

    let is_stream = response_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false)
        || status.is_success() && looks_like_streaming(&resp);

    if is_stream {
        let stream = resp.bytes_stream();
        let body_stream = stream.map(|chunk| {
            chunk
                .map_err(|e| AppError::Command(format!("upstream stream error: {e}")))
                .map(axum::body::Bytes::from)
        });
        let body = Body::from_stream(body_stream);
        let mut response = Response::builder()
            .status(axum_status)
            .body(body)
            .map_err(|e| AppError::Command(format!("build streaming response: {e}")))?;
        *response.headers_mut() = response_headers;
        Ok(response)
    } else {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Command(format!("upstream read body: {e}")))?;
        let mut response = Response::builder()
            .status(axum_status)
            .body(Body::from(bytes))
            .map_err(|e| AppError::Command(format!("build response: {e}")))?;
        *response.headers_mut() = response_headers;
        Ok(response)
    }
}

fn rewrite_model_field(body: Bytes, upstream_model: &str) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "model".to_string(),
            serde_json::Value::String(upstream_model.to_string()),
        );
        if let Ok(serialized) = serde_json::to_vec(&value) {
            return Bytes::from(serialized);
        }
    }
    body
}

fn looks_like_streaming(_resp: &reqwest::Response) -> bool {
    // 简化: 大多数 chat completions 是 stream=true; 我们用 SSE content-type 判断
    // 上面已处理; 此函数留作扩展点
    false
}

/// 构造 reqwest client(全局单例,可通过 Tauri state 共享)
pub fn build_reqwest_client() -> Client {
    Client::builder()
        .user_agent("codex-box-proxy/0.1.0")
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_upstream_url_appends_chat() {
        let url = build_upstream_url("https://api.openai.com/v1", "/chat/completions").unwrap();
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn build_upstream_url_strips_trailing_slash() {
        let url = build_upstream_url("https://api.openai.com/v1/", "/responses").unwrap();
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn build_upstream_url_rejects_non_v1_base() {
        let result = build_upstream_url("https://api.openai.com", "/chat/completions");
        assert!(result.is_err());
    }

    #[test]
    fn extract_model_id_reads_json() {
        let body = br#"{"model":"zhipu/glm-4","messages":[]}"#;
        assert_eq!(extract_model_id(body).as_deref(), Some("zhipu/glm-4"));
    }

    #[test]
    fn extract_model_id_returns_none_for_invalid() {
        let body = b"not json";
        assert!(extract_model_id(body).is_none());
    }

    #[test]
    fn extract_model_id_returns_none_when_missing() {
        let body = br#"{"messages":[]}"#;
        assert!(extract_model_id(body).is_none());
    }
}
