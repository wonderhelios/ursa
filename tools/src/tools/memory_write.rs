use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;

use ursa_services::memory::store::MemoryStore;

use crate::{Tool, ToolDefinition};

pub struct MemoryWriteTool {
    store: Arc<Mutex<MemoryStore>>,
}

impl MemoryWriteTool {
    pub fn new(store: Arc<Mutex<MemoryStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_write".to_string(),
            description: "Save an important fact or piece of information to persistent memory. \
                Use this to remember things across conversations: \
                user preferences, project facts, decisions made, key findings."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The fact or information to remember. Be concise and specific."
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for categorization, e.g. [\"rust\", \"architecture\", \"user-pref\"]"
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'content' argument"))?;

        let tags = args["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let mut store = self.store.lock().unwrap();
        let id = store.write(content, tags)?;

        Ok(format!("Memory saved (id: {}): {}", &id[..8], content))
    }
}
