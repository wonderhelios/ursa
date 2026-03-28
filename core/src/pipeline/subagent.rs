//! Subagent - isolated agent loop with tool subset and timeout

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::timeout;
use tracing::{debug, info, warn};
use uuid::Uuid;

use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role, ToolCall};
use ursa_tools::{BashTool, ListDirTool, ReadFileTool, Tool, ToolDefinition, WriteFileTool};

// ===== Context Structures =====

/// Subagent context bundle - shared state passed from parent to child agent
#[derive(Debug, Clone, Default)]
pub struct SubagentContext {
    /// Explored file snapshots (to avoid redundant reads)
    pub file_snapshots: Vec<FileSnapshot>,
    /// Current todo state
    pub todo_state: TodoStateSnapshot,
    /// Parent agent observations/conclusions
    pub parent_observations: Vec<String>,
    /// Relevant conversation history
    pub relevant_history: Vec<Message>,
    /// Metadata
    pub meta: ContextMeta,
}

/// Snapshot of an explored file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content_hash: String,
    pub summary: String,
    pub lines_of_code: usize,
    pub has_chinese_comments: Option<bool>,
}

/// Todo state summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TodoStateSnapshot {
    pub items: Vec<TodoItemBrief>,
    pub in_progress_id: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
}

/// Brief todo item info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItemBrief {
    pub id: String,
    pub content: String,
    pub status: String,
}

/// Context metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextMeta {
    pub parent_agent_id: String,
    pub context_created_at: DateTime<Utc>,
    pub files_explored_count: usize,
}

// ===== AgentType =====
pub enum AgentType {
    // Read-only exploration: read_file, list_dir
    Explore,
    // Testing: bash, read_file
    Test,
    // General work: bash, read_file, write_file, list_dir
    General,
}

impl AgentType {
    fn tools(&self) -> Vec<Box<dyn Tool>> {
        match self {
            AgentType::Explore => vec![Box::new(ReadFileTool), Box::new(ListDirTool)],
            AgentType::Test => vec![Box::new(BashTool), Box::new(ReadFileTool)],
            AgentType::General => vec![
                Box::new(BashTool),
                Box::new(ReadFileTool),
                Box::new(WriteFileTool),
                Box::new(ListDirTool),
            ],
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "explore" => AgentType::Explore,
            "test" => AgentType::Test,
            _ => AgentType::General,
        }
    }
}

// ===== Subagent =====
pub struct Subagent {
    llm: Arc<dyn LLMProvider>,
    tools: Vec<Box<dyn Tool>>,
    timeout_secs: u64,
    #[allow(dead_code)]
    agent_id: String,
}

impl Subagent {
    pub fn new(llm: Arc<dyn LLMProvider>, agent_type: AgentType) -> Self {
        Self {
            tools: agent_type.tools(),
            llm,
            timeout_secs: 180,
            agent_id: format!("subagent_{}", Uuid::new_v4()),
        }
    }

    // Run with timeout
    pub async fn run(&self, prompt: &str) -> anyhow::Result<String> {
        timeout(
            Duration::from_secs(self.timeout_secs),
            self.run_inner(prompt),
        )
        .await
        .map_err(|_| anyhow!("Subagent timed out after {}s", self.timeout_secs))?
    }

    async fn run_inner(&self, prompt: &str) -> anyhow::Result<String> {
        info!("Subagent start: {:.80}", prompt);

        let tools_json: Vec<serde_json::Value> = self
            .tools
            .iter()
            .map(|it| {
                let def = it.definition();
                json!({
                    "type":"function",
                    "function":{
                        "name": def.name,
                        "description":def.description,
                        "parameters":def.parameters,
                    }
                })
            })
            .collect();

        let mut messages = vec![Message {
            role: Role::User,
            content: prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        for iter in 0..40 {
            debug!("Subagent iteration {}", iter);
            let request = ChatRequest {
                messages: messages.clone(),
                temperature: Some(0.3),
                max_tokens: Some(4096),
                tools: if tools_json.is_empty() {
                    None
                } else {
                    Some(tools_json.clone())
                },
                tool_choice: None,
                stream: None,
            };

            let response = self.llm.chat(request).await?;

            match &response.tool_calls {
                Some(tool_calls) if !tool_calls.is_empty() => {
                    messages.push(Message {
                        role: Role::Assistant,
                        content: response.content.clone(),
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                    });

                    for tc in tool_calls {
                        let result = self.execute_tool(tc).await;
                        messages.push(Message {
                            role: Role::Tool,
                            content: result,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                }
                _ => {
                    info!("Subagent done after {} iterations", iter + 1);
                    return Ok(response.content);
                }
            }
        }

        warn!("Subagent max iterations reachd");
        Ok("Subagent reached max iterations.".to_string())
    }

    async fn execute_tool(&self, tc: &ToolCall) -> String {
        let name = &tc.function.name;
        let args = &tc.function.arguments;

        let tool = match self.tools.iter().find(|t| t.definition().name == *name) {
            Some(t) => t,
            None => return format!("Tool '{}' not found", name),
        };

        let args_val: serde_json::Value = match serde_json::from_str(args) {
            Ok(v) => v,
            Err(e) => return format!("Failed to parse args: {}", e),
        };

        match tool.execute(args_val).await {
            Ok(r) => {
                if r.len() > 10000 {
                    let truncated: String = r.chars().take(10000).collect();
                    format!("{}...\n[truncated]", truncated)
                } else {
                    r
                }
            }
            Err(e) => format!("Tool error: {}", e),
        }
    }
}

// ===== SpawnAgentTool =====

// Tool that lets the main LLM delegate subtasks to isolated subagents
pub struct SpawnAgentTool {
    llm: Arc<dyn LLMProvider>,
}

impl SpawnAgentTool {
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_agent".to_string(),
            description: "Spawn an isolated subagent to handle a focused subtask. \
                The subagent has its own context and a limited tool set. \
                Use 'explore' for read-only research, 'test' for running tests, \
                'general' for tasks that need file writes. \
                Returns the subagent's final response."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_type": {
                        "type": "string",
                        "enum": ["explore", "test", "general"],
                        "description": "explore=read_file+list_dir only; test=bash+read_file; general=bash+read_file+write_file+list_dir"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Clear, self-contained instructions for the subagent. Include all context it needs."
                    }
                },
                "required": ["agent_type", "prompt"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<String> {
        let agent_type_str = args["agent_type"].as_str().unwrap_or("general");
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'prompt' argument"))?;

        let agent_type = AgentType::from_str(agent_type_str);
        let subagent = Subagent::new(self.llm.clone(), agent_type);

        info!("Spawning {} subagent", agent_type_str);
        let result = subagent.run(prompt).await?;

        Ok(format!("[subagent/{} result]\n{}", agent_type_str, result))
    }
}
