// src-tauri/src/proxy/responses_ws.rs
//
// Codex Desktop 会对 /v1/responses 发起 WebSocket upgrade。这里负责把
// Responses WebSocket 的 response.create 消息转换为 OpenAI-compatible
// /chat/completions 请求，并把上游 chat SSE 转回 Responses 事件。

use crate::error::{AppError, AppResult};
use crate::proxy::routing::ChatReasoningConfig;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

const CUSTOM_TOOL_INPUT_FIELD: &str = "input";
const CUSTOM_TOOL_INPUT_DESCRIPTION: &str =
    "Raw string input for the original custom tool. Preserve formatting exactly and follow the original tool definition embedded in the description.";
const CUSTOM_TOOL_PRESERVED_METADATA_HEADING: &str = "Original tool definition:";

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequestParts {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChatRequestOptions<'a> {
    pub chat_reasoning: Option<&'a ChatReasoningConfig>,
    pub text_only_input: bool,
}

pub fn responses_create_to_chat_request(
    message: &serde_json::Value,
    upstream_model: &str,
) -> AppResult<ChatRequestParts> {
    responses_create_to_chat_request_with_options(
        message,
        upstream_model,
        ChatRequestOptions::default(),
    )
}

pub fn responses_create_to_chat_request_with_reasoning(
    message: &serde_json::Value,
    upstream_model: &str,
    chat_reasoning: Option<&ChatReasoningConfig>,
) -> AppResult<ChatRequestParts> {
    responses_create_to_chat_request_with_options(
        message,
        upstream_model,
        ChatRequestOptions {
            chat_reasoning,
            text_only_input: false,
        },
    )
}

pub fn responses_create_to_chat_request_with_options(
    message: &serde_json::Value,
    upstream_model: &str,
    options: ChatRequestOptions<'_>,
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
        append_input_messages(
            input,
            &mut messages,
            upstream_model,
            options.text_only_input,
        );
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
    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        copy_if_present(message, &mut body, key);
    }
    if let Some(max_output_tokens) = message.get("max_output_tokens") {
        body["max_tokens"] = max_output_tokens.clone();
    } else {
        copy_if_present(message, &mut body, "max_tokens");
    }

    let mut tools = responses_tools_to_chat_tools(message.get("tools"));
    collect_tool_search_output_tools(message.get("input"), &mut tools);
    dedup_chat_tools_by_name(&mut tools);
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        if let Some(tool_choice) = message.get("tool_choice") {
            body["tool_choice"] = responses_tool_choice_to_chat(tool_choice);
        }
    } else if let Some(obj) = body.as_object_mut() {
        obj.remove("tool_choice");
        obj.remove("parallel_tool_calls");
    }
    inject_stream_include_usage(&mut body);
    apply_chat_reasoning_compat(message, &mut body, upstream_model, options.chat_reasoning);

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

pub fn chat_completion_json_to_response_events(
    value: &serde_json::Value,
    model: &str,
) -> Vec<serde_json::Value> {
    let mut stream_state = ResponsesWsStreamState::new(model);
    let mut events = vec![stream_state.started_event()];
    events.extend(stream_state.ingest_chat_completion_json(value));
    events
}

fn chat_completion_choice_to_synthetic_delta(choice: &serde_json::Value) -> serde_json::Value {
    let message = choice.get("message").unwrap_or(&serde_json::Value::Null);
    let mut delta = serde_json::Map::new();
    if let Some(reasoning) = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(|value| value.as_str())
        .filter(|text| !text.is_empty())
    {
        delta.insert(
            "reasoning_content".to_string(),
            serde_json::Value::String(reasoning.to_string()),
        );
    }
    if let Some(content) = message
        .get("content")
        .and_then(|content| content.as_str())
        .filter(|text| !text.is_empty())
    {
        delta.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
        let normalized = tool_calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                let mut call = call.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.entry("index".to_string())
                        .or_insert_with(|| serde_json::json!(index));
                }
                call
            })
            .collect::<Vec<_>>();
        if !normalized.is_empty() {
            delta.insert(
                "tool_calls".to_string(),
                serde_json::Value::Array(normalized),
            );
        }
    }

    if delta.is_empty() {
        delta.insert(
            "content".to_string(),
            serde_json::Value::String(" ".to_string()),
        );
    }

    let finish_reason = choice
        .get("finish_reason")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::String("stop".to_string()));
    let synthetic = json!({
        "choices": [{ "delta": serde_json::Value::Object(delta), "finish_reason": finish_reason }]
    });
    synthetic
}

pub fn chat_completion_json_to_response_body(
    value: &serde_json::Value,
    model: &str,
) -> serde_json::Value {
    chat_completion_json_to_response_events(value, model)
        .into_iter()
        .rev()
        .find(|event| {
            event.get("type").and_then(|value| value.as_str()) == Some("response.completed")
        })
        .and_then(|event| event.get("response").cloned())
        .unwrap_or_else(|| {
            json!({
                "id": format!("resp_{}", now_millis()),
                "object": "response",
                "created_at": now_seconds(),
                "status": "completed",
                "model": model,
                "output": [],
            })
        })
}

pub fn response_events_to_sse(events: &[serde_json::Value]) -> String {
    events
        .iter()
        .map(|event| String::from_utf8_lossy(&response_event_to_sse_bytes(event)).to_string())
        .collect::<String>()
}

pub fn response_event_to_sse_bytes(event: &serde_json::Value) -> axum::body::Bytes {
    let event_type = event
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("message");
    axum::body::Bytes::from(format!("event: {event_type}\ndata: {event}\n\n"))
}

pub struct ResponsesWsStreamState {
    response_id: String,
    message_id: String,
    model: String,
    message_index: Option<usize>,
    opened_message: bool,
    message_closed: bool,
    completed: bool,
    text: String,
    think_filter: ThinkTagFilter,
    tool_markup_filter: MiniMaxToolMarkupFilter,
    next_output_index: usize,
    tool_calls: BTreeMap<usize, ToolCallState>,
    reasoning_blocks: BTreeMap<String, ReasoningState>,
    usage: Option<Value>,
}

