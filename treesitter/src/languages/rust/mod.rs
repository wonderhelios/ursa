use crate::language::{MemoizedQuery, TSLanguageConfig};

pub static RUST: TSLanguageConfig = TSLanguageConfig {
    language_ids: &["Rust"],
    file_extensions: &["rs"],
    grammar: tree_sitter_rust::language,
    definitions_query: MemoizedQuery::new(include_str!("./definitions.scm")),
};
