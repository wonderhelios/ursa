use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use ursa_treesitter::symbol_index::SymbolIndex;
use ursa_treesitter::scope_graph::Definition;

use crate::{Tool, ToolDefinition};

pub struct SymbolSearchTool {
    index: Arc<SymbolIndex>,
}

impl SymbolSearchTool {
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self { index }
    }

    fn format_result(&self, matches: &[Definition]) -> String {
        if matches.is_empty() {
            return "No symbols found.".to_string();
        }

        let mut output = format!("Found {} symbols:\n\n", matches.len());

        for (i, def) in matches.iter().enumerate() {
            output.push_str(&format!(
                "{}. {} `{}` at {}:{}\n",
                i + 1,
                def.kind,
                def.name,
                def.file.display(),
                def.line + 1
            ));

            if let Ok(content) = std::fs::read_to_string(&def.file) {
                let lines: Vec<&str> = content.lines().collect();
                let start = def.line;
                let end = (def.line + 3).min(lines.len());

                if start < lines.len() {
                    output.push_str("   ```\n");
                    for (idx, line) in lines[start..end].iter().enumerate() {
                        let marker = if idx == 0 { ">>> " } else { "    " };
                        output.push_str(&format!("{}{}\n", marker, line));
                    }
                    output.push_str("   ```\n");
                }
            }

            output.push('\n');
        }

        output
    }
}

#[async_trait]
impl Tool for SymbolSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "symbol_search".to_string(),
            description: "Search for code definitions (functions, structs, traits, etc). \
                Returns locations WITH code snippets. \
                Use this INSTEAD of bash/grep when looking for code entities."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Symbol name to search for"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        let matches: Vec<_> = self
            .index
            .search(query)
            .into_iter()
            .take(5)
            .cloned()
            .collect();

        Ok(self.format_result(&matches))
    }
}
