// read_file tool

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::{Tool, ToolDefinition};

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file at the given path".to_string(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "path":{
                        "type":"string",
                        "description":"The path to the file to read"
                    }
                },
                "required":["path"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing 'path' argument"))?;

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| anyhow!("Failed to read '{}': {}", path, e))?;

        Ok(content)
    }
}