impl ResponsesWsStreamState {
    pub fn new(model: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            response_id: format!("resp_{now}"),
            message_id: format!("msg_{now}"),
            model: model.into(),
            message_index: None,
            opened_message: false,
            message_closed: false,
            completed: false,
            text: String::new(),
            think_filter: ThinkTagFilter::default(),
            tool_markup_filter: MiniMaxToolMarkupFilter::default(),
            next_output_index: 0,
            tool_calls: BTreeMap::new(),
            reasoning_blocks: BTreeMap::new(),
            usage: None,
        }
    }

    pub fn started_event(&self) -> serde_json::Value {
        json!({
            "type": "response.created",
            "response": self.response("in_progress", false),
        })
    }

    pub fn ingest_chat_completion_json(
        &mut self,
        value: &serde_json::Value,
    ) -> Vec<serde_json::Value> {
        let choice = value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let synthetic = chat_completion_choice_to_synthetic_delta(&choice);
        let line = format!("data: {synthetic}");

        self.set_usage(value.get("usage").cloned());
        let mut events = self.ingest_sse_line(&line);
        if !self.is_completed() {
            events.extend(self.finish_events());
        }
        events
    }

    pub fn ingest_sse_line(&mut self, line: &str) -> Vec<serde_json::Value> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        if trimmed == "data: [DONE]" || trimmed == "[DONE]" {
            return self.finish_events();
        }

        let payload = trimmed.strip_prefix("data: ").unwrap_or(trimmed);
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            return Vec::new();
        };
        if let Some(usage) = value.get("usage").filter(|value| value.is_object()) {
            self.usage = Some(usage.clone());
        }
        let Some(choice) = value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first())
        else {
            return Vec::new();
        };
        let delta = choice.get("delta").cloned().unwrap_or_else(|| json!({}));

        let mut output = Vec::new();
        if let Some(reasoning) = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|value| value.as_str())
            .filter(|text| !text.is_empty())
        {
            output.extend(self.chat_reasoning_delta(reasoning));
        }

        if let Some(content) = delta
            .get("content")
            .and_then(|value| value.as_str())
            .filter(|text| !text.is_empty())
        {
            output.extend(self.close_open_reasoning_events());
            let filtered = self.think_filter.filter(content);
            if !filtered.is_empty() {
                output.extend(self.content_delta(&filtered));
            }
        }

        if let Some(calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
            for call in calls {
                output.extend(self.chat_tool_delta(call));
            }
        }

        if choice
            .get("finish_reason")
            .and_then(|value| value.as_str())
            .is_some()
        {
            output.extend(self.finish_events());
        }
        output
    }

    pub fn finish_events(&mut self) -> Vec<serde_json::Value> {
        if self.completed {
            return Vec::new();
        }
        self.completed = true;
        let mut output = Vec::new();
        output.extend(self.close_open_reasoning_events());
        if self.opened_message && !self.message_closed {
            let pending = self.tool_markup_filter.flush_text();
            if !pending.is_empty() {
                output.extend(self.text_delta(&pending));
            }
            output.extend(self.close_message_events());
        }

        let keys = self.tool_calls.keys().copied().collect::<Vec<_>>();
        for key in keys {
            output.extend(self.close_tool_events(key));
        }

        if self.response_output_items().is_empty() {
            output.extend(self.text_delta(" "));
            output.extend(self.close_message_events());
        }
        output.push(json!({
            "type": "response.completed",
            "response": self.response("completed", true),
        }));
        output
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn set_usage(&mut self, usage: Option<Value>) {
        if let Some(usage) = usage.filter(|value| value.is_object()) {
            self.usage = Some(usage);
        }
    }

    pub fn assistant_message(&self) -> serde_json::Value {
        let tool_calls = self
            .tool_calls
            .values()
            .map(|state| {
                json!({
                    "id": state.id,
                    "type": "function",
                    "function": {
                        "name": state.name,
                        "arguments": state.arguments,
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut message = json!({
            "role": "assistant",
            "content": if self.text.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(self.text.clone())
            },
        });
        if !tool_calls.is_empty() {
            message["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        message
    }

    fn open_message_events(&mut self) -> Vec<serde_json::Value> {
        if self.opened_message {
            return Vec::new();
        }
        let output_index = self.next_output_index;
        self.next_output_index += 1;
        self.message_index = Some(output_index);
        self.opened_message = true;
        vec![
            json!({
                "type": "response.output_item.added",
                "output_index": output_index,
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
                "output_index": output_index,
                "content_index": 0,
                "part": { "type": "output_text", "text": "", "annotations": [] },
            }),
        ]
    }

    fn close_message_events(&mut self) -> Vec<serde_json::Value> {
        if !self.opened_message || self.message_closed {
            return Vec::new();
        }
        self.message_closed = true;
        let output_index = self.message_index.unwrap_or(0);
        vec![
            json!({
                "type": "response.output_text.done",
                "item_id": self.message_id,
                "output_index": output_index,
                "content_index": 0,
                "text": self.text,
            }),
            json!({
                "type": "response.content_part.done",
                "item_id": self.message_id,
                "output_index": output_index,
                "content_index": 0,
                "part": { "type": "output_text", "text": self.text, "annotations": [] },
            }),
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": self.message_item("completed"),
            }),
        ]
    }

    fn text_delta(&mut self, text: &str) -> Vec<serde_json::Value> {
        let mut output = Vec::new();
        if !self.opened_message {
            output.extend(self.open_message_events());
        }
        self.text.push_str(text);
        output.push(json!({
            "type": "response.output_text.delta",
            "item_id": self.message_id,
            "output_index": self.message_index.unwrap_or(0),
            "content_index": 0,
            "delta": text,
        }));
        output
    }

    fn content_delta(&mut self, text: &str) -> Vec<serde_json::Value> {
        let mut output = Vec::new();
        for piece in self.tool_markup_filter.push(text) {
            match piece {
                ContentPiece::Text(text) if !text.is_empty() => {
                    output.extend(self.text_delta(&text));
                }
                ContentPiece::Tool { name, arguments } => {
                    output.extend(self.complete_text_tool_call(&name, &arguments));
                }
                ContentPiece::Text(_) => {}
            }
        }
        output
    }

    fn chat_reasoning_delta(&mut self, text: &str) -> Vec<serde_json::Value> {
        let mut output = Vec::new();
        if !self.reasoning_blocks.contains_key("chat_reasoning") {
            output.extend(self.open_reasoning_events("chat_reasoning"));
        }
        let state = self.reasoning_blocks.get_mut("chat_reasoning").unwrap();
        state.text.push_str(text);
        output.push(json!({
            "type": "response.reasoning_text.delta",
            "item_id": state.id,
            "output_index": state.output_index,
            "content_index": 0,
            "delta": text,
        }));
        output.push(json!({
            "type": "response.reasoning_summary_text.delta",
            "item_id": state.id,
            "output_index": state.output_index,
            "summary_index": 0,
            "delta": text,
        }));
        output
    }

    fn open_reasoning_events(&mut self, key: &str) -> Vec<serde_json::Value> {
        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let id = format!("rs_{}_{}", now_millis(), output_index);
        self.reasoning_blocks.insert(
            key.to_string(),
            ReasoningState {
                id: id.clone(),
                output_index,
                text: String::new(),
                closed: false,
            },
        );
        vec![
            json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": {
                    "id": id,
                    "type": "reasoning",
                    "status": "in_progress",
                    "summary": [],
                    "content": [],
                    "encrypted_content": null,
                },
            }),
            json!({
                "type": "response.content_part.added",
                "item_id": self.reasoning_blocks.get(key).unwrap().id,
                "output_index": output_index,
                "content_index": 0,
                "part": { "type": "reasoning_text", "text": "" },
            }),
        ]
    }

    fn close_open_reasoning_events(&mut self) -> Vec<serde_json::Value> {
        let keys = self
            .reasoning_blocks
            .iter()
            .filter_map(|(key, state)| (!state.closed).then_some(key.clone()))
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        for key in keys {
            output.extend(self.close_reasoning_events(&key));
        }
        output
    }

    fn close_reasoning_events(&mut self, key: &str) -> Vec<serde_json::Value> {
        let Some(state) = self.reasoning_blocks.get_mut(key) else {
            return Vec::new();
        };
        if state.closed {
            return Vec::new();
        }
        state.closed = true;
        let item = reasoning_item(state, "completed");
        vec![
            json!({
                "type": "response.reasoning_text.done",
                "item_id": state.id,
                "output_index": state.output_index,
                "content_index": 0,
                "text": state.text,
            }),
            json!({
                "type": "response.reasoning_summary_text.done",
                "item_id": state.id,
                "output_index": state.output_index,
                "summary_index": 0,
                "text": state.text,
            }),
            json!({
                "type": "response.output_item.done",
                "output_index": state.output_index,
                "item": item,
            }),
        ]
    }

    fn chat_tool_delta(&mut self, call: &serde_json::Value) -> Vec<serde_json::Value> {
        let index = call
            .get("index")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        let function = call.get("function").unwrap_or(&serde_json::Value::Null);
        let id = call
            .get("id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let name_delta = function
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let arg_delta = function
            .get("arguments")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        let state = self.tool_calls.entry(index).or_insert_with(|| {
            let call_id = id.clone().unwrap_or_else(|| format!("call_{index}"));
            ToolCallState {
                id: call_id.clone(),
                call_id,
                name: String::new(),
                arguments: String::new(),
                added: false,
                closed: false,
                output_index: None,
            }
        });
        if let Some(id) = id {
            state.id = id.clone();
            state.call_id = id;
        }
        if !name_delta.is_empty() {
            state.name.push_str(name_delta);
        }

        let mut output = Vec::new();
        if !state.name.is_empty() && !state.added {
            output.extend(self.open_tool_events(index));
        }
        if !arg_delta.is_empty() {
            let state = self.tool_calls.get_mut(&index).unwrap();
            state.arguments.push_str(arg_delta);
            if state.added {
                output.push(json!({
                    "type": "response.function_call_arguments.delta",
                    "item_id": state.id,
                    "output_index": state.output_index.unwrap_or(0),
                    "delta": arg_delta,
                }));
            }
        }
        output
    }

    fn open_tool_events(&mut self, index: usize) -> Vec<serde_json::Value> {
        if self
            .tool_calls
            .get(&index)
            .map(|s| s.added)
            .unwrap_or(false)
        {
            return Vec::new();
        }
        let mut output = Vec::new();
        if self.opened_message && !self.message_closed {
            output.extend(self.close_message_events());
        }
        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let state = self.tool_calls.get_mut(&index).unwrap();
        state.added = true;
        state.output_index = Some(output_index);
        output.push(json!({
            "type": "response.output_item.added",
            "output_index": output_index,
            "item": {
                "id": state.id,
                "type": "function_call",
                "status": "in_progress",
                "call_id": state.call_id,
                "name": state.name,
                "arguments": "",
            },
        }));
        output
    }

    fn close_tool_events(&mut self, index: usize) -> Vec<serde_json::Value> {
        let mut output = self.open_tool_events(index);
        let Some(state) = self.tool_calls.get_mut(&index) else {
            return output;
        };
        if state.closed {
            return output;
        }
        state.closed = true;
        output.push(json!({
            "type": "response.function_call_arguments.done",
            "item_id": state.id,
            "output_index": state.output_index.unwrap_or(0),
            "arguments": state.arguments,
        }));
        output.push(json!({
            "type": "response.output_item.done",
            "output_index": state.output_index.unwrap_or(0),
            "item": tool_item(state, "completed"),
        }));
        output
    }

    fn complete_text_tool_call(&mut self, name: &str, arguments: &str) -> Vec<serde_json::Value> {
        let index = self
            .tool_calls
            .keys()
            .next_back()
            .map(|index| index + 1)
            .unwrap_or(0);
        let call_id = format!("call_{}_{}", now_millis(), index);
        self.tool_calls.insert(
            index,
            ToolCallState {
                id: call_id.clone(),
                call_id,
                name: name.to_string(),
                arguments: arguments.to_string(),
                added: false,
                closed: false,
                output_index: None,
            },
        );

        let mut output = self.close_open_reasoning_events();
        output.extend(self.open_tool_events(index));
        if !arguments.is_empty() {
            let state = self.tool_calls.get(&index).unwrap();
            output.push(json!({
                "type": "response.function_call_arguments.delta",
                "item_id": state.id,
                "output_index": state.output_index.unwrap_or(0),
                "delta": arguments,
            }));
        }
        output.extend(self.close_tool_events(index));
        output
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
            let items = self.response_output_items();
            if items.is_empty() {
                vec![self.message_item("completed")]
            } else {
                items
            }
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
            "usage": chat_usage_to_responses_usage(self.usage.as_ref()),
        })
    }

    fn response_output_items(&self) -> Vec<serde_json::Value> {
        let mut collected = Vec::new();
        for state in self.reasoning_blocks.values() {
            collected.push((state.output_index, reasoning_item(state, "completed")));
        }
        if self.opened_message && self.message_index.is_some() && !self.text.is_empty() {
            collected.push((self.message_index.unwrap(), self.message_item("completed")));
        }
        for state in self.tool_calls.values() {
            if let Some(output_index) = state.output_index {
                collected.push((output_index, tool_item(state, "completed")));
            }
        }
        collected.sort_by_key(|(index, _)| *index);
        collected.into_iter().map(|(_, item)| item).collect()
    }
}

