//! Ursa CLI - AI-powered coding assistant

mod completion;
mod ui;

use std::io::Write;
use std::sync::{Arc, Mutex};
use rustyline::error::ReadlineError;
use ursa_core::context::engine::ContextEngine;
use ursa_core::pipeline::engine::PipelineEngine;
use ursa_core::pipeline::gvrc::ExecutionMode;
use ursa_core::runtime::session::SessionManager;
use ursa_services::memory::store::MemoryStore;
use ursa_services::skills::manager::SkillsManager;
use ursa_treesitter::symbol_index::{build_shared_index, SharedSymbolIndex, is_source_file};
use std::sync::mpsc::channel;
use std::thread;
use notify::Watcher;

use completion::create_editor;

/// Parse command line arguments
fn parse_args() -> (Option<ExecutionMode>, Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut mode = None;
    let mut resume_prefix = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                if i + 1 < args.len() {
                    mode = match args[i + 1].to_lowercase().as_str() {
                        "fast" => Some(ExecutionMode::Fast),
                        "standard" => Some(ExecutionMode::Standard),
                        "strict" => Some(ExecutionMode::Strict),
                        _ => {
                            eprintln!("Warning: Invalid mode '{}'. Using default.", args[i + 1]);
                            None
                        }
                    };
                    i += 2;
                } else {
                    eprintln!("Warning: --mode requires an argument. Using default.");
                    i += 1;
                }
            }
            "--resume" => {
                if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    resume_prefix = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    resume_prefix = Some("".to_string());
                    i += 1;
                }
            }
            "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                // Skip unknown arguments
                i += 1;
            }
        }
    }

    (mode, resume_prefix)
}

/// Print help message
fn print_help() {
    println!("Ursa - AI-powered coding assistant");
    println!();
    println!("Usage: ursa [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --mode MODE      Set execution mode (fast, standard, strict)");
    println!("  --resume [PREFIX] Resume a saved session");
    println!("  --help           Show this help message");
    println!();
    println!("Execution modes:");
    println!("  fast      - Direct tool execution without verification");
    println!("  standard  - Single-stage GVRC with planning");
    println!("  strict    - Multi-stage GVRC with full verification");
}

/// Convert ExecutionMode to a display string
fn mode_to_string(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Fast => "Fast",
        ExecutionMode::Standard => "Standard",
        ExecutionMode::Strict => "Strict",
    }
}

