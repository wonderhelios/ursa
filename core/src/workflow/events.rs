//! Workflow events

use super::definition::StageId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowEvent {
    /// Workflow started
    Started {
        mode: super::definition::ExecutionMode,
    },
    /// Stage started
    StageStarted {
        stage_id: StageId,
        iteration: usize,
    },
    /// Verification completed
    VerificationCompleted {
        stage_id: StageId,
        passed: bool,
    },
    /// Stage completed
    StageCompleted {
        stage_id: StageId,
        iterations: usize,
    },
    /// Stage failed (max iterations)
    StageFailed {
        stage_id: StageId,
        attempts: usize,
    },
    /// Workflow completed
    Completed {
        success: bool,
    },
}

impl WorkflowEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::StageCompleted { .. } | Self::StageFailed { .. } | Self::Completed { .. }
        )
    }
}
