pub mod registry;
pub mod tool;
pub mod tools;

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::execute;

/// Tool definition
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, args: Value) -> anyhow::Result<String>;
}

/// Re-exports for convenience
pub use registry::ToolRegistry;
pub use tools::bash::BashTool;
pub use tools::list_dir::ListDirTool;
pub use tools::memory_search::MemorySearchTool;
pub use tools::memory_write::MemoryWriteTool;
pub use tools::notify::NotifyTool;
pub use tools::read_file::ReadFileTool;
pub use tools::symbol_search::SymbolSearchTool;
pub use tools::todo_write::TodoItem;
pub use tools::todo_write::TodoManager;
pub use tools::todo_write::TodoWriteTool;
pub use tools::write_file::WriteFileTool;
