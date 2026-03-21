// SkillsManager - loads and manages skills from a directory

use std::collections::HashMap;
use std::path::PathBuf;

use tokio::fs;
use tracing::{info, warn};

use super::loader::{Skill, load_skill_file};

pub struct SkillsManager {
    skills_dir: PathBuf,
    skills: HashMap<String, Skill>,
}

impl SkillsManager {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            skills: HashMap::new(),
        }
    }

    // load all .md files from the skills directory
    pub async fn load(&mut self) -> anyhow::Result<()> {
        if !self.skills_dir.exists() {
            info!("Skills directory {:?} not found, skipping", self.skills_dir);
            return Ok(());
        }
        let mut entries = fs::read_dir(&self.skills_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                match load_skill_file(&path).await {
                    Ok(skill) => {
                        info!("Loaded skill: {}", skill.name);
                        self.skills.insert(skill.name.clone(), skill);
                    }
                    Err(e) => warn!("Failed to load skill {:?}: {}", path, e),
                }
            }
        }

        info!("Loaded {} skills", self.skills.len());
        Ok(())
    }

    // get a skill by name
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    // get just the prompt for a skill
    pub fn get_prompt(&self, name: &str) -> Option<String> {
        self.skills.get(name).map(|s| s.prompt.clone())
    }

    // list all loaed skills
    pub fn list(&self) -> Vec<&Skill> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by_key(|s| &s.name);
        skills
    }

    // find skills by tag
    pub fn find_by_tag(&self, tag: &str) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| s.tags.iter().any(|t| t == tag))
            .collect()
    }

    // render a help listing for the user
    pub fn render_list(&self) -> String {
        let skills = self.list();
        if skills.is_empty() {
            return "No skills loaded. Add .md files to the .skills/ directory.".to_string();
        }
        let lines: Vec<String> = skills
            .iter()
            .map(|s| {
                if s.description.is_empty() {
                    format!("  /{}", s.name)
                } else {
                    format!("   /{:<20} {}", s.name, s.description)
                }
            })
            .collect();
        format!("Avaliable skills:\n{}", lines.join("\n"))
    }

    // build the full prompt to send to the engine for a skill invocation
    // appends optional user-provided context after the skill prompt
    pub fn build_invocation(&self, name: &str, context: &str) -> Option<String> {
        let skill = self.get(name)?;
        if context.is_empty() {
            Some(skill.prompt.clone())
        } else {
            Some(format!(
                "{}\n\nAdditional context from user: {}",
                skill.prompt, context
            ))
        }
    }
}
