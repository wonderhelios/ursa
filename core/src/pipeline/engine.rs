// Pipeline engine - TPAR architecture with GVRC mode
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};
use ursa_services::memory::store::MemoryStore;

use crate::pipeline::gvrc::{
    ActionExecutor, ExecutionMode, Planner, Reviewer, Solver, Verifier, WorkflowEvent,
};
use crate::runtime::bus::EventBus;
use ursa_llm::provider::{ChatRequest, FunctionCall, LLMProvider, Message, Role, ToolCall, StreamChunk};
use ursa_tools::{TodoManager, ToolRegistry};

use crate::context::engine::ContextEngine;
use crate::runtime::lane::{LANE_MAIN, LaneScheduler};
use crate::runtime::session::{Session, SessionManager};
use ursa_treesitter::symbol_index::SharedSymbolIndex;

/// Maximum iterations for Fast mode ReAct loop
const DEFAULT_MAX_ITERATIONS: usize = 50;
/// Maximum iterations for streaming mode
const DEFAULT_STREAM_MAX_ITERATIONS: usize = 50;

pub struct PipelineEngine {
    llm: Arc<dyn LLMProvider>,
    execution_mode: ExecutionMode,
    event_bus: EventBus,
    registry: ToolRegistry,
    todo_manager: Option<Arc<Mutex<TodoManager>>>,
    system_prompt: Option<String>,
    memory_store: Option<Arc<Mutex<MemoryStore>>>,
    conversation: Arc<Mutex<Vec<Message>>>,
    session_manager: Option<Arc<SessionManager>>,
    context_engine: Option<Arc<ContextEngine>>,
    lane_scheduler: Option<Arc<LaneScheduler>>,
    symbol_index: Option<SharedSymbolIndex>,
}

