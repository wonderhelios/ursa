//! UI components for clean CLI display

use std::io::Write;

#[allow(dead_code)]
/// A foldable content block
pub struct FoldableBlock {
    pub title: String,
    pub content: String,
    pub lines: usize,
    pub folded: bool,
}

#[allow(dead_code)]
impl FoldableBlock {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let lines = content.lines().count();
        Self {
            title: title.into(),
            content,
            lines,
            folded: lines > 20,
        }
    }

    pub fn render(&self) -> String {
        if self.folded {
            format!("▶ {} ({} lines, click to expand)\n", self.title, self.lines)
        } else {
            format!("▼ {}\n{}\n", self.title, self.content)
        }
    }
}

/// Clean UI renderer
pub struct CleanUI {
    #[allow(dead_code)]
    current_block: Option<String>,
    buffer: String,
}

impl CleanUI {
    pub fn new() -> Self {
        Self {
            current_block: None,
            buffer: String::new(),
        }
    }

    /// Start a new assistant response
    pub fn start_response(&mut self) {
        self.buffer.clear();
        print!("\n🤖 ");
        std::io::stdout().flush().ok();
    }

    #[allow(dead_code)]
    /// Add a chunk to the current response
    pub fn add_chunk(&mut self, chunk: &str) {
        self.buffer.push_str(chunk);
        print!("{}", chunk);
        std::io::stdout().flush().ok();
    }

    #[allow(dead_code)]
    /// End the current response with optional folding
    pub fn end_response(&mut self) {
        let lines = self.buffer.lines().count();
        if lines > 30 {
            print!("\n\n▶ Output ({} lines) - Press 'o' to expand\n", lines);
        } else {
            print!("\n");
        }
        std::io::stdout().flush().ok();
    }

    #[allow(dead_code)]
    /// Show a tool execution indicator
    pub fn show_tool_start(name: &str) {
        print!("\n⚡ Executing {}... ", name);
        std::io::stdout().flush().ok();
    }

    #[allow(dead_code)]
    /// Show tool completion
    pub fn show_tool_end() {
        println!("✓");
    }

    /// Show an error
    pub fn show_error(msg: &str) {
        eprintln!("\n❌ Error: {}\n", msg);
    }

    /// Print welcome message
    pub fn print_welcome(model: &str, mode: &str, skills_count: usize, tools_count: usize) {
        println!();
        println!("┌─────────────────────────────────────────┐");
        println!("│     🐻 Ursa Agent - Ready to help       │");
        println!("├─────────────────────────────────────────┤");
        println!("│  Model: {:31} │", model);
        println!("│  Mode: {:32} │", mode);
        println!("│  Skills: {:30} │", format!("{} loaded", skills_count));
        println!("│  Tools: {:31} │", format!("{} available", tools_count));
        println!("├─────────────────────────────────────────┤");
        println!("│  Commands:                              │");
        println!("│    /help      - Show all commands       │");
        println!("│    /skills    - List available skills   │");
        println!("│    /history   - List saved sessions     │");
        println!("│    /clear     - Clear conversation      │");
        println!("│    quit       - Exit                    │");
        println!("└─────────────────────────────────────────┘");
        println!();
    }

    #[allow(dead_code)]
    /// Print prompt
    pub fn print_prompt() {
        print!("❯ ");
        std::io::stdout().flush().ok();
    }
}