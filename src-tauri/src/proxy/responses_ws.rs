// src-tauri/src/proxy/responses_ws.rs
//
// Codex Desktop 会对 /v1/responses 发起 WebSocket upgrade。这里负责把
// Responses WebSocket 的 response.create 消息转换为 OpenAI-compatible
// /chat/completions 请求，并把上游 chat SSE 转回 Responses 事件。

use crate::error::{AppError, AppResult};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequestParts {
    pub body: serde_json::Value,
}

pub fn responses_create_to_chat_request(
    message: &serde_json::Value,
    upstream_model: &str,
) -> AppResult<ChatRequestParts> {
    let obj = message.as_object().ok_or_else(|| {
        AppError::Command("response.create message must be a JSON object".to_string())
    })?;

    let mut messages = Vec::new();
    if let Some(instructions) = obj.get("instructions").and_then(content_to_text) {
        if !instructions.trim().is_empty() {
            messages.push(json!({ "role": "system", "content": instructions }));
        }
    }

    if let Some(input) = obj.get("input") {
        append_input_messages(input, &mut messages);
    }

    if messages.is_empty() {
        messages.push(json!({ "role": "user", "content": " " }));
    }

    let mut body = json!({
        "model": upstream_model,
        "messages": messages,
        "stream": message.get("stream").and_then(|v| v.as_bool()).unwrap_or(true),
    });

    copy_if_present(message, &mut body, "temperature");
    copy_if_present(message, &mut body, "top_p");
    copy_if_present(message, &mut body, "parallel_tool_calls");
    copy_if_present(message, &mut body, "reasoning_effort");
    if let Some(max_output_tokens) = message.get("max_output_tokens") {
        body["max_tokens"] = max_output_tokens.clone();
    } else {
        copy_if_present(message, &mut body, "max_tokens");
    }

    Ok(ChatRequestParts { body })
}

pub fn chat_sse_line_to_response_events(line: &str) -> Vec<serde_json::Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed == "data: [DONE]" || trimmed == "[DONE]" {
        return vec![json!({ "type": "response.completed" })];
    }
    let payload = trimmed.strip_prefix("data: ").unwrap_or(trimmed);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Vec::new();
    };
    let Some(choice) = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
    else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(text) = choice
        .get("delta")
        .and_then(|delta| {
            delta
                .get("reasoning_content")
                .or_else(|| delta.get("reasoning"))
        })
        .and_then(|value| value.as_str())
        .filter(|text| !text.is_empty())
    {
        events.push(json!({
            "type": "response.reasoning_summary_text.delta",
            "delta": text,
        }));
    }
    if let Some(text) = choice
        .get("delta")
        .and_then(|delta| delta.get("content"))
        .and_then(|value| value.as_str())
        .filter(|text| !text.is_empty())
    {
        events.push(json!({
            "type": "response.output_text.delta",
            "delta": text,
        }));
    }
    if choice
        .get("finish_reason")
        .and_then(|value| value.as_str())
        .is_some()
    {
        events.push(json!({ "type": "response.completed" }));
    }
    events
}

pub struct ResponsesWsStreamState {
    response_id: String,
    message_id: String,
    model: String,
    opened_message: bool,
    completed: bool,
    text: String,
}

