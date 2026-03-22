//! Language configuration and query memoization.

use std::sync::OnceLock;

use crate::languages::ALL_LANGUAGES;

/// Per-language tree-sitter configuration.
#[derive(Debug)]
pub struct TSLanguageConfig {
    /// E.g. `&["Rust"]`
    pub language_ids: &'static [&'static str],
    /// E.g. `&["rs"]`
    pub file_extensions: &'static [&'static str],
    /// Returns the tree-sitter grammar for this language
    pub grammar: fn() -> tree_sitter::Language,
    /// The definitions query (compiled lazily on first use)
    pub definitions_query: MemoizedQuery,
}

impl TSLanguageConfig {
    /// Find a config by file extension.
    pub fn from_extension(ext: &str) -> Option<&'static TSLanguageConfig> {
        ALL_LANGUAGES
            .iter()
            .copied()
            .find(|cfg| cfg.file_extensions.contains(&ext))
    }
}

/// A tree-sitter query that is compiled exactly once (lazily).
#[derive(Debug)]
pub struct MemoizedQuery {
    cell: OnceLock<anyhow::Result<tree_sitter::Query>>,
    source: &'static str,
}

impl MemoizedQuery {
    pub const fn new(source: &'static str) -> Self {
        Self {
            cell: OnceLock::new(),
            source,
        }
    }

    /// Return the compiled query, compiling it on first call.
    pub fn get(
        &self,
        grammar: fn() -> tree_sitter::Language,
    ) -> anyhow::Result<&tree_sitter::Query> {
        self.cell
            .get_or_init(|| {
                tree_sitter::Query::new(&grammar(), self.source)
                    .map_err(|e| anyhow::anyhow!("Query compile error: {}", e))
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}
