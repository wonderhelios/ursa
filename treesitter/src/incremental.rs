//!
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{debug, trace};
use tree_sitter::QueryCursor;
use tree_sitter::{Parser, Tree};

use crate::language::TSLanguageConfig;
use crate::scope_graph::Definition;

pub struct FileParse {
    parser: Parser,
    tree: Tree,
    definitions: Vec<Definition>,
    source: String,
}

impl FileParse {
    pub fn new(
        source: String,
        config: &'static TSLanguageConfig,
        path: &Path,
    ) -> anyhow::Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(&((config.grammar)()))?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| anyhow::anyhow!("Initial parse failed"))?;

        let definitions = extract_from_tree(&tree, &source, config, path)?;

        Ok(Self {
            parser,
            tree,
            definitions,
            source,
        })
    }

    pub fn update(
        &mut self,
        new_source: String,
        config: &'static TSLanguageConfig,
        path: &Path,
    ) -> anyhow::Result<()> {
        trace!("Incremental parse for {:?}", path);

        let new_tree = self
            .parser
            .parse(&new_source, Some(&self.tree))
            .ok_or_else(|| anyhow::anyhow!("Increment parse failed"))?;

        let old_root = self.tree.root_node();
        let new_root = new_tree.root_node();

        if old_root.to_sexp() == new_root.to_sexp() {
            trace!("AST unchanged, skipping definition extraction");
            self.source = new_source;
            self.tree = new_tree;
            return Ok(());
        }

        self.definitions = extract_from_tree(&new_tree, &new_source, config, path)?;
        self.tree = new_tree;
        self.source = new_source;

        debug!(
            "Incremental update for {:?}: {} definitions",
            path,
            self.definitions.len()
        );

        Ok(())
    }

    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }
}

pub struct IncrementalSymbolIndex {
    files: HashMap<PathBuf, FileParse>,
}

impl IncrementalSymbolIndex {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    pub fn update_file(&mut self, path: &Path, source: &str) -> anyhow::Result<()> {
        let path = path.to_path_buf();

        let ext = path.extension().and_then(|e| e.to_str());
        let config = ext
            .and_then(TSLanguageConfig::from_extension)
            .ok_or_else(|| anyhow::anyhow!("Unsupported file extension"))?;

        if let Some(parser) = self.files.get_mut(&path) {
            parser.update(source.to_string(), config, &path)?;
        } else {
            let parser = FileParse::new(source.to_string(), config, &path)?;
            self.files.insert(path, parser);
        }
        Ok(())
    }

    pub fn remove_file(&mut self, path: &Path) {
        if self.files.remove(path).is_some() {
            debug!("Remove file from index: {:?}", path);
        }
    }

    pub fn search(&self, query: &str) -> Vec<Definition> {
        let q = query.to_lowercase();
        let mut result = Vec::new();

        for parser in self.files.values() {
            for def in parser.definitions() {
                if def.name.to_lowercase().contains(&q) {
                    result.push(def.clone());
                }
            }
        }
        result
    }

    pub fn all_definitions(&self) -> Vec<Definition> {
        self.files
            .values()
            .flat_map(|p| p.definitions().to_vec())
            .collect()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn definition_count(&self) -> usize {
        self.files.values().map(|p| p.definitions().len()).sum()
    }
}

fn extract_from_tree(
    tree: &Tree,
    source: &str,
    config: &'static TSLanguageConfig,
    path: &Path,
) -> anyhow::Result<Vec<Definition>> {
    let query = config.definitions_query.get(config.grammar)?;
    let mut cursor = QueryCursor::new();
    let source_bytes = source.as_bytes();

    let mut definitions = Vec::new();

    for match_ in cursor.matches(query, tree.root_node(), source_bytes) {
        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];

            let Some(kind) = capture_name.strip_prefix("definition.") else {
                continue;
            };

            let Ok(name) = capture.node.utf8_text(source_bytes) else {
                continue;
            };

            definitions.push(Definition {
                name: name.to_string(),
                kind: kind.to_string(),
                file: path.to_path_buf(),
                line: capture.node.start_position().row,
            });
        }
    }

    Ok(definitions)
}
