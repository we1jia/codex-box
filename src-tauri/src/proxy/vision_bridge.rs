use crate::proxy::routing::{ResolvedRoute, VisionBridgeConfig};
use crate::proxy::upstream::build_upstream_url;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

const VISION_PROMPT: &str =
    "请详细描述此屏幕截图的内容。如果包含文字、错误信息、按钮、代码或界面状态,请尽量提取。只输出描述,不要额外对话。";
const VISION_FAILURE_MARKER: &str = "无法通过视觉 fallback 生成截图描述";

#[derive(Debug, Clone)]
struct ImageTarget {
    message_index: usize,
    field: &'static str,
    part_index: usize,
    image_url: String,
}

pub async fn apply_vision_bridge(
    client: &Client,
    route: &ResolvedRoute,
    body: &mut Value,
) -> usize {
    let Some(config) = route.vision_bridge.as_ref() else {
        return 0;
    };

    let targets = collect_image_targets(body);
    if targets.is_empty() {
        return 0;
    }

    let mut replaced = 0usize;
    for target in targets {
        let description = describe_image(client, config, &target.image_url)
            .await
            .unwrap_or_else(|| VISION_FAILURE_MARKER.to_string());
        if replace_target_with_description(body, &target, &description) {
            replaced += 1;
        }
    }
    replaced
}

pub fn replace_images_with_vision_bridge_marker(body: &mut Value, description: &str) -> usize {
    let targets = collect_image_targets(body);
    if targets.is_empty() {
        return 0;
    }

    let mut replaced = 0usize;
    for target in targets {
        if replace_target_with_description(body, &target, description) {
            replaced += 1;
        }
    }
    replaced
}

fn collect_image_targets(body: &Value) -> Vec<ImageTarget> {
    let Some(input) = body.get("input").and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    let mut targets = Vec::new();
    for (message_index, item) in input.iter().enumerate() {
        if !should_process_item(item) {
            continue;
        }
        for field in ["content", "output"] {
            let Some(parts) = item.get(field).and_then(|value| value.as_array()) else {
                continue;
            };
            for (part_index, part) in parts.iter().enumerate() {
                if let Some(image_url) = image_part_url(part) {
                    targets.push(ImageTarget {
                        message_index,
                        field,
                        part_index,
                        image_url,
                    });
                }
            }
        }
    }
    targets
}

fn should_process_item(item: &Value) -> bool {
    let item_type = item.get("type").and_then(|value| value.as_str());
    if matches!(
        item_type,
        Some("function_call_output" | "tool_search_output")
    ) {
        return true;
    }
    item.get("role")
        .and_then(|value| value.as_str())
        .map(|role| role == "user")
        .unwrap_or(true)
}

fn image_part_url(part: &Value) -> Option<String> {
    if let Some(image_url) = part.get("image_url") {
        let url = if let Some(url) = image_url.as_str() {
            url
        } else {
            image_url.get("url").and_then(|value| value.as_str())?
        };
        return Some(url.to_string()).filter(|value| !value.trim().is_empty());
    }

    if let Some(source) = part.get("source") {
        let source_type = source
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if source_type == "base64" {
            let media_type = source
                .get("media_type")
                .and_then(|value| value.as_str())
                .unwrap_or("image/png");
            let data = source.get("data").and_then(|value| value.as_str())?;
            return Some(format!("data:{media_type};base64,{data}"));
        }
    }

    let file_data = part.get("file_data").and_then(|value| value.as_str())?;
    if file_data.starts_with("data:image/") {
        Some(file_data.to_string())
    } else {
        Some(format!("data:image/png;base64,{file_data}"))
    }
}

async fn describe_image(
    client: &Client,
    config: &VisionBridgeConfig,
    image_url: &str,
) -> Option<String> {
    let url = match build_upstream_url(&config.base_url, "/chat/completions") {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(error = %error, "vision bridge base_url invalid");
            return None;
        }
    };

    let mut request = client
        .post(url)
        .timeout(Duration::from_secs(30))
        .json(&json!({
            "model": config.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": VISION_PROMPT },
                        { "type": "image_url", "image_url": { "url": image_url } }
                    ]
                }
            ],
            "stream": false,
            "max_tokens": 1024
        }));

    if let Some(env_key) = config.env_key.as_deref().filter(|value| !value.is_empty()) {
        match std::env::var(env_key) {
            Ok(api_key) if !api_key.trim().is_empty() => {
                request = request.bearer_auth(api_key);
            }
            _ if !is_local_base_url(&config.base_url) => {
                tracing::warn!(
                    env_key = %env_key,
                    "vision bridge env var missing; skip remote vision request"
                );
                return None;
            }
            _ => {}
        }
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = %error, "vision bridge request failed");
            return None;
        }
    };

    if !response.status().is_success() {
        tracing::warn!(
            status = %response.status().as_u16(),
            "vision bridge upstream returned non-success status"
        );
        return None;
    }

    let value = match response.json::<Value>().await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(error = %error, "vision bridge response JSON parse failed");
            return None;
        }
    };
    extract_chat_message_text(&value).filter(|text| !text.trim().is_empty())
}

fn extract_chat_message_text(value: &Value) -> Option<String> {
    let content = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))?;

    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    let parts = content
        .as_array()?
        .iter()
        .filter_map(|part| {
            part.get("text")
                .or_else(|| part.get("input_text"))
                .or_else(|| part.get("output_text"))
                .and_then(|value| value.as_str())
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn replace_target_with_description(
    body: &mut Value,
    target: &ImageTarget,
    description: &str,
) -> bool {
    let Some(part) = body
        .get_mut("input")
        .and_then(|value| value.as_array_mut())
        .and_then(|input| input.get_mut(target.message_index))
        .and_then(|item| item.get_mut(target.field))
        .and_then(|parts| parts.as_array_mut())
        .and_then(|parts| parts.get_mut(target.part_index))
    else {
        return false;
    };

    *part = json!({
        "type": "input_text",
        "text": format!("\n[截图描述: {}]\n", description.trim()),
    });
    true
}

fn is_local_base_url(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.contains("127.0.0.1") || lower.contains("localhost")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::routing::VisionBridgeConfig;
    use axum::routing::post;
    use axum::{Json, Router};

    #[tokio::test]
    async fn vision_bridge_replaces_user_image_with_description() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/v1/chat/completions", post(mock_vision));
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let mut body = json!({
            "model": "minimax-m3",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看图" },
                        { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                    ]
                }
            ]
        });
        let route = ResolvedRoute {
            provider_name: "minimax".to_string(),
            model_id: "MiniMax-M3".to_string(),
            upstream_base_url: "https://api.minimaxi.com/v1".to_string(),
            wire_api: "chat".to_string(),
            auth_source: None,
            env_key: None,
            api_key: None,
            http_headers: Default::default(),
            chat_reasoning: None,
            text_only: true,
            vision_bridge: Some(VisionBridgeConfig {
                base_url: format!("http://{addr}/v1"),
                model: "vision-test".to_string(),
                env_key: None,
            }),
        };

        let replaced = apply_vision_bridge(&Client::new(), &route, &mut body).await;
        let content = body["input"][0]["content"].as_array().unwrap();

        assert_eq!(replaced, 1);
        assert_eq!(content[1]["type"], "input_text");
        assert_eq!(content[1]["text"], "\n[截图描述: 截图里有错误提示]\n");
    }

    async fn mock_vision(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["model"], "vision-test");
        assert_eq!(
            body["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,abc"
        );
        Json(json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "截图里有错误提示"
                    }
                }
            ]
        }))
    }
}
