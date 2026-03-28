use std::path::Path;
use std::sync::{Arc, RwLock};

use tracing::{debug, info, warn};

use crate::incremental::IncrementalSymbolIndex;
use crate::language::TSLanguageConfig;

const IGNORED_DIRS: &[&str] = &["target", ".git", "node_modules", ".ursa", ".vscode"];

/// Thread-safe symbol index
pub type SharedSymbolIndex = Arc<RwLock<IncrementalSymbolIndex>>;

/// Create a new shared index
pub fn create_shared_index() -> SharedSymbolIndex {
    Arc::new(RwLock::new(IncrementalSymbolIndex::new()))
}

/// Build shared index from workspace directory
pub fn build_shared_index(workspace_root: &Path) -> SharedSymbolIndex {
    let index = IncrementalSymbolIndex::new();
    let shared = Arc::new(RwLock::new(index));

    // Initial scan
    if let Ok(guard) = shared.write() {
        drop(guard); // Release lock first
        scan_dir_recursive(workspace_root, &shared);
    }

    info!("SymbolIndex built for {:?}", workspace_root);
    shared
}

/// Recursively scan directory
fn scan_dir_recursive(dir: &Path, index: &SharedSymbolIndex) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !IGNORED_DIRS.contains(&name.as_str()) {
                scan_dir_recursive(&path, index);
            }
        } else if let Some(config) = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(TSLanguageConfig::from_extension)
        {
            index_file(&path, config, index);
        }
    }
}

/// Index a single file
fn index_file(path: &Path, _config: &'static TSLanguageConfig, index: &SharedSymbolIndex) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };

    if let Ok(mut guard) = index.write() {
        if let Err(e) = guard.update_file(path, &source) {
            warn!("Failed to index {:?}: {}", path, e);
        } else {
            debug!("Indexed {:?}", path);
        }
    }
}

/// Check if file is a supported source file
pub fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("rs" | "go" | "ts" | "js" | "py" | "c" | "cpp" | "h" | "hpp")
    )
}
