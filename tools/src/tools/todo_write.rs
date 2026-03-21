// todo_write tool (s03)

use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{Tool, ToolDefinition};

/// ===== Data types =====
#[derive(Debug, Clone, PartialEq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub updated_at: DateTime<Utc>,
}

/// ===== TodoManager =====
pub struct TodoManager {
    items: Vec<TodoItem>,
}

impl TodoManager {
    pub fn new() -> Self {
        Self { items: vec![] }
    }

    pub fn update(&mut self, items: Vec<TodoItem>) -> anyhow::Result<()> {
        let in_progress = items
            .iter()
            .filter(|it| it.status == TodoStatus::InProgress)
            .count();
        if in_progress > 1 {
            return Err(anyhow!("Only one todo can be in_progress at a time"));
        }
        if items.len() > 20 {
            return Err(anyhow!("Max 20 todos allowed"));
        }
        self.items = items;

        Ok(())
    }

    pub fn items(&self) -> &[TodoItem] {
        &self.items
    }

    /// Render for display: [ ] pending [>] in_progress [x] completed
    pub fn render(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        self.items
            .iter()
            .map(|it| {
                let sym = match it.status {
                    TodoStatus::Pending => "[ ]",
                    TodoStatus::InProgress => "[>]",
                    TodoStatus::Completed => "[x]",
                };
                format!("{} {}", sym, it.content)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// True if an InProgress item hasn't been updated for >= 5 minutes
    pub fn need_nag(&self) -> bool {
        self.items
            .iter()
            .filter(|it| it.status == TodoStatus::InProgress)
            .any(|it| (Utc::now() - it.updated_at).num_minutes() >= 5)
    }
}

impl Default for TodoManager {
    fn default() -> Self {
        Self::new()
    }
}

/// ===== TodoWriteTool =====
pub struct TodoWriteTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl TodoWriteTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "todo_write".to_string(),
            description: "Update the todo list to track tasks and progress. \
                Each call REPLACES the entire list. \
                Rules: only one item can be 'in_progress' at a time, max 20 items. \
                Always mark the current task as 'in_progress' before starting it, \
                and 'completed' when done."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "description": "The complete replacement todo list",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Unique short ID, e.g. 'task_1'"
                                },
                                "content": {
                                    "type": "string",
                                    "description": "Task description"
                                },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"],
                                    "description": "Current status"
                                }
                            },
                            "required": ["id", "content", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let todos_json = args["todos"]
            .as_array()
            .ok_or_else(|| anyhow!("Missing 'todos' array"))?;

        let items: Vec<TodoItem> = todos_json
            .iter()
            .filter_map(|it| {
                let id = it["id"].as_str()?.to_string();
                let content = it["content"].as_str()?.to_string();
                let status = match it["status"].as_str()? {
                    "in_progress" => TodoStatus::InProgress,
                    "completed" => TodoStatus::Completed,
                    _ => TodoStatus::Pending,
                };

                Some(TodoItem {
                    id,
                    content,
                    status,
                    updated_at: Utc::now(),
                })
            })
            .collect();

        let mut mgr = self.manager.lock().unwrap();
        mgr.update(items)?;

        let rendered = mgr.render();
        println!("\n{}\n", rendered);

        Ok(format!("Todo list updated:\n{}", rendered))
    }
}