/// Setup logging to file only, not stdout
fn setup_logging() {
    use tracing_subscriber::EnvFilter;

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(".ursa/ursa.log")
        .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap());

    tracing_subscriber::fmt()
        .with_writer(move || log_file.try_clone().unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap()))
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logging();

    // Parse command line arguments
    let (mode_arg, resume_prefix_arg) = parse_args();

    let cwd = std::env::current_dir()?;
    let ursa_dir = cwd.join(".ursa");
    std::fs::create_dir_all(&ursa_dir).ok();

    // Initialize core services
    let memory_file = ursa_dir.join("memory.json");
    let memory_store = Arc::new(Mutex::new(MemoryStore::load(memory_file)?));
    let session_manager = Arc::new(SessionManager::new(ursa_dir.join("sessions")));

    // Initialize LLM
    let config = ursa_llm::models::openai::OpenAIConfig::from_env()
        .expect("URSA_LLM_API_KEY not set");
    let model_name = config.model.clone();

    // Build tool registry
    let todo_manager = Arc::new(Mutex::new(ursa_tools::TodoManager::new()));
    let symbol_index: SharedSymbolIndex = build_shared_index(&cwd);

    // Start file watcher (background incremental updates)
    let watcher_index = symbol_index.clone();
    let watcher_root = cwd.clone();
    thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = match notify::recommended_watcher({
            let tx = tx.clone();
            move |res: Result<notify::Event, notify::Error>| {
                use notify::EventKind;
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            for path in event.paths {
                                if is_source_file(&path) {
                                    let _ = tx.send((path, false));
                                }
                            }
                        }
                        EventKind::Remove(_) => {
                            for path in event.paths {
                                if is_source_file(&path) {
                                    let _ = tx.send((path, true));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&watcher_root, notify::RecursiveMode::Recursive) {
            tracing::warn!("Failed to start file watcher: {}", e);
            return;
        }

        tracing::info!("File watcher started for incremental indexing");

        // Keep watcher alive
        let _watcher = watcher;

        for (path, is_remove) in rx {
            if is_remove {
                if let Ok(mut idx) = watcher_index.write() {
                    idx.remove_file(&path);
                    tracing::debug!("Removed from index: {:?}", path);
                }
            } else if let Ok(source) = std::fs::read_to_string(&path) {
                if let Ok(mut idx) = watcher_index.write() {
                    if let Err(e) = idx.update_file(&path, &source) {
                        tracing::warn!("Failed to update index for {:?}: {}", path, e);
                    } else {
                        tracing::debug!("Incrementally updated: {:?}", path);
                    }
                }
            }
        }
    });

    let mut registry = ursa_tools::ToolRegistry::with_defaults();
    registry.register(ursa_tools::TodoWriteTool::new(todo_manager.clone()));
    registry.register(ursa_tools::MemoryWriteTool::new(memory_store.clone()));
    registry.register(ursa_tools::MemorySearchTool::new(memory_store.clone()));
    registry.register(ursa_tools::tools::symbol_search::SymbolSearchTool::new(symbol_index.clone()));

    let tools_count = registry.all().len();

    // Create LLM provider
    let llm: Arc<dyn ursa_llm::provider::LLMProvider> =
        if let Some(auth) = ursa_llm::resilience::AuthManager::from_env() {
            let resilience = Arc::new(
                ursa_llm::resilience::Resilience::builder()
                    .retry(ursa_llm::resilience::RetryPolicy::default())
                    .auth(auth)
                    .circuit_breaker(ursa_llm::resilience::CircuitBreaker::default())
                    .build(),
            );
            Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config).with_resilience(resilience))
        } else {
            Arc::new(ursa_llm::models::openai::OpenAIProvider::new(config))
        };

    // Register tools that need LLM
    registry.register(ursa_core::SpawnAgentTool::new(llm.clone()));

    // Build pipeline engine
    let context_engine = Arc::new(ContextEngine::new(cwd.clone()));
    let scheduler = Arc::new(ursa_core::runtime::lane::LaneScheduler::default());

    let mut engine_builder = PipelineEngine::new(llm, registry)
        .with_todos(todo_manager)
        .with_memory(memory_store)
        .with_context(context_engine)
        .with_lanes(scheduler)
        .with_symbol_index(symbol_index);

    // Set execution mode from command line argument
    if let Some(mode) = mode_arg {
        engine_builder = engine_builder.with_execution_mode(mode);
    }

    // Load bootstrap system prompt if exists
    let bootstrap = ursa_services::bootstrap::loader::BootstrapLoader::new(cwd);
    if let Some(prompt) = bootstrap.load_system_prompt() {
        engine_builder = engine_builder.with_system_prompt(prompt);
    }

    // Check for --resume flag (now handled by parse_args)
    let initial_messages = if let Some(prefix) = resume_prefix_arg {
        match session_manager.load(Some(&prefix)) {
            Ok(Some(session)) => {
                println!("📂 Resumed session: {} ({} messages)\n", session.id, session.messages.len());
                session.messages
            }
            _ => vec![],
        }
    } else {
        vec![]
    };

    let engine = Arc::new(engine_builder.with_session(session_manager.clone(), initial_messages));

    // Load skills
    let mut skills = SkillsManager::new(std::path::PathBuf::from(".skills"));
    skills.load().await?;
    let skills_count = skills.list().len();

    // Print welcome with mode information
    let mode_str = mode_to_string(engine.execution_mode());
    ui::CleanUI::print_welcome(&model_name, mode_str, skills_count, tools_count);

    // Setup readline with completion
    let mut rl = create_editor()?;
    let history_file = ursa_dir.join("history.txt");
    let _ = rl.load_history(&history_file);

    // Main loop
    loop {
        match rl.readline("❯ ") {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }

                rl.add_history_entry(&line)?;

                if input == "quit" || input == "exit" {
                    println!("👋 Goodbye!");
                    break;
                }

                // Handle built-in commands
                if let Some(rest) = input.strip_prefix('/') {
                    let mut parts = rest.splitn(2, ' ');
                    let cmd = parts.next().unwrap_or("").trim();
                    let arg = parts.next().unwrap_or("").trim();

                    match cmd {
                        "help" => {
                            println!("\n📚 Commands:");
                            println!("  /skills          — list available skills");
                            println!("  /history         — list saved sessions");
                            println!("  /clear           — clear conversation history");
                            println!("  /resume [prefix] — resume a saved session");
                            println!("  quit / exit      — exit\n");
                        }
                        "skills" => println!("\n{}\n", skills.render_list()),
                        "history" => {
                            let sessions = session_manager.list();
                            if sessions.is_empty() {
                                println!("\n📂 No saved sessions.\n");
                            } else {
                                println!("\n📂 Saved sessions:");
                                for s in sessions.iter().take(10) {
                                    println!("  {}", s);
                                }
                                println!();
                            }
                        }
                        "clear" => {
                            engine.clear_conversation();
                            println!("\n🗑️  Conversation history cleared.\n");
                        }
                        "resume" => {
                            let prefix = if arg.is_empty() { None } else { Some(arg) };
                            match session_manager.load(prefix) {
                                Ok(Some(session)) => {
                                    engine.load_session(session.messages);
                                    println!("\n📂 Resumed session: {} ({} messages)\n",
                                        session.id, engine.conversation_len() / 2);
                                }
                                Ok(None) => println!("\n❌ No session found to resume.\n"),
                                Err(e) => println!("\n❌ Failed to load session: {}\n", e),
                            }
                        }
                        _ => {
                            // Try skill invocation
                            if let Some(prompt) = skills.build_invocation(cmd, arg) {
                                ui::CleanUI::start_response(&mut ui::CleanUI::new());
                                let engine_clone = Arc::clone(&engine);
                                match engine_clone.run_stream(&prompt, |chunk| {
                                    print!("{}", chunk);
                                    std::io::stdout().flush().ok();
                                }).await {
                                    Ok(_) => println!("\n"),
                                    Err(e) => ui::CleanUI::show_error(&e.to_string()),
                                }
                            } else {
                                println!("\n❓ Unknown command '/{}'. Type /help for help.\n", cmd);
                            }
                        }
                    }
                    continue;
                }

                // Normal conversation with streaming
                ui::CleanUI::start_response(&mut ui::CleanUI::new());
                let engine_clone = Arc::clone(&engine);
                match engine_clone.run_stream(input, |chunk| {
                    print!("{}", chunk);
                    std::io::stdout().flush().ok();
                }).await {
                    Ok(_) => println!("\n"),
                    Err(e) => ui::CleanUI::show_error(&e.to_string()),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("👋 Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("Readline error: {}", err);
                break;
            }
        }
    }

    // Save history
    rl.save_history(&history_file).ok();
    Ok(())
}