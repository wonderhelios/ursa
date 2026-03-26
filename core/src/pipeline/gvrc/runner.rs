//! GVRC runner - executes the Generate-Verify-Refine-Commit loop.

use crate::pipeline::gvrc::WorkflowEvent;
use crate::pipeline::gvrc::types::{
    Criterion, FailedAttempt, Solution, Stage, StageResult, VerificationResult,
};
use crate::runtime::bus::EventBus;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info, warn};

/// GVRC loop runner.
pub struct GvrcRunner {
    event_bus: EventBus,
}

impl GvrcRunner {
    /// Create a new GVRC runner.
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    /// Execute a single stage with GVRC loop.
    pub async fn execute_stage<S, V>(
        &self,
        stage: &Stage,
        mut solver: S,
        mut verifier: V,
    ) -> Result<StageResult>
    where
        S: FnMut(
            &str,
            &[String],
            &[FailedAttempt],
        ) -> Pin<Box<dyn Future<Output = Result<Solution>> + Send>>,
        V: FnMut(
            &Solution,
            &[Criterion],
        ) -> Pin<Box<dyn Future<Output = Result<VerificationResult>> + Send>>,
    {
        info!("[{}] Starting GVRC loop", stage.id);
        self.event_bus
            .publish_workflow(WorkflowEvent::StageStarted {
                stage_id: stage.id.clone(),
                iteration: 0,
            });

        let mut attempts: Vec<FailedAttempt> = Vec::new();

        for iteration in 1..=stage.max_iterations {
            debug!(
                "[{}] Iteration {}/{}",
                stage.id, iteration, stage.max_iterations
            );

            // GENERATE
            let solution = solver(&stage.goal, &stage.available_tools, &attempts).await?;
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

            let verification = verifier(&solution, &stage.acceptance_criteria).await?;
            let passed = verification.is_passed();

            self.event_bus
                .publish_workflow(WorkflowEvent::VerificationCompleted {
                    stage_id: stage.id.clone(),
                    passed,
                });

            if passed {
                info!("[{}] Stage completed in {} iterations", stage.id, iteration);
                self.event_bus
                    .publish_workflow(WorkflowEvent::StageCompleted {
                        stage_id: stage.id.clone(),
                        iterations: iteration,
                    });
                return Ok(StageResult::Success {
                    iterations: iteration,
                    solution_summary: solution.reasoning,
                });
            }

            // REFINE
            let hints = match &verification {
                VerificationResult::Failed { hints, .. } => hints.clone(),
                _ => String::new(),
            };

            warn!("[{}] Verification failed, refining: {}", stage.id, hints);
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

        info!("[{}] Max iterations reached", stage.id);
        self.event_bus.publish_workflow(WorkflowEvent::StageFailed {
            stage_id: stage.id.clone(),
            attempts: attempts.len(),
        });

        let last_error = attempts
            .last()
            .map(|a| a.hints.clone())
            .unwrap_or_else(|| "Unknown error".to_string());

        Ok(StageResult::Failed {
            attempts: attempts.len(),
            last_error,
        })
    }
}
