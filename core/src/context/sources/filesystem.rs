//! FileSystemSource - scans workspace source files

use std::path::{Path, PathBuf};


const IGNORED_DIRS: &[&str] = &["target", ".git", "node_modules", ".ursa", ".vscode"];
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "toml", "md", "ts", "py", "go", "json", "c", "cpp", "cc", "h",
];

pub struct WorkspaceFile {
    pub path: PathBuf,
    pub relative_path: String,
}

pub struct FileSystemSource {
    workspace_root: PathBuf,
    max_file: usize,
}

impl FileSystemSource {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            max_file: 60,
        }
    }

    //scan and return source files, sorted by modification time (newest first)
    pub fn scan(&self) -> Vec<WorkspaceFile> {
        let mut files = Vec::new();
        self.scan_dir(&self.workspace_root, &mut files);

        files.sort_by(|a, b| {
            let ma = std::fs::metadata(&a.path).and_then(|m| m.modified()).ok();
            let mb = std::fs::metadata(&b.path).and_then(|m| m.modified()).ok();
            mb.cmp(&ma)
        });
        files.truncate(self.max_file);
        files
    }

    // render compact listing for system prompt injection
    pub fn render_listing(&self) -> String {
        let files = self.scan();
        if files.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = files
            .iter()
            .map(|f| format!(" - {}", f.relative_path))
            .collect();
        format!("## Workspace Files\n{}", lines.join("\n"))
    }

    fn scan_dir(&self, dir: &Path, result: &mut Vec<WorkspaceFile>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if !IGNORED_DIRS.contains(&name.as_str()) {
                    self.scan_dir(&path, result);
                }
            } else if let Some(ext) = path.extension()
                && SOURCE_EXTENSIONS.contains(&ext.to_str().unwrap_or(""))
                    && let Ok(rel) = path.strip_prefix(&self.workspace_root) {
                        result.push(WorkspaceFile {
                            path: path.clone(),
                            relative_path: rel.to_string_lossy().to_string(),
                        });
                    }
        }
    }
}
