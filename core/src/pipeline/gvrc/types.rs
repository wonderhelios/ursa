//! Core type definitions for GVRC execution mode.

use serde::{Deserialize, Serialize};

// execution mode for the pipline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Fast mode: direct tool execution without GVRC.
    Fast,
    /// Standard mode: single-stage GVRC with planning.
    #[default]
    Standard,
    /// Strict mode: multi-stage GVRC with full verification.
    Strict,
}

/// A stage in the GVRC workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    /// Unique stage identifier.
    pub id: String,

    /// Stage goal description (for the Solver).
    pub goal: String,

    /// Acceptance criteria (for the Verifier).
    pub acceptance_criteria: Vec<Criterion>,

    /// Available tools (limits Solver's tool selection).
    #[serde(default)]
    pub available_tools: Vec<String>,

    /// Maximum iterations for the GVRC loop.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    10
}

impl Stage {
    // Create a new stage with the given id and goal.
    pub fn new(id: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            goal: goal.into(),
            acceptance_criteria: Vec::new(),
            available_tools: Vec::new(),
            max_iterations: default_max_iterations(),
        }
    }

    /// Add an acceptance criterion.
    pub fn with_criterion(mut self, criterion: Criterion) -> Self {
        self.acceptance_criteria.push(criterion);
        self
    }

    /// Add available tools.
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.available_tools
            .extend(tools.into_iter().map(Into::into));
        self
    }

    /// Set maximum iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
}

/// An acceptance criterion for verification
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
    /// Create a new criterion
    pub fn new(id: impl Into<String>, description: impl Into<String>, check: CheckType) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            check,
        }
    }
}

/// The type of verification check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckType {
    /// Automated check (compilation, tests, etc.)
    Automated {
        /// Command to execute
        command: String,
    },
    /// LLM-based evaluation
    Llm {
        /// Evaluation prompt
        prompt: String,
    },
}
/// A plan consisting of multiple stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Stages in execution order.
    pub stages: Vec<Stage>,

    /// Overall strategy description.
    #[serde(default)]
    pub overall_strategy: String,
}

impl Plan {
    /// Create a new plan with the given stages.
    pub fn new(stages: Vec<Stage>) -> Self {
        Self {
            stages,
            overall_strategy: String::new(),
        }
    }

    /// Add overall strategy description.
    pub fn with_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.overall_strategy = strategy.into();
        self
    }
}

/// Result of executing a stage.
#[derive(Debug, Clone)]
pub enum StageResult {
    /// Stage completed successfully.
    Success {
        /// Number of iterations used.
        iterations: usize,
        /// Final solution description.
        solution_summary: String,
    },
    /// Stage failed after max iterations.
    Failed {
        /// Total attempts made.
        attempts: usize,
        /// Last failure reason.
        last_error: String,
    },
}

impl StageResult {
    /// Check if the stage succeeded.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Get the number of iterations if successful.
    pub fn iterations(&self) -> Option<usize> {
        match self {
            Self::Success { iterations, .. } => Some(*iterations),
            Self::Failed { attempts, .. } => Some(*attempts),
        }
    }
}

/// Result of verifying a solution.
#[derive(Debug, Clone)]
pub enum VerificationResult {
    /// All criteria passed.
    Passed,
    /// Some criteria failed.
    Failed {
        /// Failure reasons.
        failures: Vec<String>,
        /// Improvement hints.
        hints: String,
    },
}

impl VerificationResult {
    /// Check if verification passed.
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// A planned action (tool call) generated by the Solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedAction {
    /// Tool name.
    pub tool: String,

    /// Tool arguments.
    pub args: serde_json::Value,

    /// Purpose of this tool call.
    pub purpose: String,
}

/// A solution generated by the Solver.
#[derive(Debug, Clone, Default)]
pub struct Solution {
    /// Reasoning process.
    pub reasoning: String,

    /// Planned tool calls.
    pub planned_actions: Vec<PlannedAction>,

    /// Expected outcome.
    pub expected_outcome: String,
}

/// Record of a failed attempt for refinement.
#[derive(Debug, Clone)]
pub struct FailedAttempt {
    /// Iteration number.
    pub iteration: usize,

    /// The solution that was attempted.
    pub solution: Solution,

    /// Verification failures.
    pub failures: Vec<String>,

    /// Verifier-generated improvement hints.
    pub hints: String,
}
