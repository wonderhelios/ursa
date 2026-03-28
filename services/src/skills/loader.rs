// SkillLoader - Load skills from Markdown

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub file_path: PathBuf,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// load a single skill from a .md file
pub async fn load_skill_file(path: &Path) -> anyhow::Result<Skill> {
    let content = fs::read_to_string(path)
        .await
        .map_err(|e| anyhow!("Failed to read {:?}: {}", path, e))?;

    let (frontmatter, prompt) = parse_frontmatter(&content)?;

    let name = frontmatter.get("name").cloned().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    let description = frontmatter.get("description").cloned().unwrap_or_default();

    let tags = frontmatter
        .get("tags")
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    Ok(Skill {
        name,
        description,
        prompt: prompt.trim().to_string(),
        file_path: path.to_path_buf(),
        tags,
        metadata: frontmatter,
    })
}

/// Parse YAML-stype frontmatter
/// Format:
///   ---
///   key: value
///   ---
///   body content
fn parse_frontmatter(content: &str) -> anyhow::Result<(HashMap<String, String>, &str)> {
    let mut meta = HashMap::new();
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Ok((meta, content));
    }

    let after_open = &trimmed[3..];
    let Some(close_pos) = after_open.find("---") else {
        return Ok((meta, content));
    };

    let fm_content = &after_open[..close_pos];
    let body = &after_open[close_pos + 3..];

    for line in fm_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_string();
            let val = line[colon + 1..].trim().to_string();
            meta.insert(key, val);
        }
    }

    Ok((meta, body.trim_start()))
}
