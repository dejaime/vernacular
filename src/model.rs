use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// A map of locales to their translated entries.
pub type LocaleMap = HashMap<String, HashMap<String, TranslationEntry>>;

/// A part of a pre-parsed template.
///
/// Text spans reference byte ranges into the parent [`TranslationEntry::value`] string
/// to avoid per-fragment heap allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Byte range `[start..end)` into the entry's `value` string.
    Text { start: usize, end: usize },
    /// A literal `{` produced by the `{{` escape sequence.
    LiteralOpen,
    /// A literal `}` produced by the `}}` escape sequence.
    LiteralClose,
    /// Argument index (e.g. `{0}`). For `{}`, sequential indices are assigned.
    Arg(usize),
}

/// Represents a single translated string, tracking its value and origin.
///
/// The `source_path` is shared ([`Arc<Path>`]) across all entries parsed from
/// the same file to avoid redundant heap allocations.
///
/// **Note on Line Numbers:** Due to parser limitations, entries parsed from RON files
/// will always have a `line` value of `0`. Duplicate key diagnostics will only show
/// accurate line numbers for CSV entries.
#[derive(Debug, Clone)]
pub struct TranslationEntry {
    pub value: Arc<str>,
    pub template: Vec<TemplatePart>,
    pub source_path: Arc<Path>,
    pub line: usize,
}

impl TranslationEntry {
    pub fn new(value: String, source_path: Arc<Path>, line: usize) -> Self {
        let template = Self::parse_template(&value);
        Self {
            value: Arc::from(value),
            template,
            source_path,
            line,
        }
    }

    /// Parses a raw string into a template with text spans and arguments.
    /// Handles `{}` sequentially and `{0}`, `{1}` positionally.
    /// Supports escaped braces (`{{` and `}}`).
    ///
    /// **Limitations:**
    /// - Mixing sequential `{}` and positional `{0}` arguments in the same string
    ///   can lead to collisions, as the sequential index counter does not account
    ///   for explicit indices already used. A warning is emitted when this is detected.
    pub fn parse_template(raw: &str) -> Vec<TemplatePart> {
        let mut parts = Vec::new();
        let mut chars = raw.char_indices().peekable();
        let mut last_idx = 0;
        let mut seq_idx = 0;
        let mut has_sequential = false;
        let mut has_positional = false;

        while let Some((idx, ch)) = chars.next() {
            if ch == '{' {
                if let Some(&(next_idx, '{')) = chars.peek() {
                    // Escaped `{` -> `{{`
                    if idx > last_idx {
                        parts.push(TemplatePart::Text {
                            start: last_idx,
                            end: idx,
                        });
                    }
                    parts.push(TemplatePart::LiteralOpen);
                    chars.next(); // consume second '{'
                    last_idx = next_idx + 1;
                } else if let Some(&(next_idx, '}')) = chars.peek() {
                    // `{}` format
                    if idx > last_idx {
                        parts.push(TemplatePart::Text {
                            start: last_idx,
                            end: idx,
                        });
                    }
                    parts.push(TemplatePart::Arg(seq_idx));
                    has_sequential = true;
                    seq_idx += 1;
                    chars.next(); // consume '}'
                    last_idx = next_idx + 1;
                } else {
                    // Try to parse an index inside `{idx}`
                    let mut is_idx = false;
                    let mut end_idx = 0;

                    // Lookahead
                    let cloned = chars.clone();
                    for (i, c) in cloned {
                        if c == '}' {
                            end_idx = i;
                            is_idx = true;
                            break;
                        } else if !c.is_ascii_digit() {
                            break;
                        }
                    }

                    if is_idx {
                        let inner = &raw[idx + 1..end_idx];
                        if let Ok(num) = inner.parse::<usize>() {
                            if idx > last_idx {
                                parts.push(TemplatePart::Text {
                                    start: last_idx,
                                    end: idx,
                                });
                            }
                            parts.push(TemplatePart::Arg(num));
                            has_positional = true;
                            // advance main iterator past the closing '}'
                            while let Some(&(i, _)) = chars.peek() {
                                if i <= end_idx {
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            last_idx = end_idx + 1;
                        }
                    }
                }
            } else if ch == '}' {
                if let Some(&(next_idx, '}')) = chars.peek() {
                    // Escaped `}` -> `}}`
                    if idx > last_idx {
                        parts.push(TemplatePart::Text {
                            start: last_idx,
                            end: idx,
                        });
                    }
                    parts.push(TemplatePart::LiteralClose);
                    chars.next(); // consume second '}'
                    last_idx = next_idx + 1;
                }
            }
        }

        if last_idx < raw.len() {
            parts.push(TemplatePart::Text {
                start: last_idx,
                end: raw.len(),
            });
        }

        if has_sequential && has_positional {
            crate::v_warn!(
                "Template mixes sequential `{{}}` and positional `{{0}}` arguments: \"{}\". \
                 This may cause argument collisions.",
                raw
            );
        }

        parts
    }
}
