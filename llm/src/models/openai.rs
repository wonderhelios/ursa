//! OpenAI-compatible provider with optional resilience layer

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, warn};

use crate::provider::{
    ChatRequest, ChatResponse, FunctionCall, LLMProvider, Message, StreamChunk, StreamSender, TokenUsage, ToolCall,
};
use crate::resilience::Resilience;

// ===== Config =====

#[derive(Serialize, Deserialize, Clone)]
pub struct OpenAIConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_owned(),
            timeout: Duration::from_secs(60),
            temperature: 0.3,
            max_tokens: Some(4096),
        }
    }
}

impl OpenAIConfig {
    /// Load from environment variables:
    ///   URSA_LLM_API_KEY       (required)
    ///   URSA_LLM_BASE_URL      (optional)
    ///   URSA_LLM_MODEL         (optional)
    ///   URSA_LLM_TIMEOUT_SECS  (optional)
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("URSA_LLM_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        Some(Self {
            api_key,
            base_url: std::env::var("URSA_LLM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned()),
            model: std::env::var("URSA_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_owned()),
            timeout: std::env::var("URSA_LLM_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(60)),
            ..Self::default()
        })
    }
}

// ===== Provider =====

pub struct OpenAIProvider {
    config: OpenAIConfig,
    client: Client,
    /// Optional resilience layer (retry + auth rotation + circuit breaker)
    resilience: Option<Arc<Resilience>>,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to build HTTP client");
        Self {
            config,
            client,
            resilience: None,
        }
    }

    /// Attach a resilience layer (retry + auth rotation + circuit breaker)
    pub fn with_resilience(mut self, resilience: Arc<Resilience>) -> Self {
        self.resilience = Some(resilience);
        self
    }

    // ===== Private helpers =====