#[derive(Debug, Clone)]
struct ReasoningState {
    id: String,
    output_index: usize,
    text: String,
    closed: bool,
}

#[derive(Debug, Clone)]
struct ToolCallState {
    id: String,
    call_id: String,
    name: String,
    arguments: String,
    added: bool,
    closed: bool,
    output_index: Option<usize>,
}

fn reasoning_item(state: &ReasoningState, status: &str) -> serde_json::Value {
    json!({
        "id": state.id,
        "type": "reasoning",
        "status": status,
        "summary": if state.text.is_empty() {
            Vec::<serde_json::Value>::new()
        } else {
            vec![json!({ "type": "summary_text", "text": state.text })]
        },
        "content": [],
        "encrypted_content": serde_json::Value::Null,
    })
}

fn tool_item(state: &ToolCallState, status: &str) -> serde_json::Value {
    json!({
        "id": state.id,
        "type": "function_call",
        "status": status,
        "call_id": state.call_id,
        "name": state.name,
        "arguments": state.arguments,
    })
}

#[derive(Debug, Default)]
pub struct ResponseEventPassthroughNormalizer {
    think_filter: ThinkTagFilter,
}

impl ResponseEventPassthroughNormalizer {
    pub fn normalize_event(&mut self, mut event: serde_json::Value) -> Option<serde_json::Value> {
        if event.get("type").and_then(|value| value.as_str()) == Some("response.output_text.delta")
        {
            let delta = event
                .get("delta")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let filtered = self.think_filter.filter(delta);
            if filtered.is_empty() {
                return None;
            }
            if let Some(obj) = event.as_object_mut() {
                obj.insert("delta".to_string(), serde_json::Value::String(filtered));
            }
        }

        if event.get("type").and_then(|value| value.as_str()) == Some("response.output_text.done") {
            if let Some(text) = event.get("text").and_then(|value| value.as_str()) {
                let stripped = strip_complete_think_tags(text);
                if let Some(obj) = event.as_object_mut() {
                    obj.insert("text".to_string(), serde_json::Value::String(stripped));
                }
            }
        }

        Some(event)
    }
}

#[derive(Debug, Default)]
struct ThinkTagFilter {
    in_think: bool,
    pending: String,
}

fn strip_complete_think_tags(input: &str) -> String {
    let mut text = input.to_string();
    let mut output = String::new();
    loop {
        let Some(start) = find_ascii_case_insensitive(&text, "<think>") else {
            output.push_str(&text);
            return output;
        };
        output.push_str(&text[..start]);
        let after_start = start + "<think>".len();
        let rest = &text[after_start..];
        let Some(end) = find_ascii_case_insensitive(rest, "</think>") else {
            return output;
        };
        text = rest[end + "</think>".len()..].to_string();
    }
}

impl ThinkTagFilter {
    fn filter(&mut self, input: &str) -> String {
        let mut text = String::new();
        text.push_str(&self.pending);
        text.push_str(&normalize_think_aliases(input));
        self.pending.clear();

        let mut output = String::new();
        loop {
            if self.in_think {
                if let Some(end) = find_ascii_case_insensitive(&text, "</think>") {
                    let after = end + "</think>".len();
                    text = text[after..].to_string();
                    self.in_think = false;
                    continue;
                }
                self.pending = keep_possible_tag_suffix(&text, "</think>");
                return output;
            }

            if let Some(start) = find_ascii_case_insensitive(&text, "<think>") {
                if let Some(end) = find_ascii_case_insensitive(&text, "</think>") {
                    if end < start {
                        output.push_str(&text[..end]);
                        text = text[end + "</think>".len()..].to_string();
                        continue;
                    }
                }
                output.push_str(&text[..start]);
                let after = start + "<think>".len();
                text = text[after..].to_string();
                self.in_think = true;
                continue;
            }

            if let Some(end) = find_ascii_case_insensitive(&text, "</think>") {
                output.push_str(&text[..end]);
                text = text[end + "</think>".len()..].to_string();
                continue;
            }

            let keep = keep_possible_tag_suffix(&text, "<think>");
            if keep.is_empty() {
                output.push_str(&text);
            } else {
                let emit_len = text.len().saturating_sub(keep.len());
                output.push_str(&text[..emit_len]);
                self.pending = keep;
            }
            return output;
        }
    }
}

fn normalize_think_aliases(input: &str) -> String {
    input
        .replace("<mm:think>", "<think>")
        .replace("</mm:think>", "</think>")
        .replace("<MM:THINK>", "<think>")
        .replace("</MM:THINK>", "</think>")
}

#[derive(Debug, Default)]
struct MiniMaxToolMarkupFilter {
    in_tool: bool,
    buffer: String,
}

#[derive(Debug, PartialEq)]
enum ContentPiece {
    Text(String),
    Tool { name: String, arguments: String },
}

impl MiniMaxToolMarkupFilter {
    fn push(&mut self, input: &str) -> Vec<ContentPiece> {
        let mut text = normalize_minimax_markup_tokens(input);
        let mut output = Vec::new();

        loop {
            if self.in_tool {
                self.buffer.push_str(&text);
                if let Some(end) = find_ascii_case_insensitive(&self.buffer, "</tool_call>") {
                    let after = end + "</tool_call>".len();
                    let block = self.buffer[..after].to_string();
                    let rest = self.buffer[after..].to_string();
                    self.buffer.clear();
                    self.in_tool = false;
                    if let Some((name, arguments)) = parse_minimax_tool_call(&block) {
                        output.push(ContentPiece::Tool { name, arguments });
                    }
                    text = rest;
                    continue;
                }
                return output;
            }

            if let Some(start) = find_ascii_case_insensitive(&text, "<tool_call>") {
                let before = strip_minimax_markup_noise(&text[..start]);
                if !before.is_empty() {
                    output.push(ContentPiece::Text(before));
                }
                self.buffer.push_str(&text[start..]);
                self.in_tool = true;
                text.clear();
                continue;
            }

            let keep = keep_possible_tag_suffix(&text, "<tool_call>");
            if keep.is_empty() {
                let cleaned = strip_minimax_markup_noise(&text);
                if !cleaned.is_empty() {
                    output.push(ContentPiece::Text(cleaned));
                }
            } else {
                let emit_len = text.len().saturating_sub(keep.len());
                let cleaned = strip_minimax_markup_noise(&text[..emit_len]);
                if !cleaned.is_empty() {
                    output.push(ContentPiece::Text(cleaned));
                }
                self.buffer.push_str(&keep);
                self.in_tool = true;
            }
            return output;
        }
    }

    fn flush_text(&mut self) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }
        let text = strip_minimax_markup_noise(&self.buffer);
        self.buffer.clear();
        self.in_tool = false;
        text
    }
}

