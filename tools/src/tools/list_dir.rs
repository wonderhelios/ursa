// list_dir tool

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::{Tool, ToolDefinition};

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_dir".to_string(),
            description: "List the contents of a directory".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list (defaults to '.')"
                    }
                },
                "required": []
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"].as_str().unwrap_or(".");

        let mut read_dir = fs::read_dir(path)
            .await
            .map_err(|e| anyhow!("Failed to list '{}': {}", path, e))?;

        let mut entries: Vec<String> = vec![];

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();

            if metadata.is_dir() {
                entries.push(format!("{}/", name));
            } else {
                entries.push(format!("{} ({} bytes)", name, metadata.len()));
            }
        }

        entries.sort();

        if entries.is_empty() {
            Ok(format!("'{}' is empty", path))
        } else {
            Ok(format!("{}:\n{}", path, entries.join("\n")))
        }
    }
}
