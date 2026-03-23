// Pipeline engine - TPAR architecture
use serde_json::json;
use std::sync::{Arc, Mutex};
use tracing::{debug, field::debug, info, warn};
use ursa_services::memory::store::MemoryStore;

use ursa_llm::provider::{ChatRequest, ChatResponse, LLMProvider, Message, Role, ToolCall};
use ursa_tools::{TodoManager, ToolRegistry};
use ursa_tools::{Tool, ToolDefinition, tools};

use crate::context::engine::{self, ContextEngine};
use crate::runtime::lane::{LANE_MAIN, LaneScheduler};
use crate::runtime::session::{Session, SessionManager};
use ursa_treesitter::symbol_index::SymbolIndex;

pub struct PipelineEngine {
    llm: Arc<dyn LLMProvider>,
    registry: ToolRegistry,
    todo_manager: Option<Arc<Mutex<TodoManager>>>,
    system_prompt: Option<String>,
    memory_store: Option<Arc<Mutex<MemoryStore>>>,
    conversation: Arc<Mutex<Vec<Message>>>,
    session_manager: Option<Arc<SessionManager>>,
    context_engine: Option<Arc<ContextEngine>>,
    lane_scheduler: Option<Arc<LaneScheduler>>,
    symbol_index: Option<Arc<SymbolIndex>>,
}

impl PipelineEngine {
    pub fn new(llm: Arc<dyn LLMProvider>, registry: ToolRegistry) -> Self {
        info!("PiplineEngine created with {} tools", registry.all().len());
        Self {
            llm,
            registry,
            todo_manager: None,
            system_prompt: None,
            memory_store: None,
            conversation: Arc::new(Mutex::new(vec![])),
            session_manager: None,
            context_engine: None,
            lane_scheduler: None,
            symbol_index: None,
        }
    }