fn normalize_minimax_markup_tokens(input: &str) -> String {
    input
        .replace("]<minimax>[", "")
        .replace("<minimax>[", "")
        .replace("]<minimax>", "")
        .replace("<minimax>", "")
        .replace("</minimax>", "")
        .replace("[/minimax]", "")
}

fn strip_minimax_markup_noise(input: &str) -> String {
    normalize_minimax_markup_tokens(input)
        .replace("</tool_call>", "")
        .replace("<tool_call>", "")
        .replace("</mm:think>", "")
        .replace("<mm:think>", "")
}

fn parse_minimax_tool_call(block: &str) -> Option<(String, String)> {
    let block = normalize_minimax_markup_tokens(block);
    let name = extract_first_invoke_name(&block)?;
    let mut fields = BTreeMap::new();
    for (field, value) in extract_invoke_fields(&block) {
        if field != name && !value.contains('<') {
            fields.insert(field, value);
        }
    }
    if let Some(description) = extract_tag_text(&block, "description") {
        fields.insert("description".to_string(), description);
    }
    if fields.is_empty() {
        let fallback = strip_xml_tags(&block).trim().to_string();
        if !fallback.is_empty() {
            fields.insert("input".to_string(), fallback);
        }
    }
    let arguments = serde_json::to_string(&fields).ok()?;
    Some((name, arguments))
}

fn extract_first_invoke_name(input: &str) -> Option<String> {
    let start = find_ascii_case_insensitive(input, "<invoke")?;
    let after = &input[start..];
    extract_name_attr(after)
}

fn extract_invoke_fields(input: &str) -> Vec<(String, String)> {
    let mut rest = input;
    let mut fields = Vec::new();
    while let Some(start) = find_ascii_case_insensitive(rest, "<invoke") {
        let after = &rest[start..];
        let Some(name) = extract_name_attr(after) else {
            break;
        };
        let Some(gt) = after.find('>') else {
            break;
        };
        let content_start = gt + 1;
        let Some(end) = find_ascii_case_insensitive(&after[content_start..], "</invoke>") else {
            break;
        };
        let value = after[content_start..content_start + end]
            .trim()
            .trim_matches(']')
            .trim_matches('[')
            .trim()
            .to_string();
        fields.push((name, value));
        rest = &after["<invoke".len()..];
    }
    fields
}

fn extract_name_attr(input: &str) -> Option<String> {
    let name_pos = find_ascii_case_insensitive(input, "name=")?;
    let after = &input[name_pos + "name=".len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let after_quote = &after[quote.len_utf8()..];
    let end = after_quote.find(quote)?;
    Some(after_quote[..end].trim().to_string())
}

fn extract_tag_text(input: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = find_ascii_case_insensitive(input, &open)?;
    let after_start = start + open.len();
    let end = find_ascii_case_insensitive(&input[after_start..], &close)?;
    Some(
        input[after_start..after_start + end]
            .trim()
            .trim_matches(']')
            .trim_matches('[')
            .trim()
            .to_string(),
    )
}

fn strip_xml_tags(input: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

fn keep_possible_tag_suffix(text: &str, tag: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let tag = tag.to_ascii_lowercase();
    let max = lower.len().min(tag.len().saturating_sub(1));
    for len in (1..=max).rev() {
        if lower.ends_with(&tag[..len]) {
            return text[text.len() - len..].to_string();
        }
    }
    String::new()
}

fn append_input_messages(
    input: &serde_json::Value,
    messages: &mut Vec<serde_json::Value>,
    upstream_model: &str,
    text_only_input: bool,
) {
    if let Some(text) = input.as_str() {
        if !text.trim().is_empty() {
            messages.push(json!({ "role": "user", "content": text }));
        }
        return;
    }

    let Some(items) = input.as_array() else {
        return;
    };
    let mut pending_tool_calls = Vec::new();
    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if item_type == "reasoning" {
            continue;
        }

        if item_type == "function_call" {
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or("call_0");
            pending_tool_calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": item.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                    "arguments": item.get("arguments").and_then(|value| value.as_str()).unwrap_or(""),
                }
            }));
            continue;
        }

        if item_type == "custom_tool_call" {
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or("call_0");
            let input = item.get("input").cloned().unwrap_or_else(|| json!(""));
            pending_tool_calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": item.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                    "arguments": canonical_json_string(&json!({ CUSTOM_TOOL_INPUT_FIELD: input })),
                }
            }));
            continue;
        }

        if item_type == "tool_search_call" {
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or("call_0");
            let arguments = item
                .get("arguments")
                .map(canonical_json_string)
                .unwrap_or_else(|| "{}".to_string());
            pending_tool_calls.push(json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": "tool_search",
                    "arguments": arguments,
                }
            }));
            continue;
        }

        if matches!(
            item_type,
            "function_call_output" | "custom_tool_call_output" | "tool_search_output"
        ) {
            if !pending_tool_calls.is_empty() {
                messages.push(json!({
                    "role": "assistant",
                    "content": serde_json::Value::Null,
                    "tool_calls": std::mem::take(&mut pending_tool_calls),
                }));
            }
            let content = item
                .get("output")
                .or_else(|| item.get("result"))
                .and_then(content_to_text)
                .unwrap_or_else(|| " ".to_string());
            messages.push(json!({
                "role": "tool",
                "tool_call_id": item.get("call_id").and_then(|value| value.as_str()).unwrap_or(""),
                "content": content,
            }));
            continue;
        }

        if !pending_tool_calls.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": serde_json::Value::Null,
                "tool_calls": std::mem::take(&mut pending_tool_calls),
            }));
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
            .and_then(|content| content_to_chat_content(content, upstream_model, text_only_input))
            .unwrap_or_else(|| json!(" "));
        messages.push(json!({ "role": role, "content": content }));
    }

    if !pending_tool_calls.is_empty() {
        messages.push(json!({
            "role": "assistant",
            "content": serde_json::Value::Null,
            "tool_calls": pending_tool_calls,
        }));
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

fn content_to_chat_content(
    value: &serde_json::Value,
    upstream_model: &str,
    text_only_input: bool,
) -> Option<serde_json::Value> {
    if let Some(text) = value.as_str() {
        return Some(json!(text));
    }

    if let Some(array) = value.as_array() {
        let mut blocks = Vec::new();
        let mut text_only_parts = Vec::new();
        let mut saw_non_text_part = false;
        for part in array {
            if let Some(text) = text_part(part) {
                if !text.is_empty() {
                    text_only_parts.push(text.clone());
                    blocks.push(json!({ "type": "text", "text": text }));
                }
                continue;
            }
            if is_image_part(part) {
                saw_non_text_part = true;
                if text_only_input || known_text_only_model(upstream_model) {
                    blocks.push(json!({ "type": "text", "text": UNSUPPORTED_IMAGE_MARKER }));
                } else if let Some(image_url) = chat_image_url_part(part) {
                    blocks.push(image_url);
                } else {
                    blocks.push(json!({ "type": "text", "text": UNSUPPORTED_IMAGE_MARKER }));
                }
                continue;
            }
            if is_file_part(part) {
                saw_non_text_part = true;
                if text_only_input || known_text_only_model(upstream_model) {
                    blocks.push(json!({ "type": "text", "text": "[Unsupported File]" }));
                } else if let Some(file) = chat_file_part(part) {
                    blocks.push(file);
                }
                continue;
            }
            if is_audio_part(part) {
                saw_non_text_part = true;
                if text_only_input || known_text_only_model(upstream_model) {
                    blocks.push(json!({ "type": "text", "text": "[Unsupported Audio]" }));
                } else if let Some(audio) = chat_audio_part(part) {
                    blocks.push(audio);
                }
            }
        }

        if blocks.is_empty() {
            return None;
        }
        if !saw_non_text_part {
            return Some(json!(text_only_parts.join("")));
        }
        return Some(Value::Array(blocks));
    }

    if value.is_object() {
        if let Some(text) = text_part(value) {
            return Some(json!(text));
        }
        if is_image_part(value) {
            if text_only_input || known_text_only_model(upstream_model) {
                return Some(json!([{ "type": "text", "text": UNSUPPORTED_IMAGE_MARKER }]));
            }
            return chat_image_url_part(value)
                .map(|image| json!([image]))
                .or_else(|| Some(json!([{ "type": "text", "text": UNSUPPORTED_IMAGE_MARKER }])));
        }
        if is_file_part(value) {
            if text_only_input || known_text_only_model(upstream_model) {
                return Some(json!([{ "type": "text", "text": "[Unsupported File]" }]));
            }
            return chat_file_part(value)
                .map(|file| json!([file]))
                .or_else(|| Some(json!([{ "type": "text", "text": "[Unsupported File]" }])));
        }
        if is_audio_part(value) {
            if text_only_input || known_text_only_model(upstream_model) {
                return Some(json!([{ "type": "text", "text": "[Unsupported Audio]" }]));
            }
            return chat_audio_part(value)
                .map(|audio| json!([audio]))
                .or_else(|| Some(json!([{ "type": "text", "text": "[Unsupported Audio]" }])));
        }
        return value
            .get("content")
            .and_then(|content| content_to_chat_content(content, upstream_model, text_only_input));
    }

    None
}

fn responses_tools_to_chat_tools(value: Option<&serde_json::Value>) -> Vec<serde_json::Value> {
    let Some(tools) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut converted = Vec::new();
    for tool in tools {
        append_response_tool_as_chat_tools(tool, &mut converted);
    }
    converted
}

fn collect_tool_search_output_tools(value: Option<&Value>, converted: &mut Vec<Value>) {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                collect_tool_search_output_tools(Some(item), converted);
            }
        }
        Some(Value::Object(obj)) => {
            if obj.get("type").and_then(|value| value.as_str()) == Some("tool_search_output") {
                if let Some(tools) = obj.get("tools").and_then(|value| value.as_array()) {
                    for tool in tools {
                        append_response_tool_as_chat_tools(tool, converted);
                    }
                }
            }
            for value in obj.values() {
                collect_tool_search_output_tools(Some(value), converted);
            }
        }
        _ => {}
    }
}

