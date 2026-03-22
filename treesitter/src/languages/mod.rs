mod rust;

use crate::language::TSLanguageConfig;

/// All registered languages. Add new languages here.
pub static ALL_LANGUAGES: &[&TSLanguageConfig] = &[&rust::RUST];