impl PipelineEngine {
    pub fn new(llm: Arc<dyn LLMProvider>, registry: ToolRegistry) -> Self {
        info!("PipelineEngine created with {} tools", registry.all().len());
        Self {
            llm,
            execution_mode: ExecutionMode::Fast,
            event_bus: EventBus::new(),
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

    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

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

    pub fn with_symbol_index(mut self, index: SharedSymbolIndex) -> Self {
        self.symbol_index = Some(index);
        self
    }

    pub fn clear_conversation(&self) {
        self.conversation.lock().unwrap().clear();
    }

    /// Load a session at runtime (for /resume command)
    pub fn load_session(&self, messages: Vec<Message>) {
        *self.conversation.lock().unwrap() = messages;
    }

    /// Get current conversation message count
    pub fn conversation_len(&self) -> usize {
        self.conversation.lock().unwrap().len()
    }
    /// Get current execution mode
    pub fn execution_mode(&self) -> ExecutionMode {
        self.execution_mode
    }


    /// Main entry point: selects execution mode
    pub async fn run(&self, input: &str) -> anyhow::Result<String> {
        match self.execution_mode {
            ExecutionMode::Fast => self.run_fast(input).await,
            ExecutionMode::Standard | ExecutionMode::Strict => self.run_gvrc(input).await,
        }
    }

    /// Stream mode: Execute with streaming output for real-time display
    pub async fn run_stream<F>(&self, input: &str, mut on_chunk: F) -> anyhow::Result<String>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let _lane_permit = if let Some(sched) = &self.lane_scheduler {
            Some(sched.permit(LANE_MAIN).await?)
        } else {
            None
        };

        info!("[Stream] Pipeline start: {}", input);

        let system_content = self.build_system_content(input);
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
            content: input.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        let tools_json = self.build_tools_json();

        // Stream loop with tool execution
        for iter in 0..DEFAULT_STREAM_MAX_ITERATIONS {
            debug!("[Stream] Iteration {}", iter);

            let request = ChatRequest {
                messages: messages.clone(),
                temperature: Some(0.3),
                max_tokens: Some(4096),
                tools: tools_json.clone(),
                tool_choice: None,
                stream: Some(true),
            };

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamChunk>();
            let llm = self.llm.clone();

            // Spawn streaming task
            let stream_task = tokio::spawn(async move {
                llm.stream_chat(request, tx).await
            });

            let mut full_content = String::new();
            let mut pending_tool_calls: std::collections::HashMap<String, (String, String)> =
                std::collections::HashMap::new();
            let mut received_tool_calls = false;

            // Process stream chunks
            while let Some(chunk) = rx.recv().await {
                match chunk {
                    StreamChunk::Content(text) => {
                        full_content.push_str(&text);
                        on_chunk(&text);
                    }
                    StreamChunk::ToolCallStart { id, name } => {
                        pending_tool_calls.insert(id, (name, String::new()));
                        received_tool_calls = true;
                    }
                    StreamChunk::ToolCallArgs { id, delta } => {
                        if let Some((_, args)) = pending_tool_calls.get_mut(&id) {
                            args.push_str(&delta);
                        }
                    }
                    StreamChunk::ToolCallEnd { .. } => {}
                    StreamChunk::Done => break,
                    StreamChunk::Error(e) => {
                        eprintln!("\n[Stream Error]: {}", e);
                    }
                }
            }

            // Wait for stream task to complete
            stream_task.await??;

            // If no tool calls, we're done
            if !received_tool_calls {
                self.save_conversation(input, &full_content).await;
                return Ok(full_content);
            }

            // Add assistant message with tool calls
            let tool_calls: Vec<ToolCall> = pending_tool_calls
                .iter()
                .map(|(id, (name, args))| ToolCall {
                    id: id.clone(),
                    call_type: "function".to_string(),
                    function: FunctionCall {
                        name: name.clone(),
                        arguments: args.clone(),
                    },
                })
                .collect();

            messages.push(Message {
                role: Role::Assistant,
                content: full_content.clone(),
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
            });

            // Execute tools and add results
            for tc in &tool_calls {
                let result = self.execute_tool(tc).await;

                // Show tool execution
                on_chunk(&format!("\n[Executing: {}]\n", tc.function.name));
                if tc.function.name == "todo_write" {
                    on_chunk(&result);
                }

                messages.push(Message {
                    role: Role::Tool,
                    content: result,
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                });
            }

            // Continue loop for next iteration with tool results
            on_chunk("\n");
        }

        warn!("[Stream] Max iterations reached");
        Ok("Max iterations reached. Please retry with a simpler request.".to_string())
    }

    /// Fast mode: Direct ReAct loop (existing behavior)
    async fn run_fast(&self, user_input: &str) -> anyhow::Result<String> {
        let _lane_permit = if let Some(sched) = &self.lane_scheduler {
            Some(sched.permit(LANE_MAIN).await?)
        } else {
            None
        };

        info!("[Fast] Pipeline start: {}", user_input);

        let system_content = self.build_system_content(user_input);
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

        let tools_json = self.build_tools_json();

        for iter in 0..DEFAULT_MAX_ITERATIONS {
            debug!("[Fast] Iteration {}", iter);

            // Add nag message after first iteration if there are stuck todos
            if let Some(nag_msg) = iter
                .gt(&0)
                .then(|| self.build_nag_message())
                .flatten()
            {
                messages.push(nag_msg);
            }

            let request = ChatRequest {
                messages: messages.clone(),
                temperature: Some(0.3),
                max_tokens: Some(4096),
                tools: tools_json.clone(),
                tool_choice: None,
                stream: None,
            };

            let response = self.llm.chat(request).await?;
            debug!("[Fast] LLM response: {:?}", response);

            match &response.tool_calls {
                Some(tool_calls) if !tool_calls.is_empty() => {
                    info!("[Fast] Tool calls: {}", tool_calls.len());

                    messages.push(Message {
                        role: Role::Assistant,
                        content: response.content.clone(),
                        tool_calls: Some(tool_calls.clone()),
                        tool_call_id: None,
                    });

                    if !response.content.is_empty() {
                        println!("{}", response.content);
                    }

                    for tc in tool_calls {
                        let result = self.execute_tool(tc).await;

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
                    info!("[Fast] Done after {} iterations", iter + 1);
                    let final_response = response.content.clone();
                    self.save_conversation(user_input, &final_response).await;
                    return Ok(final_response);
                }
            }
        }

        warn!("[Fast] Max iterations reached");
        Ok("Max iterations reached. Please retry with a simpler request.".to_string())
    }

    /// GVRC mode: Generate-Verify-Refine-Commit loop
    async fn run_gvrc(&self, user_input: &str) -> anyhow::Result<String> {
        let _lane_permit = if let Some(sched) = &self.lane_scheduler {
            Some(sched.permit(LANE_MAIN).await?)
        } else {
            None
        };

        info!("[GVRC] Pipeline start: {}", user_input);
        self.event_bus.publish_workflow(WorkflowEvent::Started {
            mode: self.execution_mode,
        });

        // 1. PLANNING
        self.event_bus
            .publish_workflow(WorkflowEvent::PlanningStarted {
                goal: user_input.to_string(),
            });

        let planner = Planner::new(self.llm.clone());
        let available_tools: Vec<String> = self
            .registry
            .all()
            .iter()
            .map(|t| t.definition().name.clone())
            .collect();

        let plan = planner
            .create_plan(user_input, self.execution_mode, &available_tools)
            .await?;

        self.event_bus
            .publish_workflow(WorkflowEvent::PlanningCompleted {
                stage_count: plan.stages.len(),
            });

        info!("[GVRC] Plan created with {} stages", plan.stages.len());

        // 2. EXECUTION: GVRC loop for each stage (inline to avoid lifetime issues)
        use crate::pipeline::gvrc::{FailedAttempt, Solution, VerificationResult};

        let mut all_success = true;
        let mut total_iterations = 0;
        let mut stage_results: Vec<(String, bool, usize)> = Vec::new();

        for stage in &plan.stages {
            info!("[GVRC] Executing stage: {}", stage.id);

            self.event_bus
                .publish_workflow(WorkflowEvent::StageStarted {
                    stage_id: stage.id.clone(),
                    iteration: 0,
                });

            let mut attempts: Vec<FailedAttempt> = Vec::new();
            let mut stage_success = false;
            let mut accepted_solution: Option<Solution> = None;

            for iteration in 1..=stage.max_iterations {
                debug!(
                    "[{}] GVRC iteration {}/{}",
                    stage.id, iteration, stage.max_iterations
                );

                // GENERATE
                let solver = Solver::new(self.llm.clone());
                let solution = solver
                    .solve(&stage.goal, &stage.available_tools, &attempts)
                    .await?;

                self.event_bus
                    .publish_workflow(WorkflowEvent::SolverCompleted {
                        stage_id: stage.id.clone(),
                        iteration,
                        action_count: solution.planned_actions.len(),
                    });

                // VERIFY
                self.event_bus
                    .publish_workflow(WorkflowEvent::VerificationStarted {
                        stage_id: stage.id.clone(),
                        criterion_count: stage.acceptance_criteria.len(),
                    });

                let verifier = Verifier::new(self.llm.clone());
                let verification = verifier.verify(&solution, &stage.acceptance_criteria).await?;
                let passed = verification.is_passed();

                self.event_bus
                    .publish_workflow(WorkflowEvent::VerificationCompleted {
                        stage_id: stage.id.clone(),
                        passed,
                    });

                if passed {
                    info!("[GVRC] Stage {} completed in {} iterations", stage.id, iteration);
                    self.event_bus
                        .publish_workflow(WorkflowEvent::StageCompleted {
                            stage_id: stage.id.clone(),
                            iterations: iteration,
                        });
                    stage_success = true;
                    total_iterations += iteration;
                    accepted_solution = Some(solution);
                    break;
                }

                // REFINE
                let hints = match &verification {
                    VerificationResult::Failed { hints, .. } => hints.clone(),
                    _ => String::new(),
                };

                warn!("[GVRC] Verification failed, refining: {}", hints);
                self.event_bus
                    .publish_workflow(WorkflowEvent::RefinementHints {
                        stage_id: stage.id.clone(),
                        hints: hints.clone(),
                    });

                let failures = match &verification {
                    VerificationResult::Failed { failures, .. } => failures.clone(),
                    _ => Vec::new(),
                };

                attempts.push(FailedAttempt {
                    iteration,
                    solution,
                    failures,
                    hints,
                });
            }

            // EXECUTE accepted solution
            if stage_success {
                if let Some(ref solution) = accepted_solution {
                    info!(
                        "[GVRC] Executing {} planned actions for stage {}",
                        solution.planned_actions.len(),
                        stage.id
                    );

                    let executor = ActionExecutor::new(&self.registry);
                    let exec_results = executor.execute_actions(&solution.planned_actions).await;

                    // Check if any execution failed
                    let failed_count = exec_results.iter().filter(|r| r.is_err()).count();
                    let total_count = exec_results.len();

                    if failed_count > 0 {
                        warn!(
                            "[GVRC] {}/{} actions failed in stage {}",
                            failed_count, total_count, stage.id
                        );
                        // In Strict mode, any action failure marks stage as failed
                        if self.execution_mode == ExecutionMode::Strict {
                            stage_success = false;
                            all_success = false;
                            info!("[GVRC] Stage {} marked as failed due to action errors", stage.id);
                        }
                    }

                    if stage_success {
                        info!("[GVRC] Stage {} completed successfully", stage.id);
                    }
                }
            } else {
                let last_error = attempts
                    .last()
                    .map(|a| a.hints.clone())
                    .unwrap_or_else(|| "Max iterations reached".to_string());

                warn!(
                    "[GVRC] Stage {} failed after {} attempts: {}",
                    stage.id,
                    attempts.len(),
                    last_error
                );
                self.event_bus
                    .publish_workflow(WorkflowEvent::StageFailed {
                        stage_id: stage.id.clone(),
                        attempts: attempts.len(),
                    });

                all_success = false;
                total_iterations += attempts.len();

                if self.execution_mode == ExecutionMode::Strict {
                    self.event_bus
                        .publish_workflow(WorkflowEvent::Completed { success: false });
                    return Err(anyhow::anyhow!("Stage {} failed: {}", stage.id, last_error));
                }
            }

            // Collect stage result for review
            let final_iterations = if stage_success {
                accepted_solution.as_ref().map(|_| 1).unwrap_or(0) + attempts.len()
            } else {
                attempts.len()
            };
            stage_results.push((stage.id.clone(), stage_success, final_iterations));
        }

        // 3. REVIEW
        if !stage_results.is_empty() {
            let reviewer = Reviewer::new(self.llm.clone());
            let stage_ids: Vec<String> = plan.stages.iter().map(|s| s.id.clone()).collect();
            match reviewer.review(&stage_ids, &stage_results).await {
                Ok(review) => {
                    info!("[GVRC] Review: {}", review.summary);

                    // Save to memory if available
                    if let Some(ref store) = self.memory_store {
                        let mut store = store.lock().unwrap();
                        for (key, value) in review.memory_updates {
                            if !key.is_empty() {
                                let _ = store.write(&format!("{}: {}", key, value), vec!["gvrc".to_string()]);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("[GVRC] Review failed: {}", e);
                }
            }
        }

        // 4. COMPLETION
        self.event_bus.publish_workflow(WorkflowEvent::Completed {
            success: all_success,
        });

        let summary = format!(
            "GVRC execution completed. Success: {}, Total iterations: {}, Stages: {}",
            all_success,
            total_iterations,
            plan.stages.len()
        );

        info!("[GVRC] {}", summary);
        Ok(summary)
    }

    /// Build tools JSON for LLM
    fn build_tools_json(&self) -> Option<Vec<serde_json::Value>> {
        let tools = self.registry.all();
        if tools.is_empty() {
            return None;
        }

        Some(
            tools
                .iter()
                .map(|t| {
                    let def = t.definition();
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": def.name,
                            "description": def.description,
                            "parameters": def.parameters,
                        }
                    })
                })
                .collect(),
        )
    }

    /// Build system message content
    fn build_system_content(&self, user_input: &str) -> String {
        let mut content = self
            .system_prompt
            .clone()
            .unwrap_or_else(|| include_str!("./prompts/system.md").to_string());

        // Inject symbol index
        if let Some(ref index) = self.symbol_index {
            if let Ok(idx) = index.read() {
                let defs = idx.all_definitions();
                if !defs.is_empty() {
                    content.push_str("\n\n## Key Code Symbols\n");
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
        }

        // Inject workspace context
        if let Some(ctx) = &self.context_engine {
            let workspace = ctx.build_context();
            if !workspace.is_empty() {
                content.push_str(&format!("\n\n{}", workspace));
            }
        }

        // Inject memories
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

        // Inject todos
        if let Some(mgr) = &self.todo_manager {
            let mgr = mgr.lock().unwrap();
            let rendered = mgr.render();
            if !rendered.is_empty() {
                content.push_str(&format!("\n\n## Current Tasks\n{}", rendered));
            }
        }

        content
    }

    /// Build nag message for stuck todos
    fn build_nag_message(&self) -> Option<Message> {
        let mut mgr = self.todo_manager.as_ref()?.lock().unwrap();
        let nag_text = mgr.do_nag()?;
        Some(Message {
            role: Role::User,
            content: nag_text,
            tool_calls: None,
            tool_call_id: None,
        })
    }

    /// Execute a single tool
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
                if result.len() > 10000 {
                    let truncated: String = result.chars().take(10000).collect();
                    format!("{}...\n[truncated]", truncated)
                } else {
                    result
                }
            }
            Err(e) => format!("Tool error: {}", e),
        }
    }

    /// Save conversation to session
    async fn save_conversation(&self, user_input: &str, assistant_response: &str) {
        {
            let mut conv = self.conversation.lock().unwrap();
            conv.push(Message {
                role: Role::User,
                content: user_input.to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
            conv.push(Message {
                role: Role::Assistant,
                content: assistant_response.to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
        }

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
    }
}