fn dedup_chat_tools_by_name(tools: &mut Vec<Value>) {
    let mut seen = BTreeMap::<String, ()>::new();
    tools.retain(|tool| {
        let name = tool
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || seen.contains_key(&name) {
            return false;
        }
        seen.insert(name, ());
        true
    });
}

fn append_response_tool_as_chat_tools(
    tool: &serde_json::Value,
    converted: &mut Vec<serde_json::Value>,
) {
    let Some(obj) = tool.as_object() else {
        return;
    };
    let tool_type = obj
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if tool_type == "namespace" {
        let namespace = obj
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let children = obj
            .get("tools")
            .or_else(|| obj.get("functions"))
            .or_else(|| obj.get("children"))
            .and_then(|value| value.as_array());
        if let Some(children) = children {
            for child in children {
                let Some(name) = response_tool_name(child) else {
                    continue;
                };
                let chat_name = flatten_namespace_tool_name(namespace, &name);
                if let Some(chat_tool) = response_tool_to_chat_function(child, &chat_name) {
                    converted.push(chat_tool);
                }
            }
        }
        return;
    }

    if tool_type == "tool_search" {
        converted.push(json!({
            "type": "function",
            "function": {
                "name": "tool_search",
                "description": "Search and load Codex tools, plugins, connectors, and MCP namespaces for the current task.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for tools or connectors to load."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of tool groups to return."
                        }
                    },
                    "required": ["query"]
                }
            }
        }));
        return;
    }

    let Some(name) = response_tool_name(tool) else {
        return;
    };
    if name == "js" {
        return;
    }
    if let Some(chat_tool) = response_tool_to_chat_function(tool, &name) {
        converted.push(chat_tool);
    }
}

fn response_tool_to_chat_function(
    tool: &serde_json::Value,
    chat_name: &str,
) -> Option<serde_json::Value> {
    let obj = tool.as_object()?;
    if obj.get("type").and_then(|value| value.as_str()) == Some("function")
        && obj.get("function").is_some()
    {
        let mut passthrough = tool.clone();
        if let Some(function) = passthrough
            .get_mut("function")
            .and_then(|value| value.as_object_mut())
        {
            function.insert(
                "name".to_string(),
                serde_json::Value::String(chat_name.to_string()),
            );
        }
        return Some(passthrough);
    }

    let function = obj.get("function").and_then(|value| value.as_object());
    let tool_type = obj
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let description = if tool_type == "custom" {
        responses_custom_tool_description(tool)
    } else {
        function
            .and_then(|function| function.get("description"))
            .or_else(|| obj.get("description"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    };
    let parameters = if tool_type == "custom" {
        json!({
            "type": "object",
            "properties": {
                CUSTOM_TOOL_INPUT_FIELD: {
                    "type": "string",
                    "description": CUSTOM_TOOL_INPUT_DESCRIPTION
                }
            },
            "required": [CUSTOM_TOOL_INPUT_FIELD]
        })
    } else {
        function
            .and_then(|function| function.get("parameters"))
            .or_else(|| function.and_then(|function| function.get("input_schema")))
            .or_else(|| obj.get("parameters"))
            .or_else(|| obj.get("input_schema"))
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": true
                })
            })
    };

    let mut chat_tool = json!({
        "type": "function",
        "function": {
            "name": chat_name,
            "description": description,
            "parameters": parameters,
        }
    });
    if let Some(strict) = function
        .and_then(|function| function.get("strict"))
        .or_else(|| obj.get("strict"))
    {
        chat_tool["function"]["strict"] = strict.clone();
    }
    Some(chat_tool)
}

fn response_tool_name(tool: &serde_json::Value) -> Option<String> {
    tool.get("name")
        .or_else(|| {
            tool.get("function")
                .and_then(|function| function.get("name"))
        })
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn responses_custom_tool_description(tool: &Value) -> String {
    format!(
        "{CUSTOM_TOOL_PRESERVED_METADATA_HEADING}\n```json\n{}\n```",
        canonical_json_string(tool)
    )
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(&canonical_json_value(value)).unwrap_or_else(|_| "{}".to_string())
}

fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut sorted = serde_json::Map::new();
            for (key, value) in obj.iter().collect::<BTreeMap<_, _>>() {
                sorted.insert((*key).clone(), canonical_json_value(value));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        _ => value.clone(),
    }
}

fn flatten_namespace_tool_name(namespace: &str, name: &str) -> String {
    let namespace = namespace.trim();
    let name = name.trim();
    if namespace.is_empty() {
        return name.to_string();
    }
    if namespace.ends_with("__") {
        format!("{namespace}{name}")
    } else {
        format!("{namespace}_{name}")
    }
}

fn responses_tool_choice_to_chat(tool_choice: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = tool_choice.as_object() else {
        return tool_choice.clone();
    };
    let choice_type = obj
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let name = obj
        .get("name")
        .or_else(|| {
            obj.get("function")
                .and_then(|function| function.get("name"))
        })
        .and_then(|value| value.as_str());
    if matches!(choice_type, "function" | "custom") {
        if let Some(name) = name {
            return json!({
                "type": "function",
                "function": { "name": name }
            });
        }
    }
    tool_choice.clone()
}

const UNSUPPORTED_IMAGE_MARKER: &str = "[Unsupported Image]";

fn text_part(value: &serde_json::Value) -> Option<String> {
    value
        .get("text")
        .or_else(|| value.get("input_text"))
        .or_else(|| value.get("output_text"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn is_image_part(value: &serde_json::Value) -> bool {
    matches!(
        value.get("type").and_then(|value| value.as_str()),
        Some("input_image" | "image" | "image_url")
    ) || value.get("image_url").is_some()
        || value.get("source").is_some()
}

fn is_file_part(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(|value| value.as_str()),
        Some("input_file" | "file")
    ) || value.get("file_id").is_some()
        || value.get("file_data").is_some()
}

fn is_audio_part(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(|value| value.as_str()),
        Some("input_audio" | "audio")
    ) || value.get("input_audio").is_some()
}

fn chat_image_url_part(value: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(image_url) = value.get("image_url") {
        let image_url = if image_url.is_object() {
            image_url.clone()
        } else {
            json!({ "url": image_url.as_str()? })
        };
        return Some(json!({
            "type": "image_url",
            "image_url": image_url,
        }));
    }

    let source = value.get("source")?;
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
        return Some(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{media_type};base64,{data}") },
        }));
    }

    None
}

fn chat_file_part(value: &Value) -> Option<Value> {
    let mut file = serde_json::Map::new();
    let has_supported_file_ref = value.get("file_id").is_some() || value.get("file_data").is_some();
    if !has_supported_file_ref {
        return None;
    }

    for key in ["file_id", "file_data", "filename"] {
        if let Some(part) = value.get(key) {
            file.insert(key.to_string(), part.clone());
        }
    }

    Some(json!({
        "type": "file",
        "file": Value::Object(file),
    }))
}

fn chat_audio_part(value: &Value) -> Option<Value> {
    let input_audio = value
        .get("input_audio")
        .or_else(|| value.get("audio"))
        .cloned()?;
    Some(json!({
        "type": "input_audio",
        "input_audio": input_audio,
    }))
}

