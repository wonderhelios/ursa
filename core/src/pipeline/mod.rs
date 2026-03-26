//! Pipeline - execution flow for agent tasks.

pub mod engine;
pub mod gvrc;
pub mod prompts;
pub mod subagent;

pub use engine::PipelineEngine;
pub use subagent::SpawnAgentTool;
