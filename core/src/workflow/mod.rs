//! GVRC (Generate-Verify-Refine-Commit) workflow orchestration.
//!
//! This module implements the architecture described in the GVRC design document,
//! providing structured task execution with explicit verification and refinement loops.

pub mod context;
pub mod definition;
pub mod events;

pub use context::{StageContext, WorkflowContext};
pub use definition::{
    CheckType, Criterion, CriterionFailure, ExecutionMode, FailedAttempt, Plan, PlannedAction,
    Solution, Stage, StageId, StageResult, StageResults, VerificationResult,
};
pub use events::WorkflowEvent;
