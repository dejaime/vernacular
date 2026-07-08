use crate::error::VernacularError;
use crate::model::TranslationEntry;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Parses a "unified" CSV file with one column per locale.
///
/// Expected format:
/// ```csv
/// locale,en_US,ja_JP
/// items.sword,Iron Sword,鉄の剣
/// ```
///
/// The first row is always treated as a header naming the locale columns.
pub fn parse_unified(path: &Path) -> Result<crate::model::LocaleMap, VernacularError> {
    let mut map: crate::model::LocaleMap = HashMap::new();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)?;
    let source: Arc<Path> = Arc::from(path.to_path_buf());

    let mut locales = Vec::new();
    let mut first_row = true;

    for (line_idx, result) in rdr.records().enumerate() {
        let record = result?;

        if first_row {
            first_row = false;
            for i in 1..record.len() {
                locales.push(record[i].to_string());
                map.insert(record[i].to_string(), HashMap::new());
            }
            continue;
        }

        if let Some(key_field) = record.get(0) {
            let key = key_field.to_string();
            for (i, locale) in locales.iter().enumerate() {
                if i + 1 < record.len() {
                    let raw_val = &record[i + 1];
                    if !raw_val.is_empty() {
                        let val = raw_val.trim();
                        if let Some(locale_map) = map.get_mut(locale) {
                            locale_map.insert(
                                key.clone(),
                                TranslationEntry::new(
                                    val.to_string(),
                                    Arc::clone(&source),
                                    line_idx + 1,
                                ),
                            );
                        }
                    }
                }
            }
        } else {
            crate::v_warn!(
                "Ignored empty or malformed row {} in unified CSV '{}'",
                line_idx + 1,
                path.display()
            );
        }
    }

    Ok(map)
}

/// Scans a CSV file for translation keys (used by codegen).
///
/// Handles both unified (multi-locale) and per-locale (key,value) CSV formats.
/// Skips header rows depending on `is_root`.
pub fn scan_keys(path: &Path, is_root: bool) -> Result<Vec<String>, VernacularError> {
    let mut keys = Vec::new();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)?;

    for (line_idx, result) in rdr.records().enumerate() {
        let record = result?;

        // Skip common header rows for both unified and per-locale CSVs.
        if line_idx == 0 {
            if is_root {
                continue;
            } else if let Some(key_field) = record.get(0) {
                let key = key_field.trim();
                if key == "key" || key == "locale" {
                    continue;
                }
            }
        }

        if let Some(key_field) = record.get(0) {
            let key = key_field.trim().to_string();
            if !key.is_empty() {
                keys.push(key);
            }
        }
    }

    Ok(keys)
}

/// Parses a per-locale CSV file with `key,value` rows.
///
/// Expected format (no header):
/// ```csv
/// items.sword,Iron Sword
/// items.shield,Wooden Shield
/// ```
///
/// If a `key,value` header row is detected, it is skipped with a warning.
pub fn parse_locale(path: &Path) -> Result<HashMap<String, TranslationEntry>, VernacularError> {
    let mut locale_map = HashMap::new();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)?;
    let source: Arc<Path> = Arc::from(path.to_path_buf());

    for (line_idx, result) in rdr.records().enumerate() {
        let record = result?;
        if record.len() >= 2 {
            let key = record[0].to_string();
            let val = record[1].to_string();

            // Sanity check: if a user puts a "key,value" header row, warn them.
            if line_idx == 0 && key == "key" && val == "value" {
                crate::v_warn!(
                    "Ignored first row of '{}' because it is a \"key,value\" header. \
                    Per-locale CSVs should not have headers.",
                    path.display()
                );
                continue;
            }

            locale_map.insert(
                key,
                TranslationEntry::new(val, Arc::clone(&source), line_idx + 1),
            );
        }
    }

    Ok(locale_map)
}