impl ResponsesWsStreamState {
    pub fn new(model: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            response_id: format!("resp_{now}"),
            message_id: format!("msg_{now}"),
            model: model.into(),
            opened_message: false,
            completed: false,
            text: String::new(),
        }
    }

    pub fn started_event(&self) -> serde_json::Value {
        json!({
            "type": "response.created",
            "response": self.response("in_progress", false),
        })
    }

    pub fn ingest_sse_line(&mut self, line: &str) -> Vec<serde_json::Value> {
        let mut output = Vec::new();
        for event in chat_sse_line_to_response_events(line) {
            match event.get("type").and_then(|value| value.as_str()) {
                Some("response.output_text.delta") => {
                    let delta = event
                        .get("delta")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if !self.opened_message {
                        output.extend(self.open_message_events());
                    }
                    self.text.push_str(delta);
                    output.push(json!({
                        "type": "response.output_text.delta",
                        "item_id": self.message_id,
                        "output_index": 0,
                        "content_index": 0,
                        "delta": delta,
                    }));
                }
                Some("response.completed") => {
                    output.extend(self.finish_events());
                }
                _ => output.push(event),
            }
        }
        output
    }

    pub fn finish_events(&mut self) -> Vec<serde_json::Value> {
        if self.completed {
            return Vec::new();
        }
        self.completed = true;
        let mut output = Vec::new();
        if !self.opened_message {
            output.extend(self.open_message_events());
        }
        output.push(json!({
            "type": "response.output_text.done",
            "item_id": self.message_id,
            "output_index": 0,
            "content_index": 0,
            "text": self.text,
        }));
        output.push(json!({
            "type": "response.content_part.done",
            "item_id": self.message_id,
            "output_index": 0,
            "content_index": 0,
            "part": { "type": "output_text", "text": self.text, "annotations": [] },
        }));
        output.push(json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": self.message_item("completed"),
        }));
        output.push(json!({
            "type": "response.completed",
            "response": self.response("completed", true),
        }));
        output
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    fn open_message_events(&mut self) -> Vec<serde_json::Value> {
        self.opened_message = true;
        vec![
            json!({
                "type": "response.output_item.added",
                "output_index": 0,
                "item": {
                    "id": self.message_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                },
            }),
            json!({
                "type": "response.content_part.added",
                "item_id": self.message_id,
                "output_index": 0,
                "content_index": 0,
                "part": { "type": "output_text", "text": "", "annotations": [] },
            }),
        ]
    }

    fn message_item(&self, status: &str) -> serde_json::Value {
        json!({
            "id": self.message_id,
            "type": "message",
            "status": status,
            "role": "assistant",
            "content": [{ "type": "output_text", "text": self.text, "annotations": [] }],
        })
    }

    fn response(&self, status: &str, final_output: bool) -> serde_json::Value {
        let output = if final_output {
            vec![self.message_item("completed")]
        } else {
            Vec::new()
        };
        json!({
            "id": self.response_id,
            "object": "response",
            "created_at": now_seconds(),
            "status": status,
            "model": self.model,
            "output": output,
        })
    }
}

fn append_input_messages(input: &serde_json::Value, messages: &mut Vec<serde_json::Value>) {
    if let Some(text) = input.as_str() {
        if !text.trim().is_empty() {
            messages.push(json!({ "role": "user", "content": text }));
        }
        return;
    }

    let Some(items) = input.as_array() else {
        return;
    };
    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if item_type == "reasoning" {
            continue;
        }

        if item_type == "function_call_output" {
            let content = item
                .get("output")
                .and_then(content_to_text)
                .unwrap_or_else(|| " ".to_string());
            messages.push(json!({ "role": "tool", "content": content }));
            continue;
        }

        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("user");
        let role = match role {
            "assistant" => "assistant",
            "system" => "system",
            "tool" => "tool",
            _ => "user",
        };
        let content = item
            .get("content")
            .and_then(content_to_text)
            .unwrap_or_else(|| " ".to_string());
        messages.push(json!({ "role": role, "content": content }));
    }
}

fn content_to_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(array) = value.as_array() {
        let parts = array
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .or_else(|| part.get("input_text"))
                    .or_else(|| part.get("output_text"))
                    .and_then(|value| value.as_str())
            })
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return None;
        }
        return Some(parts.join(""));
    }
    if value.is_object() {
        return value
            .get("text")
            .or_else(|| value.get("input_text"))
            .or_else(|| value.get("output_text"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
    }
    None
}

fn copy_if_present(source: &serde_json::Value, target: &mut serde_json::Value, key: &str) {
    if let Some(value) = source.get(key) {
        target[key] = value.clone();
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_response_create_to_chat_completion_body() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "minimax",
            "instructions": "你是一个助手",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "你是什么模型？" }
                    ]
                }
            ],
            "stream": true,
            "max_output_tokens": 256
        });

        let chat = responses_create_to_chat_request(&message, "MiniMax-M1").unwrap();
        assert_eq!(chat.body["model"], "MiniMax-M1");
        assert_eq!(chat.body["stream"], true);
        assert_eq!(chat.body["max_tokens"], 256);
        assert_eq!(chat.body["messages"][0]["role"], "system");
        assert_eq!(chat.body["messages"][0]["content"], "你是一个助手");
        assert_eq!(chat.body["messages"][1]["role"], "user");
        assert_eq!(chat.body["messages"][1]["content"], "你是什么模型？");
    }

    #[test]
    fn converts_chat_sse_delta_to_responses_output_text_delta() {
        let events =
            chat_sse_line_to_response_events(r#"data: {"choices":[{"delta":{"content":"你好"}}]}"#);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "response.output_text.delta");
        assert_eq!(events[0]["delta"], "你好");
    }

    #[test]
    fn converts_chat_sse_done_to_response_completed() {
        let events = chat_sse_line_to_response_events("data: [DONE]");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "response.completed");
    }
}
