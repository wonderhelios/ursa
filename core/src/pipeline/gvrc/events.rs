//! Workflow events for GVRC execution.

use crate::pipeline::gvrc::types::ExecutionMode;
use serde::{Deserialize, Serialize};

/// Events emitted during GVRC workflow execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowEvent {
    /// Workflow started.
    Started { mode: ExecutionMode },

    /// Planning started.
    PlanningStarted { goal: String },

    /// Planning completed.
    PlanningCompleted { stage_count: usize },

    /// Planning failed.
    PlanningFailed { error: String },

    /// Stage execution started.
    StageStarted { stage_id: String, iteration: usize },

    /// Solver completed generating a solution.
    SolverCompleted {
        stage_id: String,
        iteration: usize,
        action_count: usize,
    },

    /// Verification started.
    VerificationStarted {
        stage_id: String,
        criterion_count: usize,
    },

    /// Verification completed.
    VerificationCompleted { stage_id: String, passed: bool },

    /// Refinement hints generated.
    RefinementHints { stage_id: String, hints: String },

    /// Stage completed successfully.
    StageCompleted { stage_id: String, iterations: usize },

    /// Stage failed (max iterations reached).
    StageFailed { stage_id: String, attempts: usize },

    /// Workflow completed.
    Completed { success: bool },

    /// Error occurred.
    Error {
        stage_id: Option<String>,
        error: String,
    },
}

impl WorkflowEvent {
    /// Get the stage ID if this event is stage-specific.
    pub fn stage_id(&self) -> Option<&str> {
        match self {
            Self::StageStarted { stage_id, .. } => Some(stage_id),
            Self::SolverCompleted { stage_id, .. } => Some(stage_id),
            Self::VerificationStarted { stage_id, .. } => Some(stage_id),
            Self::VerificationCompleted { stage_id, .. } => Some(stage_id),
            Self::RefinementHints { stage_id, .. } => Some(stage_id),
            Self::StageCompleted { stage_id, .. } => Some(stage_id),
            Self::StageFailed { stage_id, .. } => Some(stage_id),
            Self::Error { stage_id, .. } => stage_id.as_deref(),
            _ => None,
        }
    }

    /// Check if this is a terminal event.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::StageCompleted { .. } | Self::StageFailed { .. } | Self::Completed { .. }
        )
    }
}
