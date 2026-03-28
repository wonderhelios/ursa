//! Planner - breaks down user goals into stages.

use crate::pipeline::gvrc::types::{CheckType, Criterion, ExecutionMode, Plan, Stage};
use crate::pipeline::prompts;
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, warn};
use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role};

/// Planner creates execution plans from user goals.
pub struct Planner {
    llm: Arc<dyn LLMProvider>,
}

impl Planner {
    /// Create a new Planner.
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Create a plan for the given goal.
    pub async fn create_plan(
        &self,
        goal: &str,
        mode: ExecutionMode,
        available_tools: &[String],
    ) -> Result<Plan> {
        info!("Creating plan for: {} (mode: {:?})", goal, mode);

        match mode {
            ExecutionMode::Fast => Ok(Plan::new(vec![Stage::new("direct", goal)])),
            ExecutionMode::Standard | ExecutionMode::Strict => {
                self.plan_with_llm(goal, available_tools).await
            }
        }
    }

    async fn plan_with_llm(&self, goal: &str, tools: &[String]) -> Result<Plan> {
        // Limit tools list to prevent prompt overflow
        let tools_limited: Vec<String> = tools.iter().take(8).cloned().collect();

        let prompt = prompts::PLANNER
            .replace("{user_goal}", goal)
            .replace("{available_tools}", &tools_limited.join(", "));

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.2),
            max_tokens: Some(8192),  // Increased for longer plans
            tools: None,
            tool_choice: None,
            stream: None,
        };

        let response = self.llm.chat(request).await?;

        // Log the actual response for debugging
        debug!("Planner LLM response (first 1000 chars): {}",
            response.content.chars().take(1000).collect::<String>());

        self.parse_plan(&response.content)
    }

    fn parse_plan(&self, content: &str) -> Result<Plan> {
        // Clean up the response - remove markdown and find JSON
        let cleaned = content
            .replace("```json", "")
            .replace("```", "")
            .trim()
            .to_string();

        debug!("Parsing plan (first 500 chars): {}", cleaned.chars().take(500).collect::<String>());

        // Try to extract JSON object
        let json_start = cleaned.find('{').unwrap_or(0);
        let json_str = &cleaned[json_start..];

        // Try to find complete JSON (handle truncated responses)
        let json_str = if let Some(end) = json_str.rfind('}') {
            &json_str[..=end]
        } else {
            warn!("JSON appears truncated, attempting partial parse");
            json_str
        };

        // Parse as Value first for flexibility
        let value: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse plan JSON: {}", e);
                warn!("Raw content (first 300 chars): {}", json_str.chars().take(300).collect::<String>());
                // Return default single-stage plan
                return Ok(Plan::new(vec![
                    Stage::new("main", "Analyze and execute the task")
                        .with_max_iterations(10)
                ]));
            }
        };

        // Extract stages
        let stages_json = value.get("stages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut stages = Vec::new();

        for s in stages_json {
            let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("stage");
            let goal = s.get("goal").and_then(|v| v.as_str()).unwrap_or("");

            let mut stage = Stage::new(id, goal);

            if let Some(max_iter) = s.get("max_iterations").and_then(|v| v.as_u64()) {
                stage = stage.with_max_iterations(max_iter as usize);
            } else {
                stage = stage.with_max_iterations(10);
            }

            // Parse criteria with defaults
            if let Some(criteria) = s.get("acceptance_criteria").and_then(|v| v.as_array()) {
                for (i, c) in criteria.iter().enumerate() {
                    let cid_default = format!("ac{}", i);
                    let cid = c.get("id").and_then(|v| v.as_str()).unwrap_or(&cid_default);
                    let desc = c.get("description").and_then(|v| v.as_str()).unwrap_or("Check completion");

                    let check = c.get("check")
                        .and_then(|chk| {
                            if let Some(cmd) = chk.get("command").and_then(|v| v.as_str()) {
                                Some(CheckType::Automated { command: cmd.to_string() })
                            } else { chk.get("prompt").and_then(|v| v.as_str()).map(|prompt| CheckType::Llm { prompt: prompt.to_string() }) }
                        })
                        .unwrap_or_else(|| CheckType::Llm { prompt: format!("Verify: {}", desc) });

                    stage = stage.with_criterion(Criterion::new(cid, desc, check));
                }
            }

            // Default criterion if none provided
            if stage.acceptance_criteria.is_empty() {
                stage = stage.with_criterion(Criterion::new(
                    "ac1",
                    "Task completed successfully",
                    CheckType::Llm { prompt: "Verify the task is complete".to_string() }
                ));
            }

            if let Some(tools) = s.get("available_tools").and_then(|v| v.as_array()) {
                let tool_names: Vec<String> = tools
                    .iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect();
                stage = stage.with_tools(tool_names);
            }

            stages.push(stage);
        }

        // If no stages parsed, create default
        if stages.is_empty() {
            warn!("No stages parsed from LLM response, using default");
            stages.push(Stage::new("main", "Execute the task").with_max_iterations(10));
        }

        let mut plan = Plan::new(stages);

        if let Some(strategy) = value.get("overall_strategy").and_then(|v| v.as_str()) {
            plan = plan.with_strategy(strategy);
        }

        info!("Created plan with {} stages", plan.stages.len());
        Ok(plan)
    }
}
