use crate::error::VernacularError;
use crate::model::TranslationEntry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Parses a RON file containing a flat `String -> String` map.
///
/// Expected format:
/// ```ron
/// {
///     "ui.start": "Start Game",
///     "dialogue.greetings": "Hello, {}!",
/// }
/// ```
pub fn parse(path: &Path) -> Result<HashMap<String, TranslationEntry>, VernacularError> {
    let content = fs::read_to_string(path)?;
    let parsed: HashMap<String, String> = ron::from_str(&content)?;
    let source: Arc<Path> = Arc::from(path.to_path_buf());

    let mut locale_map = HashMap::new();
    for (key, val) in parsed {
        let entry = TranslationEntry::new(val, Arc::clone(&source), 0);
        locale_map.insert(key, entry);
    }

    Ok(locale_map)
}

/// Scans a RON file for translation keys (used by codegen).
pub fn scan_keys(path: &Path) -> Result<Vec<String>, VernacularError> {
    let content = fs::read_to_string(path)?;
    // We only care about the keys, but ron::from_str requires a type.
    // We can deserialize into HashMap<String, ron::Value> to ignore values.
    let parsed: HashMap<String, ron::Value> = ron::from_str(&content)?;

    Ok(parsed.into_keys().collect())
}
