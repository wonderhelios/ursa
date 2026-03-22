// BootstrapLoader - assembles system prompt from workspace .md files

use std::collections::HashMap;
use std::path::PathBuf;

use tracing::info;

use crate::bootstrap;

// standard bootstrap files, loading in order
const BOOTSTRAP_FILES: &[&str] = &[
    "SOUL.md",
    "IDENTITY.md",
    "AGENTS.md",
    "TOOLS.md",
    "USER.md",
    "MEMORY.md",
    "BOOTSTRP.md",
];

const MAX_FILE_CHARS: usize = 20_000;
const MAX_TOTAL_CHARS: usize = 100_000;

pub enum LoadMode {
    // load all bootstrap files
    Full,
    // load only AGENTS.md and TOOLS.md
    Minimal,
}

pub struct BootstrapLoader {
    workspace_dir: PathBuf,
}

impl BootstrapLoader {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }

    // load bootstrap files according to mode
    pub fn load(&self, mode: LoadMode) -> HashMap<String, String> {
        let files = match mode {
            LoadMode::Minimal => &["AGENTS.md", "TOOLS.md"] as &[&str],
            LoadMode::Full => BOOTSTRAP_FILES,
        };

        let mut result = HashMap::new();
        let mut total = 0usize;

        for name in files {
            let path = self.workspace_dir.join(name);
            if !path.exists() {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.trim().is_empty() {
                continue;
            }

            let truncated = truncate(&content, MAX_FILE_CHARS);

            if total + truncated.len() > MAX_TOTAL_CHARS {
                let remaining = MAX_TOTAL_CHARS.saturating_sub(total);
                if remaining > 100 {
                    result.insert(name.to_string(), truncate(&content, remaining));
                }
                break;
            }

            total += truncated.len();
            result.insert(name.to_string(), truncated.clone());
            info!("Bootstrap loaded: {} ({} chars)", name, truncated.len());
        }
        result
    }

    // assemble into a single system prompt string
    pub fn assemble(&self, bootstraps: &HashMap<String, String>) -> String {
        if bootstraps.is_empty() {
            return String::new();
        }
        BOOTSTRAP_FILES
            .iter()
            .filter_map(|name| {
                bootstraps
                    .get(*name)
                    .map(|content| format!("# {}\n{}", name, content))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    // load and assemble in once call. returns None if no bootstrap files found
    pub fn load_system_prompt(&self) -> Option<String> {
        let bootstraps = self.load(LoadMode::Full);
        if bootstraps.is_empty() {
            None
        } else {
            Some(self.assemble(&bootstraps))
        }
    }
}

fn truncate(content: &str, max: usize) -> String {
    if content.len() <= max {
        return content.to_string();
    }
    let cut = content[..max].rfind('\n').unwrap_or(max);
    format!("{}\n\n[... truncated ...]", &content[..cut])
}
