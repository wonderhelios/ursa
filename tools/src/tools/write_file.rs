// write_file 工具

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::{Tool, ToolDefinition};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file, creating parent directories if needed"
                .to_string(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "path":{
                        "type":"string",
                        "description": "The path to the file to write"
                    },
                    "content":{
                        "type":"string",
                        "description":"The content to write"
                    }
                },
                "required":["path","content"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'path' argument"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'content' argument"))?;

        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        fs::write(path, content)
            .await
            .map_err(|e| anyhow!("Failed to write '{}': {}", path, e))?;

        Ok(format!("Wrote {} bytes to '{}'", content.len(), path))
    }
}
