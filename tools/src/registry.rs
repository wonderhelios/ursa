// 工具注册表

use crate::Tool;
use std::collections::HashMap;

/// Tool registry - maps tool names to implementations
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.definition().name.clone();
        self.tools.insert(name, Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn all(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    /// Build with all standard tools
    pub fn with_defaults() -> Self {
        use crate::BashTool;
        use crate::tools::list_dir::ListDirTool;
        use crate::tools::read_file::ReadFileTool;
        use crate::tools::write_file::WriteFileTool;

        let mut registry = Self::new();
        registry.register(BashTool);
        registry.register(ReadFileTool);
        registry.register(WriteFileTool);
        registry.register(ListDirTool);

        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}