//! ContextCompressor - trims context to fit token budgets

use std::fmt::format;

pub struct ContextCompressor {
    max_chars: usize,
}

impl ContextCompressor {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    // truncate content at a line boundary to fit within budget
    pub fn truncate(&self, content: &str) -> String {
        if content.len() <= self.max_chars {
            return content.to_string();
        }
        let cut = content[..self.max_chars]
            .rfind('\n')
            .unwrap_or(self.max_chars);
        format!("{}\n[... truncated ...]", &content[..cut])
    }

    // rough token estimate: ~4 chars per token
    pub fn estimate_tokens(content: &str) -> usize {
        content.len() / 4
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new(6000)
    }
}
