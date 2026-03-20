// OpenAI Provider

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{debug, error};

use crate::provider::{
    ChatRequest, ChatResponse, FunctionCall, LLMProvider, Message, TokenUsage, ToolCall,
};

#[derive(Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
    /// (0.0 - 2.0)
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
    /// Create config from environment variables
    ///
    /// Variables:
    /// - `URSA_LLM_API_KEY` (required)
    /// - `URSA_LLM_BASE_URL` (optional, default: OpenAI)
    /// - `URSA_LLM_MODEL` (optional, default: gpt-4o-mini)
    /// - `URSA_LLM_TIMEOUT_SECS` (optional, default: 60)
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("URSA_LLM_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        let base_url = std::env::var("URSA_LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned());
        let model = std::env::var("URSA_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_owned());
        let time_out_secs = std::env::var("URSA_LLM_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        Some(Self {
            base_url,
            api_key,
            model,
            timeout: Duration::from_secs(time_out_secs),
            temperature: 0.3,
            max_tokens: Some(4096),
        })
    }
}

pub struct OpenAIProvider {
    config: OpenAIConfig,
    client: Client,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }

    /// Convert messages, preserving tool_calls and tool_call_id
    fn convert_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|m| {
                let mut msg = json!({
                    "role": m.role,
                    "content": m.content,
                });

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

    /// Parse response, including tool_calls
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
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        // 1. Convert messages
        let messages = self.convert_messages(&request.messages);

        // 2. Build request body
        let body = json!({
            "model":self.config.model,
            "messages":messages,
            "temperature":request.temperature.unwrap_or(self.config.temperature),
            "max_tokens": request.max_tokens.or(self.config.max_tokens),
            "tools":request.tools,
            "tool_choice": request.tool_choice,
        });

        // 3. Send request
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .await?;

        // 4. Check HTTP errors
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            error!("Open API error {}: {}", status, text);
            return Err(anyhow::anyhow!("API error {}: {}", status, text));
        }

        // 5. Parse response
        let json_resp: serde_json::Value = response.json().await?;
        debug!("Response: {}", json_resp);

        self.parse_response(json_resp)
    }

    fn name(&self) -> &str {
        "openai"
    }
}
