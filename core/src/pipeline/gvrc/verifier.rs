//! Verifier - validates solutions against acceptance criteria.

use crate::pipeline::gvrc::types::{CheckType, Criterion, Solution, VerificationResult};
use crate::pipeline::prompts;
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info, warn};
use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role};

/// Verifier checks if a solution meets acceptance criteria.
pub struct Verifier {
    llm: Arc<dyn LLMProvider>,
}

impl Verifier {
    /// Create a new Verifier.
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Verify a solution against the given criteria.
    pub async fn verify(
        &self,
        solution: &Solution,
        criteria: &[Criterion],
    ) -> Result<VerificationResult> {
        info!("Verifying solution against {} criteria", criteria.len());

        if criteria.is_empty() {
            return Ok(VerificationResult::Passed);
        }

        let mut failures = Vec::new();

        for criterion in criteria {
            debug!("Checking criterion: {}", criterion.id);

            // Smart check type detection based on description
            let passed = if criterion.description.to_lowercase().contains("no warning")
                || criterion.description.to_lowercase().contains("warning free")
                || criterion.description.to_lowercase().contains("clean build")
            {
                // For "no warnings" criteria, cargo check should have 0 warnings
                // Check if cargo check output contains "warning:"
                self.run_cargo_check_no_warnings().await
            } else if criterion.description.to_lowercase().contains("compile")
                || criterion.description.to_lowercase().contains("build")
            {
                // For compile success, just check exit code
                self.run_automated_check("cargo check").await
            } else {
                match &criterion.check {
                    CheckType::Automated { command } => self.run_automated_check(command).await,
                    CheckType::Llm { prompt } => self.run_llm_check(prompt, solution).await,
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

    async fn run_cargo_check_no_warnings(&self) -> bool {
        info!("Running cargo check to verify no warnings");

        match tokio::process::Command::new("cargo")
            .args(&["check", "--message-format=short"])
            .output()
            .await
        {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let combined = format!("{} {}", stdout, stderr);

                // Check if there are any warnings
                let has_warnings = combined.contains("warning:") || combined.contains("Warning:");

                if has_warnings {
                    debug!("Found warnings in cargo check output");
                    false
                } else {
                    info!("No warnings found");
                    true
                }
            }
            Err(e) => {
                warn!("Failed to run cargo check: {}", e);
                false
            }
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

    async fn run_llm_check(&self, check_prompt: &str, solution: &Solution) -> bool {
        let criteria_str = format!("- {}\n", check_prompt);

        let solution_str = format!(
            "Reasoning: {}\nExpected Outcome: {}\nPlanned Actions: {:?}",
            solution.reasoning, solution.expected_outcome, solution.planned_actions
        );

        let prompt = prompts::VERIFIER
            .replace("{acceptance_criteria}", &criteria_str)
            .replace("{solution}", &solution_str)
            .replace("{expected_outcome}", &solution.expected_outcome);

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.1),
            max_tokens: Some(100),
            tools: None,
            tool_choice: None,
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
