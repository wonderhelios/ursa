//! Workflow context

use super::definition::{FailedAttempt, Stage};

/// Shared workflow context
#[derive(Debug, Clone, Default)]
pub struct WorkflowContext {
    pub project_summary: String,
}

impl WorkflowContext {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Per-stage context
#[derive(Debug, Clone)]
pub struct StageContext<'a> {
    pub stage: &'a Stage,
    pub previous_attempts: &'a [FailedAttempt],
}

impl<'a> StageContext<'a> {
    pub fn new(stage: &'a Stage, previous_attempts: &'a [FailedAttempt]) -> Self {
        Self {
            stage,
            previous_attempts,
        }
    }

    pub fn build_prompt(&self) -> String {
        let mut prompt = format!("Goal: {}\n\n", self.stage.goal);

        if !self.previous_attempts.is_empty() {
            prompt.push_str("Previous attempts failed:\n");
            for (i, attempt) in self.previous_attempts.iter().enumerate() {
                prompt.push_str(&format!(
                    "{}. {} - {}\n",
                    i + 1,
                    attempt.solution.reasoning,
                    attempt.hints
                ));
            }
        }
        prompt
    }
}