    // attach a shared TodoManager for todo state injection and nag
    pub fn with_todos(mut self, manager: Arc<Mutex<TodoManager>>) -> Self {
        self.todo_manager = Some(manager);
        self
    }

    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    pub fn with_memory(mut self, store: Arc<Mutex<MemoryStore>>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_session(mut self, manager: Arc<SessionManager>, existing: Vec<Message>) -> Self {
        self.session_manager = Some(manager);
        *self.conversation.lock().unwrap() = existing;
        self
    }

    pub fn with_context(mut self, engine: Arc<ContextEngine>) -> Self {
        self.context_engine = Some(engine);
        self
    }

    pub fn with_lanes(mut self, scheduler: Arc<LaneScheduler>) -> Self {
        self.lane_scheduler = Some(scheduler);
        self
    }

    pub fn with_symbol_index(mut self, index: Arc<SymbolIndex>) -> Self {
        self.symbol_index = Some(index);
        self
    }

    pub fn clear_conversation(&self) {
        self.conversation.lock().unwrap().clear();
    }

    pub async fn run(&self, user_input: &str) -> anyhow::Result<String> {
        // Serialize concurrent calls through LANE_MAIN (holds permit until run() returns)
        let _lane_permit = if let Some(sched) = &self.lane_scheduler {
            Some(sched.permit(LANE_MAIN).await?)
        } else {
            None
        };

        info!("Pipeline start: {}", user_input);

        let system_content = self.build_system_content(user_input);

        // messages: [System] + conversation history + [current User]
        let mut messages = vec![Message {
            role: Role::System,
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
        }];

        {
            let conv = self.conversation.lock().unwrap();
            messages.extend(conv.clone());
        }

        messages.push(Message {
            role: Role::User,
            content: user_input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

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
                            json!({
                                "type":"function",
                                "function":{
                                    "name":def.name,
                                    "description":def.description,
                                    "parameters":def.parameters,
                            }})
                        })
                        .collect(),
                )
            }
        };

        for iter in 0..30 {
            debug!("Iteration {}", iter);

            let request = ChatRequest {
                messages: messages.clone(),
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
                    messages.push(Message {
                        role: Role::Assistant,
                        content: response.content.clone(),
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                    });

                    // Print intermediate narration so user sees it in real time
                    if !response.content.is_empty() {
                        println!("{}", response.content);
                    }

                    // Execute tools sequentially
                    for tc in tool_calls {
                        let result = self.execute_tool(tc).await;

                        // Print todo_write results so the user sees task list updates
                        if tc.function.name == "todo_write" {
                            println!("{}", result);
                        }

                        messages.push(Message {
                            role: Role::Tool,
                            content: result,
                            tool_calls: None,
                            tool_call_id: Some(tc.id.clone()),
                        });
                    }
                }
                _ => {
                    info!("Done after {} iterations", iter + 1);
                    let final_response = response.content.clone();

                    // append this exchange to conversation history
                    {
                        let mut cov = self.conversation.lock().unwrap();
                        cov.push(Message {
                            role: Role::User,
                            content: user_input.to_string(),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                        cov.push(Message {
                            role: Role::Assistant,
                            content: final_response.clone(),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    // save session if manager is configured
                    if let Some(sm) = &self.session_manager {
                        let conv = self.conversation.lock().unwrap();
                        let session = Session {
                            id: "current".to_string(),
                            created_at: chrono::Utc::now(),
                            messages: conv.clone(),
                        };
                        if let Err(e) = sm.save(&session) {
                            warn!("Failed to save session: {}", e);
                        }
                    }
                    return Ok(final_response);
                }
            }
        }

        warn!("Max iterations reached");
        Ok("Reach max iterations,please retry simpler request".to_string())
    }

    /// build system message content, injecting current todos if present
    fn build_system_content(&self, user_input: &str) -> String {
        // 1. base: bootstrap prompt or fallback to built-in system.md
        let mut content = self
            .system_prompt
            .clone()
            .unwrap_or_else(|| include_str!("./prompts/system.md").to_string());

        // 2：inject key symbol
        if let Some(ref index) = self.symbol_index {
            let defs = index.all_definitions();
            if !defs.is_empty() {
                content.push_str("\n\n## Key Code Symbols (search these with symbol_search)\n");
                for def in defs.iter().take(20) {
                    content.push_str(&format!(
                        "- {} `{}` ({}:{})\n",
                        def.kind,
                        def.name,
                        def.file.file_name().unwrap_or_default().to_string_lossy(),
                        def.line + 1
                    ));
                }
            }
        }
        // 3. inject workspace file listing
        if let Some(ctx) = &self.context_engine {
            let workspace = ctx.build_context();
            if !workspace.is_empty() {
                content.push_str(&format!("\n\n{}", workspace));
            }
        }

        // 4. inject relevant memories
        if let Some(store) = &self.memory_store {
            let store = store.lock().unwrap();
            let memories = store.search(user_input, 5);
            if !memories.is_empty() {
                content.push_str("\n\n## Relevant Memories\n");
                for m in memories {
                    content.push_str(&format!("- {}\n", m.content));
                }
            }
        }

        // 5. inject current todos
        if let Some(mgr) = &self.todo_manager {
            let mgr = mgr.lock().unwrap();
            let rendered = mgr.render();
            if !rendered.is_empty() {
                content.push_str(&format!("\n\n## Current Tasks\n{}", rendered));
            }
        }

        content
    }

    /// Build a nag reminder message if a todo is stuck in InProgress too long
    fn build_nag_message(&self) -> Option<Message> {
        let mgr = self.todo_manager.as_ref()?.lock().unwrap();
        if !mgr.need_nag() {
            return None;
        }
        Some(Message {
            role: Role::User,
            content: "Reminder: you have a task marked in_progress for over 5 minutes. \
                Please update the todo list - mark it completed if done, \
                or break it into smaller steps."
                .to_string(),
            tool_calls: None,
            tool_call_id: None,
        })
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
