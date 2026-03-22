//! Integration tests for Tree-sitter module.

use std::path::PathBuf;
use ursa_treesitter::language::TSLanguageConfig;
use ursa_treesitter::scope_graph::extract_definitions;
use ursa_treesitter::symbol_index::SymbolIndex;

const TEST_RUST_CODE: &str = r#"
// A simple function
fn add(a: i32, b: i32) -> i32 {
    a + b
}

// A struct
pub struct Point {
    x: f64,
    y: f64,
}

// An enum
enum Status {
    Active,
    Inactive,
}

// A trait
trait Drawable {
    fn draw(&self);
}

// A const
const MAX_SIZE: usize = 100;

// A type alias
type MyResult = Result<i32, String>;

// A module
mod utils {
    pub fn helper() {}
}

// impl block
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}
"#;

#[test]
fn test_language_from_extension() {
    let config = TSLanguageConfig::from_extension("rs");
    assert!(config.is_some());
    let config = config.unwrap();
    assert!(config.language_ids.contains(&"Rust"));
}

#[test]
fn test_language_not_found() {
    let config = TSLanguageConfig::from_extension("py");
    assert!(config.is_none(), "Python not enabled yet");
}

#[test]
fn test_extract_definitions() {
    let config = TSLanguageConfig::from_extension("rs").unwrap();
    let file = PathBuf::from("test.rs");

    let defs = extract_definitions(TEST_RUST_CODE, config, &file).unwrap();

    // Should find: add, Point, Status, Drawable, MAX_SIZE, MyResult, utils, Point (impl)
    println!("Found {} definitions:", defs.len());
    for d in &defs {
        println!("  - {}: {} (line {})", d.kind, d.name, d.line + 1);
    }

    // Check that we found the expected definitions
    let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"add"), "Should find function 'add'");
    assert!(names.contains(&"Point"), "Should find struct 'Point'");
    assert!(names.contains(&"Status"), "Should find enum 'Status'");
    assert!(names.contains(&"Drawable"), "Should find trait 'Drawable'");
    assert!(names.contains(&"MAX_SIZE"), "Should find const 'MAX_SIZE'");
    assert!(names.contains(&"MyResult"), "Should find type alias 'MyResult'");
    assert!(names.contains(&"utils"), "Should find module 'utils'");
}

#[test]
fn test_definition_display() {
    let config = TSLanguageConfig::from_extension("rs").unwrap();
    let file = PathBuf::from("src/main.rs");

    let defs = extract_definitions("fn test() {}", config, &file).unwrap();
    assert_eq!(defs.len(), 1);

    let display = defs[0].display();
    assert!(display.contains("function"));
    assert!(display.contains("test"));
    assert!(display.contains("src/main.rs"));
}

#[test]
fn test_symbol_index_update_and_search() {
    let mut index = SymbolIndex::new_empty();

    // Add some "files"
    index.update_file(&PathBuf::from("a.rs"), "fn foo() {}\nfn bar() {}");
    index.update_file(&PathBuf::from("b.rs"), "struct Foo {}");

    // Search for "foo" (case-insensitive)
    let results = index.search("foo");
    println!("Search 'foo': found {} results", results.len());
    for r in &results {
        println!("  - {} in {:?}", r.name, r.file);
    }
    assert_eq!(results.len(), 2, "Should find both 'foo' function and 'Foo' struct");

    // Search for "bar"
    let results = index.search("bar");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "bar");

    // Update a file
    index.update_file(&PathBuf::from("a.rs"), "fn updated() {}");
    assert_eq!(index.definition_count(), 2); // updated + Foo
    assert_eq!(index.search("updated").len(), 1);
    assert_eq!(index.search("bar").len(), 0);
}

#[test]
fn test_symbol_index_definitions_in_file() {
    let mut index = SymbolIndex::new_empty();
    index.update_file(&PathBuf::from("a.rs"), "fn foo() {}\nfn bar() {}");
    index.update_file(&PathBuf::from("b.rs"), "struct Baz {}");

    let defs = index.definitions_in_file(&PathBuf::from("a.rs"));
    assert_eq!(defs.len(), 2);
}
