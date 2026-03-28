//! GVRC (Generate-Verify-Refine-Commit) execution mode for PipelineEngine.
//!
//! This module implements the IMO 2025 Gold verification-and-refinement pipeline
//! as an extension to the existing PipelineEngine.

pub mod a2a;
pub mod checkpoint;
pub mod context;
pub mod events;
pub mod executor;
pub mod learning;
pub mod planner;
pub mod reviewer;
pub mod runner;
pub mod solver;
pub mod types;
pub mod verifier;

pub use a2a::{A2aServer, AgentCard, A2aTaskRequest, A2aTaskResponse};
pub use checkpoint::{Checkpoint, CheckpointHandler, CheckpointResponse};
pub use context::GvrcContextBuilder;
pub use events::WorkflowEvent;
pub use executor::ActionExecutor;
pub use learning::{ExecutionPattern, Learner};
pub use planner::Planner;
pub use reviewer::{Decision, Review, Reviewer};
pub use runner::GvrcRunner;
pub use solver::Solver;
pub use types::{
    CheckType, Criterion, ExecutionMode, FailedAttempt, Plan, PlannedAction, Solution, Stage,
    StageResult, VerificationResult,
};
pub use verifier::{ExecutionResult, Verifier};
