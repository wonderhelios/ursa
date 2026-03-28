//! Reviewer - analyzes execution history and extracts insights.

use crate::pipeline::prompts;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use ursa_llm::provider::{ChatRequest, LLMProvider, Message, Role};

/// Review analyzes completed workflow execution.
pub struct Reviewer {
    llm: Arc<dyn LLMProvider>,
}

impl Reviewer {
    /// Create a new Reviewer.
    pub fn new(llm: Arc<dyn LLMProvider>) -> Self {
        Self { llm }
    }

    /// Review a completed workflow execution.
    pub async fn review(
        &self,
        stage_ids: &[String],
        stage_results: &[(String, bool, usize)], // (stage_id, success, iterations)
    ) -> Result<Review> {
        info!("Reviewing workflow with {} stages", stage_ids.len());

        let summary = self.build_execution_summary(stage_results);
        let prompt = prompts::REVIEWER.replace("{execution_summary}", &summary);

        let request = ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.3),
            max_tokens: Some(2048),
            tools: None,
            tool_choice: None,
            stream: None,
        };

        let response = self.llm.chat(request).await?;
        self.parse_review(&response.content)
    }

    /// Build execution summary for the reviewer.
    fn build_execution_summary(
        &self,
        results: &[(String, bool, usize)],
    ) -> String {
        let mut summary = format!("Total stages: {}\n\n", results.len());

        for (i, (id, success, iterations)) in results.iter().enumerate() {
            summary.push_str(&format!(
                "Stage {}: {}\n- Success: {}\n- Iterations: {}\n\n",
                i + 1,
                id,
                success,
                iterations
            ));
        }

        let success_count = results.iter().filter(|(_, s, _)| *s).count();
        summary.push_str(&format!(
            "Overall: {}/{} stages successful",
            success_count,
            results.len()
        ));

        summary
    }

    /// Parse LLM response into Review.
    fn parse_review(&self, content: &str) -> Result<Review> {
        let json_str = extract_json(content);

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse review: {}\nRaw: {}", e, content))?;

        let summary = parsed
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("No summary")
            .to_string();

        let mut key_decisions = Vec::new();
        if let Some(decisions) = parsed.get("key_decisions").and_then(|v| v.as_array()) {
            for d in decisions {
                key_decisions.push(Decision {
                    decision: d.get("decision").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    outcome: d.get("outcome").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    lesson: d.get("lesson").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
        }

        let mut patterns = Vec::new();
        if let Some(pats) = parsed.get("patterns_learned").and_then(|v| v.as_array()) {
            for p in pats {
                if let Some(s) = p.as_str() {
                    patterns.push(s.to_string());
                }
            }
        }

        let mut memory_updates = Vec::new();
        if let Some(updates) = parsed.get("memory_updates").and_then(|v| v.as_array()) {
            for u in updates {
                memory_updates.push((
                    u.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    u.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                ));
            }
        }

        info!(
            "Review complete: {} decisions, {} patterns, {} memory updates",
            key_decisions.len(),
            patterns.len(),
            memory_updates.len()
        );

        Ok(Review {
            summary,
            key_decisions,
            patterns_learned: patterns,
            memory_updates,
        })
    }
}

/// Review result.
pub struct Review {
    /// Overall assessment.
    pub summary: String,

    /// Key decisions made during execution.
    pub key_decisions: Vec<Decision>,

    /// Patterns that led to success.
    pub patterns_learned: Vec<String>,

    /// Suggested memory updates.
    pub memory_updates: Vec<(String, String)>, // (key, value)
}

/// A key decision and its outcome.
pub struct Decision {
    pub decision: String,
    pub outcome: String,
    pub lesson: String,
}

fn extract_json(content: &str) -> &str {
    if let Some(start) = content.find("```json") {
        let after_start = &content[start + 7..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim();
        }
    }
    if let Some(start) = content.find("```") {
        let after_start = &content[start + 3..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim();
        }
    }
    content.trim()
}