fn known_text_only_model(model: &str) -> bool {
    let normalized = model
        .trim()
        .trim_start_matches("models/")
        .trim()
        .to_ascii_lowercase();
    let tail = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let compact_tail = tail
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();

    const EXACT_TAILS: &[&str] = &[
        "deepseek-chat",
        "deepseek-reasoner",
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "glm-5.1",
        "kat-coder",
        "kat-coder-pro",
        "longcat-flash-chat",
        "mimo-v2.5-pro",
    ];
    const TAIL_PREFIXES: &[&str] = &["qwen3-coder", "step-3.5-flash"];

    compact_tail.starts_with("deepseekv4")
        || compact_tail.starts_with("minimax")
        || EXACT_TAILS.contains(&tail)
        || TAIL_PREFIXES.iter().any(|prefix| tail.starts_with(prefix))
}

fn copy_if_present(source: &serde_json::Value, target: &mut serde_json::Value, key: &str) {
    if let Some(value) = source.get(key) {
        target[key] = value.clone();
    }
}

fn apply_chat_reasoning_compat(
    source: &serde_json::Value,
    chat_body: &mut serde_json::Value,
    upstream_model: &str,
    config: Option<&ChatReasoningConfig>,
) {
    if let Some(config) = config {
        apply_configured_chat_reasoning_compat(source, chat_body, config);
        return;
    }

    let Some(switch_param) = chat_reasoning_switch_param(upstream_model) else {
        return;
    };
    let enabled = codex_reasoning_requested(source);
    match switch_param {
        "thinking" => apply_thinking_param(chat_body, switch_param, enabled),
        "enable_thinking" | "reasoning_split" => {
            remove_chat_field(chat_body, "reasoning_effort");
            apply_thinking_param(chat_body, switch_param, enabled);
        }
        _ => {}
    }
}

fn apply_configured_chat_reasoning_compat(
    source: &serde_json::Value,
    chat_body: &mut serde_json::Value,
    config: &ChatReasoningConfig,
) {
    let enabled = codex_reasoning_requested(source);
    let thinking_param = config
        .thinking_param
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    let effort_param = config
        .effort_param
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    let supports_thinking = config.supports_thinking.unwrap_or(thinking_param != "none");
    let supports_effort = config.supports_effort.unwrap_or(effort_param != "none");

    if !supports_effort || effort_param != "reasoning_effort" {
        remove_chat_field(chat_body, "reasoning_effort");
    }
    if !supports_thinking {
        remove_chat_field(chat_body, "thinking");
        remove_chat_field(chat_body, "enable_thinking");
        remove_chat_field(chat_body, "reasoning_split");
    } else {
        apply_thinking_param(chat_body, thinking_param, enabled);
    }

    if supports_effort {
        apply_effort_param(
            source,
            chat_body,
            effort_param,
            config.effort_value_mode.as_deref(),
        );
    }
    if let Some(min_output_tokens) = config.min_output_tokens {
        ensure_min_output_tokens(chat_body, min_output_tokens);
    }
}

fn apply_thinking_param(chat_body: &mut serde_json::Value, param: &str, enabled: Option<bool>) {
    let Some(enabled) = enabled else {
        return;
    };
    match param {
        "thinking" => {
            chat_body["thinking"] = json!({
                "type": if enabled { "enabled" } else { "disabled" },
            });
        }
        "enable_thinking" => {
            chat_body["enable_thinking"] = json!(enabled);
        }
        "reasoning_split" => {
            chat_body["reasoning_split"] = json!(enabled);
        }
        _ => {}
    }
}

fn apply_effort_param(
    source: &serde_json::Value,
    chat_body: &mut serde_json::Value,
    param: &str,
    value_mode: Option<&str>,
) {
    let Some(effort) = codex_reasoning_effort(source) else {
        return;
    };
    let effort = map_reasoning_effort(&effort, value_mode);
    match param {
        "reasoning_effort" => {
            chat_body["reasoning_effort"] = json!(effort);
        }
        "reasoning.effort" => {
            remove_chat_field(chat_body, "reasoning_effort");
            chat_body["reasoning"] = json!({ "effort": effort });
        }
        _ => {}
    }
}

fn ensure_min_output_tokens(chat_body: &mut serde_json::Value, min_output_tokens: u64) {
    let current = chat_body.get("max_tokens").and_then(|value| value.as_u64());
    if current.is_none_or(|value| value < min_output_tokens) {
        chat_body["max_tokens"] = json!(min_output_tokens);
    }
}

fn remove_chat_field(chat_body: &mut serde_json::Value, key: &str) {
    if let Some(obj) = chat_body.as_object_mut() {
        obj.remove(key);
    }
}

fn chat_reasoning_switch_param(upstream_model: &str) -> Option<&'static str> {
    let normalized = upstream_model
        .trim()
        .trim_start_matches("models/")
        .trim()
        .to_ascii_lowercase();

    if normalized.contains("qwen")
        || normalized.contains("dashscope")
        || normalized.contains("bailian")
        || normalized.contains("siliconflow")
    {
        return Some("enable_thinking");
    }
    if normalized.contains("minimax") {
        return Some("reasoning_split");
    }
    if normalized.contains("kimi")
        || normalized.contains("moonshot")
        || normalized.contains("glm")
        || normalized.contains("zhipu")
        || normalized.contains("z.ai")
    {
        return Some("thinking");
    }

    None
}

fn codex_reasoning_requested(source: &serde_json::Value) -> Option<bool> {
    if let Some(effort) = source
        .pointer("/reasoning/effort")
        .and_then(|value| value.as_str())
        .or_else(|| {
            source
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
        })
    {
        return Some(!matches!(
            effort.trim().to_ascii_lowercase().as_str(),
            "none" | "off" | "disabled" | "false" | "0"
        ));
    }

    if let Some(enabled) = source
        .pointer("/reasoning/enabled")
        .and_then(|value| value.as_bool())
    {
        return Some(enabled);
    }

    source.get("reasoning").map(|value| !value.is_null())
}

