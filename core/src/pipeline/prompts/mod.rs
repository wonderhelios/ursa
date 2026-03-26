//! Pipeline prompts loaded from markdown files.

pub static SYSTEM: &str = include_str!("system.md");
pub static PLANNER: &str = include_str!("planner.md");
pub static SOLVER: &str = include_str!("solver.md");
pub static VERIFIER: &str = include_str!("verifier.md");
pub static REVIEWER: &str = include_str!("reviewer.md");
