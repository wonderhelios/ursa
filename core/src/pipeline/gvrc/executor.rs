//! Action executor - executes planned tool calls.

use crate::pipeline::gvrc::types::PlannedAction;
use anyhow::Result;
use tracing::{debug, error, info, warn};
use ursa_tools::ToolRegistry;

/// Executor runs planned actions using the tool registry.
pub struct ActionExecutor<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> ActionExecutor<'a> {
    /// Create a new executor with the given tool registry.
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    /// Execute a single planned action.
    pub async fn execute_action(&self, action: &PlannedAction) -> Result<String> {
        info!("Executing action: {} - {}", action.tool, action.purpose);

        let tool = match self.registry.get(&action.tool) {
            Some(t) => t,
            None => {
                let err = format!("Tool '{}' not found", action.tool);
                warn!("{}", err);
                return Err(anyhow::anyhow!(err));
            }
        };

        debug!("Tool args: {}", action.args);

        match tool.execute(action.args.clone()).await {
            Ok(result) => {
                let display = if result.len() > 5000 {
                    format!(
                        "{}...\n[truncated {} chars]",
                        &result[..5000],
                        result.len() - 5000
                    )
                } else {
                    result
                };
                info!("Action completed successfully");
                Ok(display)
            }
            Err(e) => {
                error!("Action failed: {}", e);
                Err(anyhow::anyhow!("Tool execution failed: {}", e))
            }
        }
    }

    /// Execute multiple actions sequentially.
    pub async fn execute_actions(&self, actions: &[PlannedAction]) -> Vec<Result<String>> {
        let mut results = Vec::with_capacity(actions.len());

        for action in actions {
            let result = self.execute_action(action).await;
            results.push(result);
        }

        results
    }

    /// Execute actions with early termination on failure.
    pub async fn execute_actions_strict(&self, actions: &[PlannedAction]) -> Result<Vec<String>> {
        let mut results = Vec::with_capacity(actions.len());

        for action in actions {
            match self.execute_action(action).await {
                Ok(result) => results.push(result),
                Err(e) => return Err(e),
            }
        }

        Ok(results)
    }
}
