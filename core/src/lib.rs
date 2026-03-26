pub mod context;
pub mod eval;
pub mod pipeline;
pub mod runtime;
pub mod telemetry;

// Re-export SpawnAgentTool so cli can register it
pub use pipeline::subagent::SpawnAgentTool;
