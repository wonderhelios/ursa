//! Ursa CLI
///
mod commands;
mod config;

use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Ursa starting");

    let config =
        ursa_llm::models::openai::OpenAIConfig::from_env().expect("URSA_LLM_API_KEY not set");

    let llm = Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config));

    // Shared TodoManager: both the tool and the engine hold a reference
    let todo_manager = Arc::new(Mutex::new(ursa_tools::TodoManager::new()));

    // Build registry with all 5 tools
    let mut registry = ursa_tools::ToolRegistry::with_defaults();
    registry.register(ursa_tools::TodoWriteTool::new(todo_manager.clone()));

    let engine = ursa_core::pipeline::engine::PipelineEngine::new(llm, registry)
        .with_todos(todo_manager.clone());

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

        match engine.run(input).await {
            Ok(resp) => println!("\n{}\n", resp),
            Err(e) => eprintln!("\nError: {}\n", e),
        }
    }

    Ok(())
}
