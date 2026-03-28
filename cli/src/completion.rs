//! Command completion for rustyline

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper, Result};

/// Command completer for Ursa CLI
pub struct UrsaHelper {
    commands: Vec<String>,
}

impl UrsaHelper {
    pub fn new() -> Self {
        Self {
            commands: vec![
                "help".to_string(),
                "skills".to_string(),
                "history".to_string(),
                "clear".to_string(),
                "resume".to_string(),
            ],
        }
    }
}

impl Completer for UrsaHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        // Only complete if line starts with '/'
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        // Find the start of the command (after '/')
        let start = 1;
        let prefix = &line[start..pos];

        // Filter commands that match the prefix
        let matches: Vec<Pair> = self
            .commands
            .iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .map(|cmd| Pair {
                display: format!("/{} - {}", cmd, description(cmd)),
                replacement: cmd[pos - start..].to_string(),
            })
            .collect();

        Ok((pos, matches))
    }
}

impl Highlighter for UrsaHelper {}
impl Hinter for UrsaHelper {
    type Hint = String;
}
impl Validator for UrsaHelper {}
impl Helper for UrsaHelper {}

fn description(cmd: &str) -> &'static str {
    match cmd {
        "help" => "Show all commands",
        "skills" => "List available skills",
        "history" => "List saved sessions",
        "clear" => "Clear conversation history",
        "resume" => "Resume a saved session",
        _ => "Command",
    }
}

/// Helper function to create a configured rustyline editor with completion
pub fn create_editor() -> Result<rustyline::Editor<UrsaHelper, rustyline::history::DefaultHistory>> {
    let helper = UrsaHelper::new();
    let config = rustyline::Config::builder()
        .completion_type(rustyline::CompletionType::List)
        .build();

    let mut editor = rustyline::Editor::with_config(config)?;
    editor.set_helper(Some(helper));
    Ok(editor)
}
