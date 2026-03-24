//! CLI configuration

use std::path::PathBuf;

/// CLI configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Working directory
    pub cwd: PathBuf,
    /// Config directory
    pub config_dir: PathBuf,
}

impl Config {
    /// Create new config from working directory
    pub fn new(cwd: PathBuf) -> Self {
        let config_dir = cwd.join(".ursa");
        Self { cwd, config_dir }
    }
}
