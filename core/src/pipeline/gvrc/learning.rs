//! Learning mechanism - extracts patterns from successful executions.

use crate::pipeline::gvrc::types::Plan;
use std::collections::HashMap;
use tracing::{debug, info};

/// Pattern extracted from successful execution.
#[derive(Debug, Clone)]
pub struct ExecutionPattern {
    /// Task type/category.
    pub task_type: String,

    /// Successful approach description.
    pub approach: String,

    /// Tools that were effective.
    pub effective_tools: Vec<String>,

    /// Number of iterations typically needed.
    pub typical_iterations: usize,

    /// Success rate (0.0 - 1.0).
    pub success_rate: f32,

    /// How many times this pattern has been observed.
    pub observation_count: usize,
}

/// Learner extracts and manages execution patterns.
pub struct Learner {
    /// Stored patterns indexed by task type.
    patterns: HashMap<String, ExecutionPattern>,
}

impl Learner {
    /// Create a new learner.
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
        }
    }

    /// Learn from a completed execution.
    pub fn learn(
        &mut self,
        task_description: &str,
        plan: &Plan,
        stage_results: &[(String, bool, usize)], // (stage_id, success, iterations)
    ) -> Vec<String> {
        info!("Learning from execution of: {}", task_description);

        let task_type = self.classify_task(task_description);
        let new_patterns = self.extract_patterns(plan, stage_results);

        for pattern in new_patterns {
            self.update_pattern(&task_type, pattern);
        }

        self.patterns.keys().cloned().collect()
    }

    /// Get learned patterns for a task type.
    pub fn get_patterns(&self, task_type: &str) -> Vec<&ExecutionPattern> {
        self.patterns
            .values()
            .filter(|p| p.task_type == task_type)
            .collect()
    }

    /// Get guidance for a new task based on learned patterns.
    pub fn get_guidance(&self, task_description: &str) -> Option<String> {
        let task_type = self.classify_task(task_description);

        if let Some(pattern) = self.patterns.get(&task_type)
            && pattern.observation_count >= 2 && pattern.success_rate > 0.7 {
                return Some(format!(
                    "💡 Based on {} similar tasks:\n- Typical approach: {}\n- Effective tools: {}\n- Expected iterations: ~{}",
                    pattern.observation_count,
                    pattern.approach,
                    pattern.effective_tools.join(", "),
                    pattern.typical_iterations
                ));
            }

        None
    }

    /// Classify task into a type/category.
    fn classify_task(&self, description: &str) -> String {
        let desc_lower = description.to_lowercase();

        if desc_lower.contains("fix")
            || desc_lower.contains("bug")
            || desc_lower.contains("error")
        {
            "bug_fix".to_string()
        } else if desc_lower.contains("add")
            || desc_lower.contains("implement")
            || desc_lower.contains("create")
        {
            "feature_implementation".to_string()
        } else if desc_lower.contains("refactor")
            || desc_lower.contains("clean")
            || desc_lower.contains("improve")
        {
            "refactoring".to_string()
        } else if desc_lower.contains("test") || desc_lower.contains("verify") {
            "testing".to_string()
        } else if desc_lower.contains("doc") || desc_lower.contains("comment") {
            "documentation".to_string()
        } else {
            "general".to_string()
        }
    }

    /// Extract patterns from execution results.
    fn extract_patterns(
        &self,
        plan: &Plan,
        results: &[(String, bool, usize)],
    ) -> Vec<ExecutionPattern> {
        let mut patterns = Vec::new();

        // Analyze each stage
        for (i, (stage_id, success, iterations)) in results.iter().enumerate() {
            if let Some(stage) = plan.stages.get(i) {
                let stage_type = self.classify_stage(&stage.goal);

                patterns.push(ExecutionPattern {
                    task_type: stage_type,
                    approach: if *success {
                        format!(
                            "Successfully completed {} in {} iterations",
                            stage_id, iterations
                        )
                    } else {
                        format!("Failed after {} iterations", iterations)
                    },
                    effective_tools: stage.available_tools.clone(),
                    typical_iterations: *iterations,
                    success_rate: if *success { 1.0 } else { 0.0 },
                    observation_count: 1,
                });
            }
        }

        patterns
    }

    /// Classify a stage goal.
    fn classify_stage(&self, goal: &str) -> String {
        let goal_lower = goal.to_lowercase();

        if goal_lower.contains("analysis")
            || goal_lower.contains("understand")
            || goal_lower.contains("research")
        {
            "analysis".to_string()
        } else if goal_lower.contains("design") || goal_lower.contains("plan") {
            "design".to_string()
        } else if goal_lower.contains("implementation")
            || goal_lower.contains("code")
            || goal_lower.contains("write")
        {
            "implementation".to_string()
        } else if goal_lower.contains("verification")
            || goal_lower.contains("test")
            || goal_lower.contains("check")
        {
            "verification".to_string()
        } else {
            "general".to_string()
        }
    }

    /// Update an existing pattern or add a new one.
    fn update_pattern(&mut self, task_type: &str, new_pattern: ExecutionPattern) {
        let key = format!("{}:{}", task_type, new_pattern.approach);

        self.patterns
            .entry(key)
            .and_modify(|existing| {
                // Update with moving average
                let total = existing.observation_count as f32;
                let new_total = total + 1.0;

                existing.success_rate =
                    (existing.success_rate * total + new_pattern.success_rate) / new_total;
                existing.typical_iterations = ((existing.typical_iterations as f32 * total
                    + new_pattern.typical_iterations as f32)
                    / new_total) as usize;
                existing.observation_count += 1;

                debug!(
                    "Updated pattern for {}: success_rate={:.2}",
                    task_type, existing.success_rate
                );
            })
            .or_insert_with(|| {
                info!("New pattern learned for {}: {}", task_type, new_pattern.approach);
                new_pattern
            });
    }
}

impl Default for Learner {
    fn default() -> Self {
        Self::new()
    }
}
