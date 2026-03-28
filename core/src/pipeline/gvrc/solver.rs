//! Solver - generates solutions using LLM.

use crate::pipeline::gvrc::FailedAttempt;
use crate::pipeline::gvrc::types::{PlannedAction, Solution};
use crate::pipeline::prompts;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tracing::{debug, info};
use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role};

/// Solver generates solutions for a given stage goal.
pub struct Solver {
    llm: Arc<dyn LLMProvider>,
}

impl Solver {
    /// Create a new Solver.
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Generate a solution for the stage goal.
    pub async fn solve(
        &self,
        stage_goal: &str,
        available_tools: &[String],
        previous_attempts: &[FailedAttempt],
    ) -> Result<Solution> {
        info!("Solver generating solution for: {}", stage_goal);

        let previous_section = if previous_attempts.is_empty() {
            "".to_string()
        } else {
            let mut s = "Previous attempts failed:\n\n".to_string();
            for a in previous_attempts {
                s.push_str(&format!(
                    "Attempt {}:\nReasoning: {}\nFailures: {:?}\nHints: {}\n\n",
                    a.iteration, a.solution.reasoning, a.failures, a.hints
                ));
            }
            s
        };

        let prompt = prompts::SOLVER
            .replace("{stage_goal}", stage_goal)
            .replace("{available_tools}", &available_tools.join(", "))
            .replace("{previous_attempts_section}", &previous_section);

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.3),
            max_tokens: Some(4096),
            tools: None,
            tool_choice: None,
            stream: None,
        };

        let response = self.llm.chat(request).await?;
        debug!("Solver response: {}", response.content);

        self.parse_solution(&response.content)
    }

    fn parse_solution(&self, content: &str) -> Result<Solution> {
        let json_str = extract_json(content);

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| anyhow!("Failed to parse solver response: {}\nRaw: {}", e, content))?;

        let mut solution = Solution::default();

        if let Some(r) = parsed.get("reasoning").and_then(|v| v.as_str()) {
            solution.reasoning = r.to_string();
        }
        if let Some(e) = parsed.get("expected_outcome").and_then(|v| v.as_str()) {
            solution.expected_outcome = e.to_string();
        }
        if let Some(actions) = parsed.get("planned_actions").and_then(|v| v.as_array()) {
            for act in actions {
                solution.planned_actions.push(PlannedAction {
                    tool: act
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    args: act.get("args").cloned().unwrap_or(serde_json::Value::Null),
                    purpose: act
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }

        info!(
            "Solver generated {} actions",
            solution.planned_actions.len()
        );
        Ok(solution)
    }
}

fn extract_json(content: &str) -> &str {
    if let Some(start) = content.find("```json") {
        let after_start = &content[start + 7..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim();
        }
    }
    if let Some(start) = content.find("```") {
        let after_start = &content[start + 3..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim();
        }
    }
    content.trim()
}
