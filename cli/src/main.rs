//! Ursa CLI
///
mod commands;
mod config;

use std::io::Write;
use std::sync::{Arc, Mutex};
use tracing::info;
use ursa_core::context::engine::ContextEngine;
use ursa_core::runtime::session::SessionManager;
use ursa_services::bootstrap::loader::BootstrapLoader;
use ursa_services::memory::store::MemoryStore;
use ursa_services::skills::manager::SkillsManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    info!("Ursa starting");

    let cwd = std::env::current_dir()?;

    // bootstrap loader - dynamic system prompt
    let bootstrap = BootstrapLoader::new(cwd.clone());
    let system_prompt = bootstrap.load_system_prompt();
    if system_prompt.is_some() {
        info!("Bootstrap system prompt loaded");
    }

    // memory store - persistent memory at .ursa/memory.json
    let memory_file = cwd.join(".ursa").join("memory.json");
    let memory_store = Arc::new(Mutex::new(MemoryStore::load(memory_file)?));

    // session manager
    let session_manager = Arc::new(SessionManager::new(cwd.join(".ursa").join("sessions")));

    // delivery queue
    let delivery_queue = Arc::new(ursa_services::delivery::queue::DeliveryQueue::new(
        cwd.join(".ursa"),
    )?);

    // start background runner
    ursa_services::delivery::runner::DeliveryRunner::new(delivery_queue.clone()).start();

    // context engine
    let context_engine = Arc::new(ContextEngine::new(cwd.clone()));

    let config =
        ursa_llm::models::openai::OpenAIConfig::from_env().expect("URSA_LLM_API_KEY not set");

    // resilience (retry + auth rotation + circuit breaker)
    // Automatically enabled if URSA_LLM_API_KEY is set (always true)
    // Backup keys: URSA_LLM_API_KEY_2, URSA_LLM_API_KEY_3 (optional)
    let llm = if let Some(auth) = ursa_llm::resilience::AuthManager::from_env() {
        let resilience = Arc::new(
            ursa_llm::resilience::Resilience::builder()
                .retry(ursa_llm::resilience::RetryPolicy::default())
                .auth(auth)
                .circuit_breaker(ursa_llm::resilience::CircuitBreaker::default())
                .build(),
        );
        Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config).with_resilience(resilience))
            as Arc<dyn ursa_llm::provider::LLMProvider>
    } else {
        Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config))
            as Arc<dyn ursa_llm::provider::LLMProvider>
    };

    // shared TodoManager: both the tool and the engine hold a reference
    let todo_manager = Arc::new(Mutex::new(ursa_tools::TodoManager::new()));

    // build registry with all tools
    let mut registry = ursa_tools::ToolRegistry::with_defaults();
    registry.register(ursa_tools::TodoWriteTool::new(todo_manager.clone()));
    registry.register(ursa_core::SpawnAgentTool::new(llm.clone()));
    registry.register(ursa_tools::MemoryWriteTool::new(memory_store.clone()));
    registry.register(ursa_tools::MemorySearchTool::new(memory_store.clone()));
    registry.register(ursa_tools::NotifyTool::new(delivery_queue.clone()));

    // lane scheduler - serializes user requests through LANE_MAIN
    let scheduler = Arc::new(ursa_core::runtime::lane::LaneScheduler::default());

    let mut engine_builder = ursa_core::pipeline::engine::PipelineEngine::new(llm, registry)
        .with_todos(todo_manager.clone())
        .with_memory(memory_store)
        .with_context(context_engine)
        .with_lanes(scheduler);

    if let Some(prompt) = system_prompt {
        engine_builder = engine_builder.with_system_prompt(prompt);
    }

    // check for -- resume flag (resume most rencent session)
    let resume_prefix = std::env::args().nth(1).filter(|a| a != "--help");
    let initial_messages = if resume_prefix.as_deref() == Some("--resume") {
        let prefix = std::env::args().nth(2);
        match session_manager.load(prefix.as_deref()) {
            Ok(Some(session)) => {
                println!(
                    "Resumed session: {} ({} messages)\n",
                    session.id,
                    session.messages.len()
                );
                session.messages
            }
            Ok(None) => {
                println!("No session found to resume.\n");
                vec![]
            }
            Err(e) => {
                eprintln!("Failed to load session: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    let engine = engine_builder.with_session(session_manager.clone(), initial_messages);

    // load skills from .skills/ directory in cwd
    let mut skills = SkillsManager::new(std::path::PathBuf::from(".skills"));
    skills.load().await?;

    println!("Ursa Agent - type '/help' for commands, 'quit' to exit\n");

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

        // built-in commands
        if let Some(rest) = input.strip_prefix('/') {
            let mut parts = rest.splitn(2, ' ');
            let cmd = parts.next().unwrap_or("").trim();
            let arg = parts.next().unwrap_or("").trim();

            match cmd {
                "help" => {
                    println!("\nCommands:");
                    println!("  /skills          — list available skills");
                    println!("  /history         — list saved sessions");
                    println!("  /clear           — clear conversation history");
                    println!(
                        "  /resume [id]     — not available mid-session; restart with --resume"
                    );
                    println!("  quit / exit      — exit\n");
                    continue;
                }
                "skills" => {
                    println!("\n{}\n", skills.render_list());
                    continue;
                }
                "history" => {
                    let sessions = session_manager.list();
                    if sessions.is_empty() {
                        println!("\nNo saved sessions.\n");
                    } else {
                        println!("\nSaved sessions:");
                        for s in sessions.iter().take(10) {
                            println!("  {}", s);
                        }
                        println!();
                    }
                    continue;
                }
                "clear" => {
                    engine.clear_conversation();
                    println!("\nConversation history cleared.\n");
                    continue;
                }
                _ => {
                    // Try skill invocation
                    match skills.build_invocation(cmd, arg) {
                        Some(prompt) => match engine.run(&prompt).await {
                            Ok(resp) => println!("\n{}\n", resp),
                            Err(e) => eprintln!("\nError: {}\n", e),
                        },
                        None => {
                            println!("\nUnknown command '/{}'. Type /help for help.\n", cmd);
                        }
                    }
                    continue;
                }
            }
        }

        // Normal conversation
        match engine.run(input).await {
            Ok(resp) => println!("\n{}\n", resp),
            Err(e) => eprintln!("\nError: {}\n", e),
        }
    }

    Ok(())
}
