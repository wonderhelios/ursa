//! A2A (Agent-to-Agent) protocol support.
//! Allows Ursa to be called by other agents via a simple HTTP interface.

use serde::{Deserialize, Serialize};
use tracing::info;

/// A2A Agent card - describes this agent's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub skills: Vec<String>,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
}

impl AgentCard {
    /// Create Ursa's agent card.
    pub fn ursa() -> Self {
        Self {
            name: "Ursa".to_string(),
            description: "A Rust-focused AI coding assistant with GVRC verification pipeline"
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                "code_generation".to_string(),
                "code_review".to_string(),
                "refactoring".to_string(),
                "bug_fixing".to_string(),
                "symbol_search".to_string(),
            ],
            skills: vec![
                "rust".to_string(),
                "cargo".to_string(),
                "tree_sitter".to_string(),
            ],
            default_input_modes: vec!["text".to_string()],
            default_output_modes: vec!["text".to_string()],
        }
    }
}

/// A2A Task request from another agent.
#[derive(Debug, Clone, Deserialize)]
pub struct A2aTaskRequest {
    pub id: String,
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub task_type: String,
    pub content: String,
    pub mode: Option<String>, // "fast", "standard", "strict"
}

/// A2A Task response.
#[derive(Debug, Clone, Serialize)]
pub struct A2aTaskResponse {
    pub id: String,
    pub status: String, // "completed", "failed", "in_progress"
    pub result: Option<String>,
    pub error: Option<String>,
    pub artifacts: Vec<A2aArtifact>,
}

/// A2A Artifact (file or output produced).
#[derive(Debug, Clone, Serialize)]
pub struct A2aArtifact {
    pub name: String,
    pub content_type: String,
    pub content: String,
}

/// A2A server handler.
pub struct A2aServer {
    agent_card: AgentCard,
}

impl A2aServer {
    /// Create a new A2A server.
    pub fn new() -> Self {
        Self {
            agent_card: AgentCard::ursa(),
        }
    }

    /// Get the agent card.
    pub fn agent_card(&self) -> &AgentCard {
        &self.agent_card
    }

    /// Handle an incoming task request.
    pub fn handle_task_request(&self, request: A2aTaskRequest) -> A2aTaskResponse {
        info!(
            "A2A task received: {} (type: {})",
            request.id, request.task_type
        );

        // Convert A2A mode to ExecutionMode
        let _mode = match request.mode.as_deref() {
            Some("strict") => crate::pipeline::gvrc::ExecutionMode::Strict,
            Some("standard") => crate::pipeline::gvrc::ExecutionMode::Standard,
            _ => crate::pipeline::gvrc::ExecutionMode::Fast,
        };

        // Note: Actual execution would require access to PipelineEngine
        // This is a simplified implementation that would need to be integrated
        // with the main engine to actually execute tasks.

        A2aTaskResponse {
            id: request.id,
            status: "in_progress".to_string(),
            result: Some(format!(
                "Task '{}' received. Processing with mode: {:?}",
                request.task_type, request.mode
            )),
            error: None,
            artifacts: vec![],
        }
    }

    /// Discover tasks this agent can handle.
    pub fn discover_tasks(&self) -> Vec<A2aTaskType> {
        vec![
            A2aTaskType {
                name: "code_review".to_string(),
                description: "Review code for issues and improvements".to_string(),
                input_schema: "string (file path or code)".to_string(),
                output_schema: "string (review comments)".to_string(),
            },
            A2aTaskType {
                name: "fix_warnings".to_string(),
                description: "Fix compiler warnings in a Rust project".to_string(),
                input_schema: "string (project path)".to_string(),
                output_schema: "string (changes made)".to_string(),
            },
            A2aTaskType {
                name: "implement_feature".to_string(),
                description: "Implement a new feature with GVRC verification".to_string(),
                input_schema: "string (feature description)".to_string(),
                output_schema: "string (implementation summary)".to_string(),
            },
        ]
    }
}

impl Default for A2aServer {
    fn default() -> Self {
        Self::new()
    }
}

/// A2A Task type definition.
#[derive(Debug, Clone, Serialize)]
pub struct A2aTaskType {
    pub name: String,
    pub description: String,
    pub input_schema: String,
    pub output_schema: String,
}
