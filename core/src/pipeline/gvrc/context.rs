//! GVRC context builder - integrates symbol index and project knowledge.

use crate::context::engine::ContextEngine;
use crate::pipeline::gvrc::types::{ExecutionMode, Stage};
use std::sync::Arc;
use ursa_treesitter::symbol_index::SharedSymbolIndex;

/// Context builder for GVRC stages.
pub struct GvrcContextBuilder {
    symbol_index: Option<SharedSymbolIndex>,
    context_engine: Option<Arc<ContextEngine>>,
}

impl GvrcContextBuilder {
    /// Create a new context builder.
    pub fn new() -> Self {
        Self {
            symbol_index: None,
            context_engine: None,
        }
    }

    /// Set symbol index.
    pub fn with_symbol_index(mut self, index: SharedSymbolIndex) -> Self {
        self.symbol_index = Some(index);
        self
    }

    /// Set context engine.
    pub fn with_context_engine(mut self, engine: Arc<ContextEngine>) -> Self {
        self.context_engine = Some(engine);
        self
    }

    /// Build context for a specific stage.
    pub fn build_for_stage(&self, stage: &Stage, mode: ExecutionMode) -> String {
        let mut context = String::new();

        // Execution mode context
        context.push_str(&format!("## Execution Mode: {:?}\n\n", mode));

        // Stage information
        context.push_str(&format!("## Current Stage: {}\n", stage.id));
        context.push_str(&format!("Goal: {}\n\n", stage.goal));

        // Available tools
        if !stage.available_tools.is_empty() {
            context.push_str("## Available Tools\n");
            for tool in &stage.available_tools {
                context.push_str(&format!("- {}\n", tool));
            }
            context.push('\n');
        }

        // Symbol index integration
        if let Some(ref _index) = self.symbol_index {
            let relevant_symbols = self.find_relevant_symbols(stage);
            if !relevant_symbols.is_empty() {
                context.push_str("## Relevant Code Symbols\n");
                for sym in relevant_symbols.iter().take(15) {
                    context.push_str(&format!(
                        "- {} `{}` ({}:{})\n",
                        sym.kind,
                        sym.name,
                        sym.file.file_name().unwrap_or_default().to_string_lossy(),
                        sym.line + 1
                    ));
                }
                context.push('\n');
                context.push_str(
                    "Tip: Use symbol_search to find definitions and understand the codebase structure.\n\n",
                );
            }
        }

        // Project context from ContextEngine
        if let Some(ref ctx_engine) = self.context_engine {
            let project_ctx = ctx_engine.build_context();
            if !project_ctx.is_empty() {
                context.push_str(&project_ctx);
                context.push('\n');
            }
        }

        context
    }

    /// Find symbols relevant to the stage goal.
    fn find_relevant_symbols(&self, stage: &Stage) -> Vec<ursa_treesitter::scope_graph::Definition> {
        let index = match self.symbol_index {
            Some(ref idx) => match idx.read() {
                Ok(guard) => guard,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        };

        // Extract keywords from stage goal
        let keywords: Vec<&str> = stage
            .goal
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        let mut relevant = Vec::new();
        let all_defs = index.all_definitions();

        for def in all_defs {
            // Check if definition name matches any keyword
            let name_lower = def.name.to_lowercase();
            let goal_lower = stage.goal.to_lowercase();

            if name_lower
                .split('_')
                .any(|part| goal_lower.contains(part))
                || keywords
                    .iter()
                    .any(|k| name_lower.contains(&k.to_lowercase()))
            {
                relevant.push(def.clone());
            }
        }

        // Sort by relevance (exact matches first)
        relevant.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == stage.goal.to_lowercase();
            let b_exact = b.name.to_lowercase() == stage.goal.to_lowercase();
            b_exact.cmp(&a_exact)
        });

        relevant
    }
}

impl Default for GvrcContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
