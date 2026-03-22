//! ContextEngine - assembles workspace context for system prompt injection

use super::compression::ContextCompressor;
use super::sources::filesystem::FileSystemSource;
use std::path::PathBuf;

pub struct ContextEngine {
    fs_source: FileSystemSource,
    compressor: ContextCompressor,
}

impl ContextEngine {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            fs_source: FileSystemSource::new(workspace_root),
            compressor: ContextCompressor::default(),
        }
    }

    // build a context string suitable for injection into the system prompt
    pub fn build_context(&self) -> String {
        let listing = self.fs_source.render_listing();
        if listing.is_empty() {
            return String::new();
        }
        self.compressor.truncate(&listing)
    }
}
