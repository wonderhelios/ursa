//! Manual checkpoints for human-in-the-loop workflow.

use crate::pipeline::gvrc::types::Solution;
use anyhow::Result;
use std::io::{self, Write};
use tracing::{info, warn};

/// Checkpoint type for human confirmation.
pub enum Checkpoint {
    /// Confirm before executing a stage.
    BeforeStage { stage_id: String, goal: String },

    /// Confirm before executing specific actions.
    BeforeActions {
        stage_id: String,
        actions_summary: Vec<String>,
    },

    /// Review and confirm the solution before commit.
    BeforeCommit {
        stage_id: String,
        solution: Solution,
    },
}

/// Human response at a checkpoint.
pub enum CheckpointResponse {
    /// Proceed with execution.
    Proceed,
    /// Modify the plan/solution.
    Modify(String),
    /// Skip this stage/action.
    Skip,
    /// Abort the entire workflow.
    Abort,
}

/// Checkpoint handler for human-in-the-loop.
pub struct CheckpointHandler;

impl CheckpointHandler {
    /// Create a new checkpoint handler.
    pub fn new() -> Self {
        Self
    }

    /// Trigger a checkpoint and wait for human response.
    pub fn trigger(&self, checkpoint: Checkpoint) -> Result<CheckpointResponse> {
        match checkpoint {
            Checkpoint::BeforeStage { stage_id, goal } => {
                self.prompt_before_stage(&stage_id, &goal)
            }
            Checkpoint::BeforeActions {
                stage_id,
                actions_summary,
            } => self.prompt_before_actions(&stage_id, &actions_summary),
            Checkpoint::BeforeCommit { stage_id, solution } => {
                self.prompt_before_commit(&stage_id, &solution)
            }
        }
    }

    fn prompt_before_stage(&self, stage_id: &str, goal: &str) -> Result<CheckpointResponse> {
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  CHECKPOINT: Before Stage Execution                          ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Stage: {:<52} ║", stage_id);
        println!(
            "║  Goal:  {:<52} ║",
            goal.chars().take(52).collect::<String>()
        );
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  [P]roceed  [M]odify  [S]kip  [A]bort                        ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        print!("> ");
        io::stdout().flush()?;

        self.read_response()
    }

    fn prompt_before_actions(
        &self,
        stage_id: &str,
        actions: &[String],
    ) -> Result<CheckpointResponse> {
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  CHECKPOINT: Before Action Execution                         ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Stage: {:<52} ║", stage_id);
        println!("║                                                              ║");
        println!("║  Planned actions:                                            ║");

        for (i, action) in actions.iter().take(5).enumerate() {
            let truncated = action.chars().take(56).collect::<String>();
            println!("║    {}. {:<54} ║", i + 1, truncated);
        }

        if actions.len() > 5 {
            println!(
                "║    ... and {} more actions                    ║",
                actions.len() - 5
            );
        }

        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  [P]roceed  [M]odify  [S]kip  [A]bort                        ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        print!("> ");
        io::stdout().flush()?;

        self.read_response()
    }

    fn prompt_before_commit(
        &self,
        stage_id: &str,
        solution: &Solution,
    ) -> Result<CheckpointResponse> {
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  CHECKPOINT: Before Commit                                   ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Stage: {:<52} ║", stage_id);
        println!("║                                                              ║");
        println!("║  Reasoning:                                                  ║");
        let reasoning = solution.reasoning.chars().take(200).collect::<String>();
        for line in reasoning.lines() {
            println!("║  {:<60} ║", line.chars().take(60).collect::<String>());
        }
        println!("║                                                              ║");
        println!(
            "║  Actions: {}                                                   ║",
            solution.planned_actions.len()
        );
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  [P]roceed  [M]odify  [S]kip  [A]bort                        ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        print!("> ");
        io::stdout().flush()?;

        self.read_response()
    }

    fn read_response(&self) -> Result<CheckpointResponse> {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "p" | "proceed" | "" => Ok(CheckpointResponse::Proceed),
            "m" | "modify" => {
                println!("Enter modification instructions:");
                let mut modify = String::new();
                io::stdin().read_line(&mut modify)?;
                Ok(CheckpointResponse::Modify(modify.trim().to_string()))
            }
            "s" | "skip" => Ok(CheckpointResponse::Skip),
            "a" | "abort" => Ok(CheckpointResponse::Abort),
            _ => {
                warn!("Unknown response, defaulting to Proceed");
                Ok(CheckpointResponse::Proceed)
            }
        }
    }
}

impl Default for CheckpointHandler {
    fn default() -> Self {
        Self::new()
    }
}
