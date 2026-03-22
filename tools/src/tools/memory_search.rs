use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;

use ursa_services::memory::store::MemoryStore;

use crate::{Tool, ToolDefinition};

pub struct MemorySearchTool {
    store: Arc<Mutex<MemoryStore>>,
}

impl MemorySearchTool {
    pub fn new(store: Arc<Mutex<MemoryStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search persistent memory for relevant past information. \
                Use before answering questions about user preferences, \
                project decisions, or anything you might have noted before."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for in memory"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results to return (default 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'query' argument"))?;

        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        let store = self.store.lock().unwrap();
        let results = store.search(query, limit);

        if results.is_empty() {
            return Ok(format!("No memories found for '{}'", query));
        }

        let lines: Vec<String> = results
            .iter()
            .map(|e| {
                let tags = if e.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", e.tags.join(", "))
                };
                format!("- {}{}", e.content, tags)
            })
            .collect();

        Ok(format!("Found {} memories:\n{}", results.len(), lines.join("\n")))
    }
}
