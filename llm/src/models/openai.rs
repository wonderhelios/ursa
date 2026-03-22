//! OpenAI-compatible provider with optional resilience layer

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error};

use crate::provider::{
    ChatRequest, ChatResponse, FunctionCall, LLMProvider, Message, TokenUsage, ToolCall,
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
        let response = self
            .client
            .post(format!("{}/chat/completions", self.config.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            error!("API error {}: {}", status, text);
            return Err(anyhow::anyhow!("API error {}: {}", status, text));
        }

        let json_resp: serde_json::Value = response.json().await?;
        debug!("Response: {}", json_resp);

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
                            let response = client
                                .post(format!("{}/chat/completions", base_url))
                                .header("Authorization", format!("Bearer {}", api_key))
                                .json(body.as_ref())
                                .send()
                                .await?;

                            if !response.status().is_success() {
                                let status = response.status();
                                let text = response.text().await?;
                                error!("API error {}: {}", status, text);
                                return Err(anyhow::anyhow!("API error {}: {}", status, text));
                            }

                            let json_resp: serde_json::Value = response.json().await?;
                            Ok(json_resp)
                        }
                    })
                    .await
                    .and_then(|json_resp| self.parse_response(json_resp))
            }
            None => self.do_request(&self.config.api_key, &body).await,
        }
    }

    fn name(&self) -> &str {
        "openai"
    }
}
