//! Ursa CLI
///
mod commands;
mod config;

use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::info;
use ursa_services::skills::loader::Skill;
use ursa_services::skills::manager::SkillsManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    info!("Ursa starting");

    let config =
        ursa_llm::models::openai::OpenAIConfig::from_env().expect("URSA_LLM_API_KEY not set");

    let llm = Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config));

    // shared TodoManager: both the tool and the engine hold a reference
    let todo_manager = Arc::new(Mutex::new(ursa_tools::TodoManager::new()));

    // build registry with all tools
    let mut registry = ursa_tools::ToolRegistry::with_defaults();
    registry.register(ursa_tools::TodoWriteTool::new(todo_manager.clone()));
    registry.register(ursa_core::SpawnAgentTool::new(llm.clone()));

    let engine = ursa_core::pipeline::engine::PipelineEngine::new(llm, registry)
        .with_todos(todo_manager.clone());

    // load skills from .skills/ directory in cwd
    let mut skills = SkillsManager::new(std::path::PathBuf::from(".skills"));
    skills.load().await?;

    println!("Ursa Agent - type 'quit' to exit\n");

    loop {
        print!("> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "quit" || input == "exit" {
            println!("Goodbye!");
            break;
        }
        if input.is_empty() {
            continue;
        }

        // skill invocation: /skill-name [context]
        if let Some(rest) = input.strip_prefix('/') {
            let mut parts = rest.splitn(2, ' ');
            let skill_name = parts.next().unwrap_or("").trim();
            let context = parts.next().unwrap_or("").trim();

            // built-in /skills command
            if skill_name == "skills" {
                println!("\n{}\n", skills.render_list());
                continue;
            }

            // look up skill
            match skills.build_invocation(skill_name, context) {
                Some(prompt) => {
                    info!("Invoking skill: {}", skill_name);
                    match engine.run(&prompt).await {
                        Ok(resp) => println!("\n{}\n", resp),
                        Err(e) => eprintln!("\nError: {}\n", e),
                    }
                }
                None => {
                    println!(
                        "\nUnknown skill '{}'. Type /skills to see available skills.\n",
                        skill_name
                    );
                }
            }
            continue;
        }

        // normal conversation
        match engine.run(input).await {
            Ok(resp) => println!("\n{}\n", resp),
            Err(e) => eprintln!("\nError: {}\n", e),
        }
    }

    Ok(())
}
