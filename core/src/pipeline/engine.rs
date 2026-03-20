// Pipeline engine - TPAR architecture
use serde_json::json;
use std::sync::Arc;
use tracing::{debug, field::debug, info, warn};

use ursa_llm::provider::{ChatRequest, ChatResponse, LLMProvider, Message, Role, ToolCall};
use ursa_tools::{Tool, ToolDefinition, tools};
use ursa_tools::{ToolRegistry, registry};

pub struct PipelineEngine {
    llm: Arc<dyn LLMProvider>,
    registry: ToolRegistry,
}

impl PipelineEngine {
    pub fn new(llm: Arc<dyn LLMProvider>, registry: ToolRegistry) -> Self {
        info!("PiplineEngine created with {} tools", registry.all().len());
        Self { llm, registry }
    }

    pub async fn run(&self, user_input: &str) -> anyhow::Result<String> {
        info!("Pipeline start: {}", user_input);

        let mut message = vec![Message {
            role: Role::User,
            content: user_input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let tools_json = {
            let tools = self.registry.all();
            if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|t| {
                            let def = t.definition();
                            json!({"type":"function","function":{
                                "name":def.name,
                                "description":def.description,
                                "parameters":def.parameters,
                            }})
                        })
                        .collect(),
                )
            }
        };

        for iter in 0..10 {
            debug!("Iteration {}", iter);

            let request = ChatRequest {
                messages: message.clone(),
                temperature: Some(0.3),
                max_tokens: Some(4096),
                tools: tools_json.clone(),
                tool_choice: None,
            };

            let response = self.llm.chat(request).await?;
            debug!("LLM response: {:?}", response);

            match &response.tool_calls {
                Some(tool_calls) if !tool_calls.is_empty() => {
                    info!("Tool calls: {}", tool_calls.len());

                    // asssistant message with tool_calls
                    message.push(Message {
                        role: Role::Assistant,
                        content: response.content.clone(),
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                    });

                    // Execute tools sequentially
                    for tc in tool_calls {
                        let result = self.execute_tool(tc).await;
                        message.push(Message {
                            role: Role::Tool,
                            content: result,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                }
                _ => {
                    info!("Done after {} iterations", iter + 1);
                    return Ok(response.content);
                }
            }
        }

        warn!("Max iterations reached");
        Ok("Reach max iterations,please retry simpler request".to_string())
    }

    async fn execute_tool(&self, tc: &ToolCall) -> String {
        let name = &tc.function.name;
        let args = &tc.function.arguments;

        info!("Executing tool: {} args: {}", name, args);

        let tool = match self.registry.get(name) {
            Some(t) => t,
            None => {
                let err = format!("Tool '{}' not found", name);
                warn!("{}", err);
                return err;
            }
        };

        let args_val: serde_json::Value = match serde_json::from_str(args) {
            Ok(v) => v,
            Err(e) => {
                let err = format!("Failed to parse args: {}", e);
                warn!("{}", err);
                return err;
            }
        };

        match tool.execute(args_val).await {
            Ok(result) => {
                // Truncate overly long results
                if result.len() > 10000 {
                    format!("{}...\n[truncated]", &result[..10000])
                } else {
                    result
                }
            }
            Err(e) => format!("Tool error: {}", e),
        }
    }
}
