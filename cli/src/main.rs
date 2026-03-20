///! Ursa CLI
///
mod commands;
mod config;

use std::io::Write;
use std::sync::Arc;
use tracing::info;
use ursa_tools::registry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Ursa starting");

    let config =
        ursa_llm::models::openai::OpenAIConfig::from_env().expect("URSA_LLM_API_KEY not set");

    let llm = Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config));
    let registry = ursa_tools::ToolRegistry::with_defaults();
    let engine = ursa_core::pipeline::engine::PipelineEngine::new(llm, registry);

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
