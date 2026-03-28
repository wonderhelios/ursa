//! Verifier - validates solutions against acceptance criteria.

use crate::pipeline::gvrc::types::{CheckType, Criterion, Solution, VerificationResult};
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, warn};
use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role};

/// Execution result for post-execution verification
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Verifier checks if a solution meets acceptance criteria.
pub struct Verifier {
    llm: Arc<dyn LLMProvider>,
}

impl Verifier {
    /// Create a new Verifier.
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Verify a solution after execution (post-execution verification)
    pub async fn verify_execution(
        &self,
        solution: &Solution,
        criteria: &[Criterion],
        exec_results: &[ExecutionResult],
    ) -> Result<VerificationResult> {
        info!("Verifying execution against {} criteria", criteria.len());

        if criteria.is_empty() {
            return Ok(VerificationResult::Passed);
        }

        let mut failures = Vec::new();

        for criterion in criteria {
            debug!("Checking criterion: {}", criterion.id);

            let passed = match &criterion.check {
                CheckType::Automated { command } => {
                    // Run the check command after execution
                    self.run_automated_check(command).await
                }
                CheckType::Llm { prompt } => {
                    // Use LLM to verify with execution context
                    self.run_llm_check_with_results(prompt, solution, exec_results).await
                }
            };

            if !passed {
                failures.push(format!("{}: {}", criterion.id, criterion.description));
            }
        }

        if failures.is_empty() {
            info!("All criteria passed");
            Ok(VerificationResult::Passed)
        } else {
            let hints = format!("Failed criteria: {}", failures.join(", "));
            warn!("Verification failed: {}", hints);
            Ok(VerificationResult::Failed { failures, hints })
        }
    }

    /// Pre-execution verification: performs lightweight sanity checks before execution
    ///
    /// Checks for:
    /// - Solution has planned actions
    /// - Each action has valid tool name and parameters
    /// - Reasoning is not empty
    /// - Expected outcome is clearly stated
    pub async fn verify(
        &self,
        solution: &Solution,
        criteria: &[Criterion],
    ) -> Result<VerificationResult> {
        info!("Pre-execution verification of {} criteria", criteria.len());

        let mut failures = Vec::new();

        // Check 1: Solution must have actions
        if solution.planned_actions.is_empty() {
            failures.push("No planned actions".to_string());
        }

        // Check 2: Reasoning should be substantive (at least 20 chars)
        if solution.reasoning.len() < 20 {
            failures.push("Reasoning too brief".to_string());
        }

        // Check 3: Expected outcome should be stated
        if solution.expected_outcome.is_empty() || solution.expected_outcome.len() < 10 {
            failures.push("Expected outcome unclear".to_string());
        }

        // Check 4: Each planned action should have valid tool name and non-empty args
        for (i, action) in solution.planned_actions.iter().enumerate() {
            if action.tool.is_empty() {
                failures.push(format!("Action {}: missing tool name", i + 1));
            }
            // Note: We don't validate if tool exists here - that's done during execution
            // But we can check if args are valid JSON
            if !action.args.is_null() && !action.args.is_object() {
                failures.push(format!("Action {}: args should be an object", i + 1));
            }
        }

        // Check 5: Validate against explicit criteria (lightweight checks only)
        for criterion in criteria {
            // For pre-execution, only check criteria that don't require execution
            // For example, check if the solution mentions required tools
            match &criterion.check {
                CheckType::Automated { command } if command.starts_with("exists ") => {
                    // File existence check can be done pre-execution
                    let path = command.trim_start_matches("exists ");
                    if !std::path::Path::new(path).exists() {
                        failures.push(format!("{}: required file not found", criterion.id));
                    }
                }
                _ => {
                    // Other checks are deferred to post-execution verification
                    debug!("Deferring criterion {} to post-execution", criterion.id);
                }
            }
        }

        if failures.is_empty() {
            info!("Pre-execution checks passed");
            Ok(VerificationResult::Passed)
        } else {
            let hints = format!("Pre-execution failures: {}", failures.join("; "));
            warn!("{}", hints);
            Ok(VerificationResult::Failed { failures, hints })
        }
    }

    async fn run_automated_check(&self, command: &str) -> bool {
        info!("Running automated check: {}", command);

        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
        {
            Ok(output) => {
                let success = output.status.success();
                if !success {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    debug!("Check failed: {}", stderr);
                }
                success
            }
            Err(e) => {
                warn!("Failed to execute check '{}': {}", command, e);
                false
            }
        }
    }

    async fn run_llm_check_with_results(
        &self,
        check_prompt: &str,
        solution: &Solution,
        exec_results: &[ExecutionResult],
    ) -> bool {
        let results_str = exec_results
            .iter()
            .map(|r| {
                format!(
                    "Action: {}\nSuccess: {}\nOutput: {}\nError: {:?}",
                    if r.success { "✓" } else { "✗" },
                    r.success,
                    r.output.chars().take(500).collect::<String>(),
                    r.error
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");

        let prompt = format!(
            "Verify if the following execution results meet the criterion.\n\n\
            Criterion: {}\n\n\
            Solution: {}\n\n\
            Expected Outcome: {}\n\n\
            Execution Results:\n{}\n\n\
            Does the execution successfully meet the criterion? Reply with PASS or FAIL and briefly explain why.",
            check_prompt,
            solution.reasoning,
            solution.expected_outcome,
            results_str
        );

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.1),
            max_tokens: Some(200),
            tools: None,
            tool_choice: None,
            stream: None,
        };

        match self.llm.chat(request).await {
            Ok(response) => {
                let content = response.content.trim().to_uppercase();
                content.contains("PASS") && !content.contains("FAIL")
            }
            Err(e) => {
                warn!("LLM check failed: {}", e);
                false
            }
        }
    }
}
