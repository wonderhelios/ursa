use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;

use ursa_services::delivery::queue::DeliveryQueue;

use crate::{Tool, ToolDefinition};

pub struct NotifyTool {
    queue: Arc<DeliveryQueue>,
}

impl NotifyTool {
    pub fn new(queue: Arc<DeliveryQueue>) -> Self {
        Self { queue }
    }
}

#[async_trait]
impl Tool for NotifyTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "notify".to_string(),
            description: "Send a notification to the user when a task is complete or needs attention. \
                Use this at the end of long-running tasks so the user knows when to look at results."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The notification message to send"
                    }
                },
                "required": ["message"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let message = args["message"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'message' argument"))?;

        let id = self.queue.enqueue("terminal", "user", message)?;
        Ok(format!("Notification queued (id: {})", &id[..8]))
    }
}