fn codex_reasoning_effort(source: &serde_json::Value) -> Option<String> {
    source
        .pointer("/reasoning/effort")
        .and_then(|value| value.as_str())
        .or_else(|| {
            source
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| {
            !matches!(
                value.to_ascii_lowercase().as_str(),
                "none" | "off" | "disabled" | "false" | "0"
            )
        })
        .map(ToString::to_string)
}

fn map_reasoning_effort(effort: &str, mode: Option<&str>) -> String {
    let normalized = effort.trim().to_ascii_lowercase();
    match mode.map(str::trim) {
        Some("low_high") => {
            if matches!(normalized.as_str(), "low" | "minimal" | "none") {
                "low".to_string()
            } else {
                "high".to_string()
            }
        }
        Some("openrouter") => match normalized.as_str() {
            "max" | "xhigh" | "extra_high" | "extra-high" => "xhigh".to_string(),
            "minimal" | "low" | "medium" | "high" => normalized,
            _ => "medium".to_string(),
        },
        Some("deepseek") | Some("passthrough") | None => match normalized.as_str() {
            "xhigh" | "extra_high" | "extra-high" | "max" => "high".to_string(),
            "minimal" => "low".to_string(),
            _ => normalized,
        },
        _ => normalized,
    }
}

fn inject_stream_include_usage(body: &mut Value) {
    if body.get("stream").and_then(|value| value.as_bool()) != Some(true) {
        return;
    }
    if body.get("stream_options").is_none() || !body["stream_options"].is_object() {
        body["stream_options"] = json!({});
    }
    if let Some(options) = body
        .get_mut("stream_options")
        .and_then(|value| value.as_object_mut())
    {
        options.insert("include_usage".to_string(), Value::Bool(true));
    }
}

fn chat_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0,
            "output_tokens_details": { "reasoning_tokens": 0 }
        });
    };

    let input_tokens = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(|value| value.as_u64())
        .unwrap_or(input_tokens + output_tokens);

    let mut result = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens,
    });

    if let Some(cached) = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .and_then(|value| value.as_u64())
    {
        result["input_tokens_details"] = json!({ "cached_tokens": cached });
    }

    if let Some(details) = usage
        .get("completion_tokens_details")
        .or_else(|| usage.get("output_tokens_details"))
        .filter(|value| value.is_object())
    {
        let mut details = details.clone();
        if details.get("reasoning_tokens").is_none() {
            details["reasoning_tokens"] = json!(0);
        }
        result["output_tokens_details"] = details;
    } else {
        result["output_tokens_details"] = json!({ "reasoning_tokens": 0 });
    }

    if let Some(cache_read) = usage.get("cache_read_input_tokens") {
        result["cache_read_input_tokens"] = cache_read.clone();
    }
    if let Some(cache_creation) = usage.get("cache_creation_input_tokens") {
        result["cache_creation_input_tokens"] = cache_creation.clone();
    }

    result
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
    fn minimax_chat_request_maps_reasoning_effort_to_reasoning_split() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "minimax-m3",
            "input": "解释这个错误",
            "reasoning_effort": "high"
        });

        let chat = responses_create_to_chat_request(&message, "MiniMax-M3").unwrap();

        assert_eq!(chat.body["reasoning_split"], true);
        assert!(chat.body.get("reasoning_effort").is_none());
    }

    #[test]
    fn minimax_chat_request_preserves_explicit_reasoning_off() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "minimax-m3",
            "input": "直接回答",
            "reasoning": { "effort": "none" }
        });

        let chat = responses_create_to_chat_request(&message, "MiniMax-M3").unwrap();

        assert_eq!(chat.body["reasoning_split"], false);
        assert!(chat.body.get("reasoning_effort").is_none());
    }

    #[test]
    fn qwen_chat_request_maps_reasoning_to_enable_thinking() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "qwen3",
            "input": "分析一下",
            "reasoning": { "effort": "high" }
        });

        let chat = responses_create_to_chat_request(&message, "qwen3-coder").unwrap();

        assert_eq!(chat.body["enable_thinking"], true);
        assert!(chat.body.get("reasoning_effort").is_none());
    }

    #[test]
    fn configured_chat_reasoning_overrides_model_name_heuristic() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "custom-visible",
            "input": "分析一下",
            "reasoning": { "effort": "high" },
            "max_output_tokens": 128
        });
        let config = ChatReasoningConfig {
            supports_thinking: Some(true),
            supports_effort: Some(false),
            thinking_param: Some("enable_thinking".to_string()),
            effort_param: Some("none".to_string()),
            effort_value_mode: None,
            min_output_tokens: Some(2048),
            output_format: Some("reasoning_content".to_string()),
        };

        let chat = responses_create_to_chat_request_with_reasoning(
            &message,
            "not-qwen-name",
            Some(&config),
        )
        .unwrap();

        assert_eq!(chat.body["enable_thinking"], true);
        assert_eq!(chat.body["max_tokens"], 2048);
        assert!(chat.body.get("reasoning_effort").is_none());
    }

    #[test]
    fn configured_openrouter_effort_uses_reasoning_object() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "openrouter-model",
            "input": "分析一下",
            "reasoning": { "effort": "max" }
        });
        let config = ChatReasoningConfig {
            supports_thinking: Some(false),
            supports_effort: Some(true),
            thinking_param: Some("none".to_string()),
            effort_param: Some("reasoning.effort".to_string()),
            effort_value_mode: Some("openrouter".to_string()),
            min_output_tokens: None,
            output_format: Some("auto".to_string()),
        };

        let chat =
            responses_create_to_chat_request_with_reasoning(&message, "deepseek-r1", Some(&config))
                .unwrap();

        assert_eq!(chat.body["reasoning"]["effort"], "xhigh");
        assert!(chat.body.get("reasoning_effort").is_none());
        assert!(chat.body.get("thinking").is_none());
    }

    #[test]
    fn response_input_image_converts_to_chat_image_url() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "gpt-4o",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看这张图" },
                        { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                    ]
                }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "gpt-4o").unwrap();
        let content = chat.body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "看这张图");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,abc");
    }

    #[test]
    fn response_input_file_and_audio_convert_to_chat_parts() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "multimodal",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "分析附件" },
                        {
                            "type": "input_file",
                            "filename": "report.pdf",
                            "file_data": "data:application/pdf;base64,abc"
                        },
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": "abc",
                                "format": "wav"
                            }
                        }
                    ]
                }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "multimodal").unwrap();
        let content = chat.body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "file");
        assert_eq!(content[1]["file"]["filename"], "report.pdf");
        assert_eq!(content[2]["type"], "input_audio");
        assert_eq!(content[2]["input_audio"]["format"], "wav");
    }

    #[test]
    fn text_only_model_replaces_input_image_with_marker() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "deepseek-v4-pro",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看这张图" },
                        { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                    ]
                }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "deepseek-v4-pro").unwrap();
        let content = chat.body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], UNSUPPORTED_IMAGE_MARKER);
    }

    #[test]
    fn minimax_model_replaces_input_image_with_marker() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "minimax-m3",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看这张图" },
                        { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                    ]
                }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "MiniMax-M3").unwrap();
        let content = chat.body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], UNSUPPORTED_IMAGE_MARKER);
    }

    #[test]
    fn configured_text_only_input_replaces_images_for_unknown_model() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "minimax-m3",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "看这张图" },
                        { "type": "input_image", "image_url": "data:image/png;base64,abc" }
                    ]
                }
            ]
        });

        let chat = responses_create_to_chat_request_with_options(
            &message,
            "MiniMax-M3",
            ChatRequestOptions {
                chat_reasoning: None,
                text_only_input: true,
            },
        )
        .unwrap();
        let content = chat.body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], UNSUPPORTED_IMAGE_MARKER);
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

    #[test]
    fn stream_state_strips_think_tags_from_output_text() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let events = state.ingest_sse_line(
            r#"data: {"choices":[{"delta":{"content":"<think>hidden</think>visible"}}]}"#,
        );

        let deltas = events
            .iter()
            .filter(|event| event["type"] == "response.output_text.delta")
            .map(|event| event["delta"].as_str().unwrap_or(""))
            .collect::<Vec<_>>();
        assert_eq!(deltas, vec!["visible"]);
    }

    #[test]
    fn stream_state_strips_split_think_tags() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let first = state.ingest_sse_line(r#"data: {"choices":[{"delta":{"content":"<thi"}}]}"#);
        let second = state
            .ingest_sse_line(r#"data: {"choices":[{"delta":{"content":"nk>hidden</think>ok"}}]}"#);

        assert!(first
            .iter()
            .all(|event| event["type"] != "response.output_text.delta"));
        let deltas = second
            .iter()
            .filter(|event| event["type"] == "response.output_text.delta")
            .map(|event| event["delta"].as_str().unwrap_or(""))
            .collect::<Vec<_>>();
        assert_eq!(deltas, vec!["ok"]);
    }

    #[test]
    fn passthrough_normalizer_strips_think_tags_from_responses_events() {
        let mut normalizer = ResponseEventPassthroughNormalizer::default();
        let first = normalizer.normalize_event(serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "<thi"
        }));
        let second = normalizer.normalize_event(serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "nk>hidden</think>visible"
        }));

        assert!(first.is_none());
        assert_eq!(second.unwrap()["delta"], "visible");
    }

    #[test]
    fn stream_state_emits_reasoning_lifecycle() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let first = state
            .ingest_sse_line(r#"data: {"choices":[{"delta":{"reasoning_content":"思考中"}}]}"#);
        let second = state.ingest_sse_line(r#"data: {"choices":[{"delta":{"content":"答案"}}]}"#);
        let done = state.finish_events();
        let events = first
            .into_iter()
            .chain(second)
            .chain(done)
            .collect::<Vec<_>>();

        assert!(events
            .iter()
            .any(|event| event["type"] == "response.output_item.added"
                && event["item"]["type"] == "reasoning"));
        assert!(events
            .iter()
            .any(|event| event["type"] == "response.reasoning_text.delta"));
        assert!(events
            .iter()
            .any(|event| event["type"] == "response.reasoning_text.done"));
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "response.output_text.delta"
                    && event["delta"] == "答案")
        );
    }

    #[test]
    fn stream_state_accumulates_tool_call_arguments() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let first = state.ingest_sse_line(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"lookup","arguments":"{\"q\""}}]}}]}"#,
        );
        let second = state.ingest_sse_line(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"rust\"}"}}]},"finish_reason":"tool_calls"}]}"#,
        );
        let events = first.into_iter().chain(second).collect::<Vec<_>>();

        assert!(events
            .iter()
            .any(|event| event["type"] == "response.output_item.added"
                && event["item"]["type"] == "function_call"
                && event["item"]["name"] == "lookup"));
        assert!(events
            .iter()
            .any(|event| event["type"] == "response.function_call_arguments.delta"));
        assert!(events.iter().any(|event| event["type"]
            == "response.function_call_arguments.done"
            && event["arguments"] == "{\"q\":\"rust\"}"));
    }

    #[test]
    fn stream_state_converts_minimax_markup_tool_call_to_function_call() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let events = state.ingest_sse_line(
            r#"data: {"choices":[{"delta":{"content":"我先看一下截图。 </mm:think>]<minimax>[<tool_call>\n]<minimax>[<invoke name=\"shell_cmd\">]<minimax>[<invoke name=\"command\">ls -la /tmp/a.png</invoke>]<minimax>[<description>Check if image file exists</description>]</invoke>]</tool_call>"}}]}"#,
        );

        assert!(events
            .iter()
            .any(|event| event["type"] == "response.output_text.delta"
                && event["delta"] == "我先看一下截图。 "));
        assert!(events
            .iter()
            .any(|event| event["type"] == "response.output_item.added"
                && event["item"]["type"] == "function_call"
                && event["item"]["name"] == "shell_cmd"));
        assert!(events.iter().any(|event| event["type"]
            == "response.function_call_arguments.done"
            && event["arguments"]
                .as_str()
                .unwrap_or("")
                .contains("/tmp/a.png")));
        assert!(!events
            .iter()
            .any(|event| event["type"] == "response.output_text.delta"
                && event["delta"]
                    .as_str()
                    .unwrap_or("")
                    .contains("<tool_call>")));
    }

    #[test]
    fn stream_state_buffers_split_minimax_tool_call() {
        let mut state = ResponsesWsStreamState::new("minimax");
        let first = state.ingest_sse_line(r#"data: {"choices":[{"delta":{"content":"<tool_"}}]}"#);
        let second = state.ingest_sse_line(
            r#"data: {"choices":[{"delta":{"content":"call><invoke name=\"lookup\"><invoke name=\"q\">rust</invoke></invoke></tool_call>"}}]}"#,
        );

        assert!(first
            .iter()
            .all(|event| event["type"] != "response.output_text.delta"));
        assert!(second
            .iter()
            .any(|event| event["type"] == "response.output_item.added"
                && event["item"]["type"] == "function_call"
                && event["item"]["name"] == "lookup"));
    }

    #[test]
    fn response_input_function_calls_convert_to_chat_tool_messages() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "demo",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{\"q\":\"rust\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "result"
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "继续" }]
                }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "demo-upstream").unwrap();
        assert_eq!(chat.body["messages"][0]["role"], "assistant");
        assert_eq!(chat.body["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(chat.body["messages"][1]["role"], "tool");
        assert_eq!(chat.body["messages"][1]["tool_call_id"], "call_1");
        assert_eq!(chat.body["messages"][2]["content"], "继续");
    }

    #[test]
    fn response_tools_convert_to_chat_tools_and_tool_choice() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "demo",
            "parallel_tool_calls": true,
            "tool_choice": { "type": "function", "name": "lookup" },
            "tools": [
                {
                    "type": "function",
                    "name": "lookup",
                    "description": "Lookup docs",
                    "parameters": {
                        "type": "object",
                        "properties": { "q": { "type": "string" } },
                        "required": ["q"]
                    },
                    "strict": true
                },
                { "type": "custom", "name": "apply_patch" },
                {
                    "type": "namespace",
                    "name": "shell",
                    "tools": [
                        {
                            "type": "function",
                            "name": "cmd",
                            "description": "Run command",
                            "parameters": {
                                "type": "object",
                                "properties": { "command": { "type": "string" } }
                            }
                        }
                    ]
                },
                { "type": "tool_search" }
            ],
            "input": "test"
        });

        let chat = responses_create_to_chat_request(&message, "demo-upstream").unwrap();
        let tools = chat.body["tools"].as_array().unwrap();
        let names = tools
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap_or(""))
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["lookup", "apply_patch", "shell_cmd", "tool_search"]
        );
        assert_eq!(tools[0]["function"]["strict"], true);
        assert_eq!(tools[1]["function"]["parameters"]["required"][0], "input");
        assert!(tools[1]["function"]["description"]
            .as_str()
            .unwrap_or("")
            .contains("Original tool definition"));
        assert_eq!(chat.body["tool_choice"]["type"], "function");
        assert_eq!(chat.body["tool_choice"]["function"]["name"], "lookup");
        assert_eq!(chat.body["parallel_tool_calls"], true);
        assert_eq!(chat.body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn response_custom_tool_preserves_format_metadata_in_description() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "demo",
            "tools": [
                {
                    "type": "custom",
                    "name": "apply_patch",
                    "description": "Use the `apply_patch` tool to edit files.",
                    "format": {
                        "type": "grammar",
                        "syntax": "lark",
                        "definition": "start: /.+/"
                    }
                }
            ],
            "tool_choice": { "type": "custom", "name": "apply_patch" },
            "input": "test"
        });

        let chat = responses_create_to_chat_request(&message, "demo-upstream").unwrap();
        let description = chat.body["tools"][0]["function"]["description"]
            .as_str()
            .unwrap_or("");

        assert!(description.contains("\"format\""));
        assert!(description.contains("\"syntax\":\"lark\""));
        assert_eq!(chat.body["tool_choice"]["function"]["name"], "apply_patch");
    }

    #[test]
    fn response_tool_search_output_tools_keep_tool_choice() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "demo",
            "tool_choice": "auto",
            "input": [
                {
                    "type": "tool_search_output",
                    "tools": [
                        {
                            "type": "function",
                            "name": "read_file",
                            "description": "Read a file",
                            "parameters": {
                                "type": "object",
                                "properties": { "path": { "type": "string" } },
                                "required": ["path"]
                            }
                        }
                    ]
                },
                { "type": "message", "role": "user", "content": "read" }
            ]
        });

        let chat = responses_create_to_chat_request(&message, "demo-upstream").unwrap();

        assert_eq!(chat.body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(chat.body["tool_choice"], "auto");
    }

    #[test]
    fn response_completed_includes_chat_usage_details() {
        let value = serde_json::json!({
            "choices": [
                { "message": { "content": "done" } }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
                "prompt_tokens_details": { "cached_tokens": 3 },
                "completion_tokens_details": { "reasoning_tokens": 2 }
            }
        });

        let body = chat_completion_json_to_response_body(&value, "demo");

        assert_eq!(body["usage"]["input_tokens"], 10);
        assert_eq!(body["usage"]["output_tokens"], 5);
        assert_eq!(body["usage"]["input_tokens_details"]["cached_tokens"], 3);
        assert_eq!(
            body["usage"]["output_tokens_details"]["reasoning_tokens"],
            2
        );
    }

    #[test]
    fn response_completed_from_non_stream_chat_preserves_reasoning_and_tool_calls() {
        let value = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "reasoning_content": "先检查截图路径。",
                        "content": "<think>hidden</think>我来检查截图。",
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
            ]
        });

        let body = chat_completion_json_to_response_body(&value, "minimax-m3");
        let output = body["output"].as_array().expect("response output");

        assert_eq!(output.len(), 3);
        assert_eq!(output[0]["type"], "reasoning");
        assert_eq!(
            output[0]["summary"][0]["text"],
            serde_json::json!("先检查截图路径。")
        );
        assert_eq!(output[1]["type"], "message");
        assert_eq!(output[1]["content"][0]["text"], "我来检查截图。");
        assert_eq!(output[2]["type"], "function_call");
        assert_eq!(output[2]["name"], "shell_cmd");
        assert_eq!(
            output[2]["arguments"],
            "{\"command\":\"ls -la /tmp/a.png\"}"
        );
    }

    #[test]
    fn response_completed_from_non_stream_minimax_markup_tool_call_is_not_visible_text() {
        let value = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": "我先看一下截图。 </mm:think>]<minimax>[<tool_call>\n]<minimax>[<invoke name=\"shell_cmd\">]<minimax>[<invoke name=\"command\">ls -la /tmp/a.png</invoke>]<minimax>[<description>Check if image file exists</description>]</invoke>]<minimax>[</tool_call>"
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        let body = chat_completion_json_to_response_body(&value, "minimax-m3");
        let output = body["output"].as_array().expect("response output");
        let visible_text = serde_json::to_string(output).expect("response output json");

        assert!(output.iter().any(|item| {
            item["type"] == "function_call"
                && item["name"] == "shell_cmd"
                && item["arguments"]
                    .as_str()
                    .unwrap_or("")
                    .contains("/tmp/a.png")
        }));
        assert!(output.iter().any(|item| {
            item["type"] == "message" && item["content"][0]["text"] == "我先看一下截图。 "
        }));
        assert!(!visible_text.contains("<tool_call>"));
        assert!(!visible_text.contains("<minimax>"));
        assert!(!visible_text.contains("</mm:think>"));
    }

    #[test]
    fn response_tools_drop_tool_choice_when_no_tools_survive() {
        let message = serde_json::json!({
            "type": "response.create",
            "model": "demo",
            "parallel_tool_calls": true,
            "tool_choice": "auto",
            "tools": [
                { "type": "function", "description": "missing name" },
                { "type": "function", "name": "js" }
            ],
            "input": "test"
        });

        let chat = responses_create_to_chat_request(&message, "demo-upstream").unwrap();
        assert!(chat.body.get("tools").is_none());
        assert!(chat.body.get("tool_choice").is_none());
        assert!(chat.body.get("parallel_tool_calls").is_none());
    }
}
