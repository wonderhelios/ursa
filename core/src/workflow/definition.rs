//! Core type definitions for the GVRC workflow system.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ===== Identifiers =====

/// Unique identifier for a stage within a workflow
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StageId(pub String);

impl StageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for StageId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ===== Execution Mode =====

/// Determines the execution strategy for the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Fast mode: direct act, no GVRC
    Fast,
    /// Standard mode: planner + stageExecutor (simple tasks skip plan)
    #[default]
    Standard,
    /// Strict mode: complete TPAR + GVRC (complex tasks)
    Strict,
}

// ===== Stage Definition =====

/// A single stage in the workflow with explicit goals and acceptance criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    /// Unique stage identifier
    pub id: StageId,
    /// Stage goal description (for the solver)
    pub goal: String,
    /// Acceptance criteria (for the verifier)
    pub acceptance_criteria: Vec<Criterion>,
    /// Available tools (limits Solver's tool selection)
    pub available_tools: Vec<String>,
    /// Maximum iterations for the GVRC loop (default 10)
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    10
}

impl Stage {
    /// Create a new stage with the given id and goal
    pub fn new(id: impl Into<StageId>, goal: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            goal: goal.into(),
            acceptance_criteria: Vec::new(),
            available_tools: Vec::new(),
            max_iterations: default_max_iterations(),
        }
    }

    /// Add an acceptance criterion to this stage
    pub fn with_criterion(mut self, criterion: Criterion) -> Self {
        self.acceptance_criteria.push(criterion);
        self
    }

    /// Add multiple available tools
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.available_tools
            .extend(tools.into_iter().map(Into::into));
        self
    }

    /// Set the maximum iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
}

// ===== Criterion =====

/// A single acceptance criterion for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    /// Unique identifier for this criterion
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// The type of check to perform
    #[serde(flatten)]
    pub check: CheckType,
}

impl Criterion {
    pub fn new(id: impl Into<String>, description: impl Into<String>, check: CheckType) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            check,
        }
    }
}

// ===== Check Types =====

/// The type of verification check to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckType {
    /// Automated check (compilation, tests, etc.)
    Automated {
        /// Command to execute
        command: String,
    },
    /// LLM evaluation
    Llm {
        /// Evaluation prompt
        prompt: String,
    },
}

/// Workflow plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub stages: Vec<Stage>,
}

impl Plan {
    pub fn new(stages: Vec<Stage>) -> Self {
        Self { stages }
    }
}

/// Solver solution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub reasoning: String,
    pub planned_actions: Vec<PlannedAction>,
}

impl Solution {
    pub fn new(reasoning: impl Into<String>) -> Self {
        Self {
            reasoning: reasoning.into(),
            planned_actions: Vec::new(),
        }
    }
}

/// Planned action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedAction {
    pub tool: String,
    pub args: Value,
    pub purpose: String,
}

impl PlannedAction {
    pub fn new(tool: impl Into<String>, args: Value, purpose: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args,
            purpose: purpose.into(),
        }
    }
}

/// Verification result
#[derive(Debug, Clone)]
pub enum VerificationResult {
    Passed,
    Failed {
        failures: Vec<CriterionFailure>,
        hints: String,
    },
}

impl VerificationResult {
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Criterion failure
#[derive(Debug, Clone)]
pub struct CriterionFailure {
    pub criterion_id: String,
    pub reason: String,
}

/// Stage execution result
#[derive(Debug, Clone)]
pub enum StageResult {
    Success {
        solution: Solution,
        iterations: usize,
    },
    Failed {
        attempts: usize,
    },
}

impl StageResult {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

/// Failed attempt record
#[derive(Debug, Clone)]
pub struct FailedAttempt {
    pub iteration: usize,
    pub solution: Solution,
    pub failures: Vec<CriterionFailure>,
    pub hints: String,
}

/// Collection of stage results
#[derive(Debug, Clone, Default)]
pub struct StageResults {
    results: HashMap<StageId, StageResult>,
}

impl StageResults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, stage_id: StageId, result: StageResult) {
        self.results.insert(stage_id, result);
    }

    pub fn get(&self, stage_id: &StageId) -> Option<&StageResult> {
        self.results.get(stage_id)
    }
}
