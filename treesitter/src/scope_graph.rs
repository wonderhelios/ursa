//! Definition extractor — runs `.scm` queries against a syntax tree.
//!
//! Capture naming convention: `@definition.<kind>`
//! E.g. `@definition.function`, `@definition.struct`

use std::path::{Path, PathBuf};

use tree_sitter::QueryCursor;

use crate::language::TSLanguageConfig;

// ===== Definition =====

/// A named definition found in source code.
#[derive(Debug, Clone)]
pub struct Definition {
    pub name: String,
    pub kind: String, // "function", "struct", "enum", etc.
    pub file: PathBuf,
    /// 0-based line number
    pub line: usize,
}

impl Definition {
    /// Human-readable one-liner: `fn foo (src/main.rs:42)`
    pub fn display(&self) -> String {
        format!(
            "{} {} ({}:{})",
            self.kind,
            self.name,
            self.file.display(),
            self.line + 1,
        )
    }
}

// ===== Extraction =====

/// Extract top-level definitions from source using the language's `.scm` query.
///
/// Captures must follow the `@definition.<kind>` naming convention.
pub fn extract_definitions(
    source: &str,
    config: &'static TSLanguageConfig,
    file: &Path,
) -> anyhow::Result<Vec<Definition>> {
    // Parse the source
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&(config.grammar)())
        .map_err(|e| anyhow::anyhow!("Failed to load grammar for {:?}: {}", file, e))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Parsing failed for {:?}", file))?;

    // Run the definitions query
    let query = config.definitions_query.get(config.grammar)?;
    let mut cursor = QueryCursor::new();
    let source_bytes = source.as_bytes();

    let mut definitions = Vec::new();

    for match_ in cursor.matches(query, tree.root_node(), source_bytes) {
        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];

            // Only process @definition.* captures
            let Some(kind) = capture_name.strip_prefix("definition.") else {
                continue;
            };

            let Ok(name) = capture.node.utf8_text(source_bytes) else {
                continue;
            };

            definitions.push(Definition {
                name: name.to_string(),
                kind: kind.to_string(),
                file: file.to_path_buf(),
                line: capture.node.start_position().row,
            });
        }
    }

    Ok(definitions)
}