    fn convert_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|m| {
                let mut msg = json!({ "role": m.role, "content": m.content });

                if let Some(tool_calls) = &m.tool_calls {
                    let calls: Vec<_> = tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments,
                                }
                            })
                        })
                        .collect();
                    msg["tool_calls"] = json!(calls);
                }

                if let Some(id) = &m.tool_call_id {
                    msg["tool_call_id"] = json!(id);
                }

                msg
            })
            .collect()
    }

    fn build_body(
        &self,
        request: &ChatRequest,
        messages: &[serde_json::Value],
    ) -> serde_json::Value {
        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(self.config.temperature),
            "max_tokens": request.max_tokens.or(self.config.max_tokens),
        });
        if let Some(tools) = &request.tools {
            body["tools"] = json!(tools);
        }
        if let Some(tc) = &request.tool_choice {
            body["tool_choice"] = json!(tc);
        }
        body
    }

    fn parse_response(&self, json_resp: serde_json::Value) -> anyhow::Result<ChatResponse> {
        let choice = &json_resp["choices"][0];
        let message = &choice["message"];

        let content = message["content"].as_str().unwrap_or("").to_string();

        let tool_calls = message["tool_calls"].as_array().map(|calls| {
            calls
                .iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc["id"].as_str()?.to_string(),
                        call_type: tc["type"].as_str()?.to_string(),
                        function: FunctionCall {
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: tc["function"]["arguments"].as_str()?.to_string(),
                        },
                    })
                })
                .collect()
        });

        let usage = TokenUsage {
            prompt_tokens: json_resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: json_resp["usage"]["completion_tokens"]
                .as_u64()
                .unwrap_or(0) as usize,
            total_tokens: json_resp["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize,
        };

        Ok(ChatResponse {
            content,
            usage,
            tool_calls,
            stop_reason: choice["finish_reason"].as_str().map(|s| s.to_string()),
        })
    }

    /// Execute a single HTTP request with the given API key
    async fn do_request(
        &self,
        api_key: &str,
        body: &serde_json::Value,
    ) -> anyhow::Result<ChatResponse> {
        debug!("Request body: {}", body);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let bytes = response.bytes().await?;
            let text = String::from_utf8_lossy(&bytes);
            error!("API error {}: {}", status, text);
            return Err(anyhow::anyhow!("API error {}: {}", status, text));
        }

        let bytes = response.bytes().await?;
        let text = String::from_utf8_lossy(&bytes);

        // Debug: log response length and first 500 chars
        debug!("Response length: {} bytes", bytes.len());
        debug!("Response preview: {}", text.chars().take(500).collect::<String>());

        if text.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response body from API"));
        }

        let json_resp: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                error!("JSON parse error: {}. Raw response (first 2000 chars): {}", e, text.chars().take(2000).collect::<String>());
                return Err(anyhow::anyhow!("Invalid JSON response: {}. Body: {}", e, text.chars().take(200).collect::<String>()));
            }
        };
        debug!("Parsed response: {}", json_resp);

        self.parse_response(json_resp)
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let messages = self.convert_messages(&request.messages);
        let body = Arc::new(self.build_body(&request, &messages));

        match &self.resilience {
            Some(resilience) => {
                // Clone what the closure needs so it owns its data
                let client = self.client.clone();
                let base_url = self.config.base_url.clone();
                let body = body.clone();

                resilience
                    .execute(move |api_key| {
                        let client = client.clone();
                        let base_url = base_url.clone();
                        let body = body.clone();
                        async move {
                            debug!("Request body (resilience): {}", body);
                            let response = client
                                .post(format!("{}/chat/completions", base_url))
                                .header("Authorization", format!("Bearer {}", api_key))
                                .json(body.as_ref())
                                .send()
                                .await?;

                            if !response.status().is_success() {
                                let status = response.status();
                                let bytes = response.bytes().await?;
                                let text = String::from_utf8_lossy(&bytes);
                                error!("API error {}: {}", status, text);
                                return Err(anyhow::anyhow!("API error {}: {}", status, text));
                            }

                            let bytes = response.bytes().await?;
                            let text = String::from_utf8_lossy(&bytes);

                            // Debug: log response length and first 500 chars
                            debug!("Response length: {} bytes", bytes.len());
                            debug!("Response preview: {}", text.chars().take(500).collect::<String>());

                            if text.trim().is_empty() {
                                return Err(anyhow::anyhow!("Empty response body from API"));
                            }

                            let json_resp: serde_json::Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(e) => {
                                    error!("JSON parse error: {}. Raw response (first 2000 chars): {}", e, text.chars().take(2000).collect::<String>());
                                    return Err(anyhow::anyhow!("Invalid JSON response: {}. Body: {}", e, text.chars().take(200).collect::<String>()));
                                }
                            };
                            Ok(json_resp)
                        }
                    })
                    .await
                    .and_then(|json_resp| self.parse_response(json_resp))
            }
            None => self.do_request(&self.config.api_key, &body).await,
        }
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        sender: StreamSender,
    ) -> anyhow::Result<()> {
        let messages = self.convert_messages(&request.messages);
        let mut body = self.build_body(&request, &messages);
        body["stream"] = json!(true);

        // Use a separate client with longer timeout for streaming
        let stream_client = Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minutes for streaming
            .build()
            .expect("Failed to build stream HTTP client");

        let response = stream_client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            error!("API error {}: {}", status, text);
            let _ = sender.send(StreamChunk::Error(format!("API error {}: {}", status, text)));
            return Err(anyhow::anyhow!("API error {}: {}", status, text));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        // Use Vec instead of HashMap to maintain order by index
        let mut active_tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, accumulated_args)

        while let Some(chunk_result) = stream.next().await {
            let chunk: Bytes = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    error!("Stream chunk error: {}", e);
                    let _ = sender.send(StreamChunk::Error(format!("Stream error: {}", e)));
                    return Err(anyhow::anyhow!("Stream error: {}", e));
                }
            };
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(pos) = buffer.find("\n\n") {
                let event = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if let Some(data) = event.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        let _ = sender.send(StreamChunk::Done);
                        return Ok(());
                    }

                    match serde_json::from_str::<serde_json::Value>(data) {
                        Ok(json) => {
                            if let Some(error) = json.get("error") {
                                let err_msg = error.to_string();
                                let _ = sender.send(StreamChunk::Error(err_msg.clone()));
                                return Err(anyhow::anyhow!("Stream error: {}", err_msg));
                            }

                            if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                for choice in choices {
                                    let delta = &choice["delta"];

                                    // Handle content
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        let _ = sender.send(StreamChunk::Content(content.to_string()));
                                    }

                                    // Handle tool calls
                                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                        for tc in tool_calls {
                                            let id = tc.get("id").and_then(|i| i.as_str());
                                            let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                            let function = tc.get("function");

                                            // Ensure we have a slot for this index
                                            while active_tool_calls.len() <= index {
                                                active_tool_calls.push((String::new(), String::new(), String::new()));
                                            }

                                            // If we have an ID, this is a new tool call start
                                            if let Some(id_str) = id {
                                                let name = function
                                                    .and_then(|f| f.get("name"))
                                                    .and_then(|n| n.as_str())
                                                    .unwrap_or("");

                                                if !name.is_empty() {
                                                    let _ = sender.send(StreamChunk::ToolCallStart {
                                                        id: id_str.to_string(),
                                                        name: name.to_string(),
                                                    });
                                                }

                                                // Store/update the tool call info
                                                active_tool_calls[index] = (id_str.to_string(), name.to_string(), String::new());
                                            }

                                            // Handle function arguments delta
                                            if let Some(func) = function
                                                && let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                                    if index < active_tool_calls.len() {
                                                        let (id, _, accumulated) = &mut active_tool_calls[index];
                                                        if !id.is_empty() {
                                                            let _ = sender.send(StreamChunk::ToolCallArgs {
                                                                id: id.clone(),
                                                                delta: args.to_string(),
                                                            });
                                                        }
                                                        accumulated.push_str(args);
                                                    }
                                                }
                                        }
                                    }

                                    // Check finish_reason
                                    if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str())
                                        && (finish_reason == "tool_calls" || finish_reason == "stop") {
                                            for (id, _, _) in &active_tool_calls {
                                                if !id.is_empty() {
                                                    let _ = sender.send(StreamChunk::ToolCallEnd { id: id.clone() });
                                                }
                                            }
                                            active_tool_calls.clear();
                                        }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse SSE data: {}. Data: {}", e, data);
                        }
                    }
                }
            }
        }

        let _ = sender.send(StreamChunk::Done);
        Ok(())
    }

    fn name(&self) -> &str {
        "openai"
    }
}
