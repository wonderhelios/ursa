use std::path::Path;

use tracing::{debug, info, warn};

use crate::language::TSLanguageConfig;
use crate::scope_graph::{Definition, extract_definitions};

const IGNORED_DIRS: &[&str] = &["target", ".git", "node_modules", ".ursa", ".vscode"];

pub struct SymbolIndex {
    definitions: Vec<Definition>,
}

impl SymbolIndex {
    pub fn build(workspace_root: &Path) -> Self {
        let mut index = Self {
            definitions: Vec::new(),
        };
        index.scan_dir(workspace_root);
        info!("SymbolIndex ready: {} definitions", index.definitions.len());
        index
    }

    pub fn search(&self, query: &str) -> Vec<&Definition> {
        let q = query.to_lowercase();
        self.definitions
            .iter()
            .filter(|d| d.name.to_lowercase().contains(&q))
            .collect()
    }

    pub fn definitions_in_file(&self, file: &Path) -> Vec<&Definition> {
        self.definitions.iter().filter(|d| d.file == file).collect()
    }

    pub fn update_file(&mut self, file: &Path, source: &str) {
        self.definitions.retain(|d| d.file != file);
        if let Some(config) = file
            .extension()
            .and_then(|e| e.to_str())
            .and_then(TSLanguageConfig::from_extension)
        {
            match extract_definitions(source, config, file) {
                Ok(defs) => self.definitions.extend(defs),
                Err(e) => warn!("Failed to re-index {:?}: {}", file, e),
            }
        }
    }

    pub fn definition_count(&self) -> usize {
        self.definitions.len()
    }

    /// Create an empty index (useful for testing).
    pub fn new_empty() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    fn scan_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !IGNORED_DIRS.contains(&name.as_str()) {
                    self.scan_dir(&path);
                }
            } else if let Some(config) = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(TSLanguageConfig::from_extension)
            {
                self.index_file(&path, config);
            }
        }
    }

    fn index_file(&mut self, path: &Path, config: &'static TSLanguageConfig) {
        let Ok(source) = std::fs::read_to_string(path) else {
            return;
        };
        match extract_definitions(&source, config, path) {
            Ok(defs) => {
                debug!("Indexed {} definitions in {:?}", defs.len(), path);
                self.definitions.extend(defs);
            }
            Err(e) => warn!("Parse error in {:?}: {}", path, e),
        }
    }
}

