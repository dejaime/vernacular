//! # Vernacular
//!
//! A dead-simple localization crate for Rust game development.
//!
//! ## Mental Model
//!
//! Vernacular loads translation strings from **CSV** and **RON** files organized
//! under a *content path* directory. The layout is:
//!
//! ```text
//! <content_path>/
//! ├── global.csv              # Root-level "unified" CSVs (all locales in columns)
//! ├── en_US/
//! │   ├── items.csv           # Per-locale CSVs (key,value pairs)
//! │   └── ui.ron              # Per-locale RON files (key-value maps)
//! └── ja_JP/
//!     ├── items.csv
//!     └── ui.ron
//! ```
//!
//! ## Load Order & Precedence
//!
//! 1. **Root CSVs** are loaded first (lazily, on first lookup). These are
//!    "unified" files with one column per locale.
//! 2. **Per-locale files** are loaded when a locale is first requested.
//!    Within a locale directory, **CSVs are processed before RON files**, so
//!    RON values deterministically overwrite CSV values for the same key.
//! 3. Within the same file type, files are processed in **alphabetical order**
//!    by filename, ensuring reproducible results across platforms.
//!
//! ## Fallback Behavior
//!
//! When a key is not found in the current locale, Vernacular automatically
//! falls back to `en_US` (or a configurable fallback locale). This exists
//! to survive missing or broken data (providing the reference English text),
//! not as a runtime content-selection feature. If the key is missing from
//! both, the raw key string is returned.
//!
//! ## Usage Styles
//!
//! **Global singleton** (simplest, good for most games):
//! ```rust,no_run
//! use vernacular::{set_locale, set_content_path, reload, loc};
//!
//! set_content_path("assets/loc");
//! set_locale("ja_JP");
//!
//! let text = loc!("ui.main_menu.start_game");
//! let greeting = loc!("dialogue.greetings", "Alice");
//!
//! // Hot-reload all translations from disk:
//! reload();
//! ```
//!
//! **Owned context** (for tests, multiple independent contexts, etc.):
//! ```rust,no_run
//! use vernacular::{VernacularContext, loc};
//!
//! let ctx = VernacularContext::new();
//! ctx.set_content_path("assets/loc");
//! ctx.set_locale("ja_JP");
//!
//! let text = loc!(ctx => "ui.main_menu.start_game");
//! let greeting = loc!(ctx => "dialogue.greetings", "Alice");
// ...
//! ```

#[cfg(not(any(feature = "csv", feature = "ron")))]
compile_error!("vernacular requires the `csv` and/or `ron` feature to be enabled");

#[doc(hidden)]
#[macro_export]
macro_rules! v_warn {
    ($($arg:tt)*) => {
        #[cfg(feature = "log")]
        log::warn!($($arg)*);
        #[cfg(not(feature = "log"))]
        eprintln!("[vernacular] warning: {}", format_args!($($arg)*));
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! v_err {
    ($($arg:tt)*) => {
        #[cfg(feature = "log")]
        log::error!($($arg)*);
        #[cfg(not(feature = "log"))]
        eprintln!("[vernacular] error: {}", format_args!($($arg)*));
    }
}

pub mod error;
pub mod model;
pub mod parsing;

#[cfg(feature = "codegen")]
pub mod codegen;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, OnceLock};

use model::{TranslationEntry, TemplatePart};

/// The default locale used when a key is missing from the active locale.
pub const FALLBACK_LOCALE: &str = "en_US";

#[inline]
fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

#[inline]
fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

#[inline]
fn mutex_lock<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|e| e.into_inner())
}

/// The core localization context that holds parsed dictionaries.
///
/// Each `VernacularContext` maintains its own content path, locale settings,
/// and translation cache. For most applications, the global singleton
/// (via free functions like [`set_locale`] and [`localize`]) is sufficient.
/// Create an explicit context when you need isolation (tests, editor tools, etc.).
///
/// This type is `Send + Sync` and can be shared across threads via `Arc<VernacularContext>`.
/// It is intentionally not `Clone`; wrap in [`Arc`] when shared ownership is needed.
#[derive(Debug)]
pub struct VernacularContext {
    current_locale: RwLock<Option<Arc<str>>>,
    fallback_locale: RwLock<Arc<str>>,
    content_path: RwLock<Arc<str>>,
    translations: RwLock<model::LocaleMap>,
    csvs_loaded: AtomicBool,
    locales_loaded: RwLock<std::collections::HashSet<String>>,
    load_lock: Mutex<()>,
}

/// The default content path is `"assets/loc"` (relative to the working directory).
/// No locale is set by default; the fallback locale (`en_US`) is used until
/// [`set_locale`](VernacularContext::set_locale) is called.
impl Default for VernacularContext {
    fn default() -> Self {
        Self {
            current_locale: RwLock::new(None),
            fallback_locale: RwLock::new(Arc::from(FALLBACK_LOCALE)),
            content_path: RwLock::new(Arc::from("assets/loc")),
            translations: RwLock::new(HashMap::new()),
            csvs_loaded: AtomicBool::new(false),
            locales_loaded: RwLock::new(std::collections::HashSet::new()),
            load_lock: Mutex::new(()),
        }
    }
}

impl VernacularContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the currently active locale (e.g. `"ja_JP"`).
    ///
    /// Returns `None` if no locale has been set yet.
    #[must_use]
    pub fn current_locale(&self) -> Option<Arc<str>> {
        read_lock(&self.current_locale).clone()
    }

    /// Returns the fallback locale used for missing-key lookups.
    #[must_use]
    pub fn fallback_locale(&self) -> Arc<str> {
        read_lock(&self.fallback_locale).clone()
    }

    /// Returns the current content path.
    #[must_use]
    pub fn content_path(&self) -> Arc<str> {
        read_lock(&self.content_path).clone()
    }

    /// Returns a sorted list of all discoverable locales by inspecting the content path.
    pub fn available_locales(&self) -> Result<Vec<String>, error::VernacularError> {
        let mut locales = std::collections::HashSet::new();
        let base_path = self.content_path();
        
        let entries = fs::read_dir(&*base_path)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') {
                        locales.insert(name.to_string());
                    }
                }
            }
        }
        
        let mut sorted: Vec<_> = locales.into_iter().collect();
        sorted.sort();
        Ok(sorted)
    }

    /// Sets the root directory where translation files are located.
    ///
    /// This invalidates all cached translations so that the next lookup
    /// re-reads from disk using the new path.
    pub fn set_content_path(&self, path: &str) {
        *write_lock(&self.content_path) = Arc::from(path);
        self.invalidate_cache();
    }

    /// Sets the fallback locale, used for per-key fallback when a key is
    /// missing from the current locale.
    ///
    /// The new fallback locale is **not** eagerly loaded; it will be loaded
    /// lazily on the next lookup that requires it.
    pub fn set_fallback_locale(&self, locale: &str) {
        *write_lock(&self.fallback_locale) = Arc::from(locale);
    }

    /// Sets the active locale and eagerly loads its translation files.
    pub fn set_locale(&self, locale: &str) {
        if locale.is_empty() {
            crate::v_warn!("set_locale(\"\") ignored. Empty strings are not valid locales.");
            return;
        }
        *write_lock(&self.current_locale) = Some(Arc::from(locale));
        self.load_locale(locale);
    }

    /// Forcefully clears all parsed dictionaries and re-reads everything from disk.
    ///
    /// If a locale is currently set, its files are re-loaded immediately.
    /// Useful for hot-reload workflows during development.
    pub fn reload(&self) {
        self.invalidate_cache();
        if let Some(locale) = self.current_locale() {
            self.load_locale(&locale);
        }
    }

    /// Strictly validates all loaded files and returns aggregated errors.
    ///
    /// Clears the cache, runs the internal load methods for CSVs and all discovered
    /// locales (via `available_locales()`), and aggregates the errors. It marks
    /// everything as loaded regardless of partial failures, to avoid lazy-loading
    /// retry spam.
    pub fn try_reload(&self) -> Result<(), error::AggregateError> {
        self.invalidate_cache();
        let mut errors = Vec::new();
        
        let _guard = mutex_lock(&self.load_lock);

        errors.extend(self.do_load_csvs());
        self.csvs_loaded.store(true, Ordering::Release);

        match self.available_locales() {
            Ok(locales) => {
                for locale in locales {
                    errors.extend(self.do_load_locale(&locale));
                    write_lock(&self.locales_loaded).insert(locale);
                }
            }
            Err(e) => {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(error::AggregateError(errors))
        }
    }

    /// Clears all cached load state so that the next lookup re-reads from disk.
    fn invalidate_cache(&self) {
        let _guard = mutex_lock(&self.load_lock);
        write_lock(&self.translations).clear();
        write_lock(&self.locales_loaded).clear();
        self.csvs_loaded.store(false, Ordering::Release);
    }

    fn ensure_csvs_are_loaded(&self) {
        if self.csvs_loaded.load(Ordering::Acquire) {
            return;
        }
        let _guard = mutex_lock(&self.load_lock);
        if self.csvs_loaded.load(Ordering::Acquire) {
            return;
        }

        let errors = self.do_load_csvs();
        for e in errors {
            crate::v_err!("{}", e);
        }

        self.csvs_loaded.store(true, Ordering::Release);
    }

    fn do_load_csvs(&self) -> Vec<error::VernacularError> {
        #[cfg(not(feature = "csv"))]
        { Vec::new() }

        #[cfg(feature = "csv")]
        {
            let mut errors = Vec::new();
            let base_path = self.content_path();

            match fs::read_dir(&*base_path) {
                Ok(entries) => {
                    // Collect and sort paths for deterministic load order.
                    let mut csv_paths: Vec<_> = entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("csv"))
                        .collect();
                    csv_paths.sort();

                    for path in csv_paths {
                        match parsing::csv::parse_unified(&path) {
                            Ok(data) => self.merge_data(data),
                            Err(e) => {
                                errors.push(error::VernacularError::Parse(Box::new(error::FileParseError {
                                    path: path.to_path_buf(),
                                    source: e,
                                })));
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(error::VernacularError::Io(e));
                }
            }

            errors
        }
    }

    fn load_locale(&self, locale: &str) {
        self.ensure_csvs_are_loaded();

        {
            let locales = read_lock(&self.locales_loaded);
            if locales.contains(locale) {
                return;
            }
        }

        let _guard = mutex_lock(&self.load_lock);

        {
            let locales = read_lock(&self.locales_loaded);
            if locales.contains(locale) {
                return;
            }
        }

        let errors = self.do_load_locale(locale);
        for e in errors {
            crate::v_err!("{}", e);
        }
        
        {
            let mut locales = write_lock(&self.locales_loaded);
            locales.insert(locale.to_string());
        }
    }

    fn do_load_locale(&self, locale: &str) -> Vec<error::VernacularError> {
        let mut errors = Vec::new();
        let base = self.content_path();
        let dir_path = Path::new(&*base).join(locale);
        match fs::read_dir(dir_path) {
            Ok(entries) => {
                // Collect all file paths once, then process by type.
                let all_files: Vec<_> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file())
                    .collect();

                // Process CSVs first (sorted) so RON files can overwrite them deterministically.
                #[cfg(feature = "csv")]
                {
                    let mut csv_paths: Vec<_> = all_files.iter()
                        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("csv"))
                        .collect();
                    csv_paths.sort();
                    for path in csv_paths {
                        match parsing::csv::parse_locale(path) {
                            Ok(data) => self.merge_locale_data(locale, data),
                            Err(e) => {
                                errors.push(error::VernacularError::Parse(Box::new(error::FileParseError {
                                    path: path.to_path_buf(),
                                    source: e,
                                })));
                            }
                        }
                    }
                }

                #[cfg(feature = "ron")]
                {
                    let mut ron_paths: Vec<_> = all_files.iter()
                        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ron"))
                        .collect();
                    ron_paths.sort();
                    for path in ron_paths {
                        match parsing::ron::parse(path) {
                            Ok(data) => self.merge_locale_data(locale, data),
                            Err(e) => {
                                errors.push(error::VernacularError::Parse(Box::new(error::FileParseError {
                                    path: path.to_path_buf(),
                                    source: e,
                                })));
                            }
                        }
                    }
                }

                let _ = &all_files;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Ignore missing locale directory
            }
            Err(e) => {
                errors.push(error::VernacularError::Io(e));
            }
        }

        errors
    }

    fn merge_locale_data(&self, locale: &str, new_entries: HashMap<String, TranslationEntry>) {
        let mut translations = write_lock(&self.translations);
        let locale_map = translations.entry(locale.to_string()).or_default();
        for (key, new_entry) in new_entries {
            if let Some(existing) = locale_map.get(&key) {
                crate::v_warn!(
                    "Duplicate key '{}' for locale '{}'.\n  -> Overwritten value from: {}:{}\n  ->              New value from: {}:{}",
                    key,
                    locale,
                    existing.source_path.display(),
                    existing.line,
                    new_entry.source_path.display(),
                    new_entry.line
                );
            }
            locale_map.insert(key, new_entry);
        }
    }

    #[cfg(feature = "csv")]
    fn merge_data(&self, data: model::LocaleMap) {
        for (locale, new_entries) in data {
            self.merge_locale_data(&locale, new_entries);
        }
    }

    /// Resolves the effective locale, ensures it is loaded, and looks up a translation entry.
    ///
    /// Encapsulates the locking and fallback logic, yielding the found entry to the closure.
    fn with_entry<R>(&self, key: &str, f: impl FnOnce(Option<&TranslationEntry>) -> R) -> R {
        let current = self.current_locale();
        let fallback = self.fallback_locale();
        
        let locale = current.unwrap_or_else(|| Arc::clone(&fallback));

        // 1. Ensure locales are loaded BEFORE locking translations to avoid deadlock
        {
            let locales = read_lock(&self.locales_loaded);
            let needs_locale = !locales.contains(&*locale);
            let needs_fallback = locale != fallback && !locales.contains(&*fallback);
            drop(locales);
            
            if needs_locale {
                self.load_locale(&locale);
            }
            if needs_fallback {
                self.load_locale(&fallback);
            }
        }

        // 2. Lock translations and perform lookup
        let translations = read_lock(&self.translations);
        
        // Try the active locale first.
        if let Some(entry) = translations.get(&*locale).and_then(|l| l.get(key)) {
            return f(Some(entry));
        }
        
        // Fall back to the fallback locale if it differs (#3).
        if locale != fallback {
            if let Some(entry) = translations.get(&*fallback).and_then(|l| l.get(key)) {
                return f(Some(entry));
            }
        }

        f(None)
    }

    /// Returns the localized string for `key`, or the raw key if not found.
    ///
    /// Falls back to the fallback locale if the key is missing from the
    /// current locale.
    ///
    /// Returns a cheaply cloneable `Arc<str>`. Note that [`localize_fmt`](Self::localize_fmt)
    /// returns a `String` instead; if you need a uniform return type, call
    /// `.to_string()` on the `Arc<str>` or use `localize_fmt` with an empty arg slice.
    #[must_use]
    pub fn localize(&self, key: &str) -> Arc<str> {
        self.with_entry(key, |entry| {
            if let Some(e) = entry {
                Arc::clone(&e.value)
            } else {
                Arc::from(key)
            }
        })
    }

    /// Returns the localized, formatted string for `key` with positional arguments,
    /// or the raw key if not found.
    ///
    /// Template placeholders like `{}` and `{0}` are replaced with the provided `args`.
    /// Falls back to the fallback locale if the key is missing from the current locale.
    ///
    /// Returns a newly allocated `String` (unlike [`localize`](Self::localize) which
    /// returns `Arc<str>`). This means the [`loc!`] macro's return type differs
    /// depending on whether arguments are provided.
    #[must_use]
    pub fn localize_fmt(&self, key: &str, args: &[&dyn std::fmt::Display]) -> String {
        self.with_entry(key, |entry| {
            if let Some(e) = entry {
                // Heuristic: template length + 16 chars per arg
                let cap = e.value.len() + (args.len() * 16);
                let mut result = String::with_capacity(cap);
                for part in &e.template {
                    match part {
                        TemplatePart::Text { start, end } => result.push_str(&e.value[*start..*end]),
                        TemplatePart::LiteralOpen => result.push('{'),
                        TemplatePart::LiteralClose => result.push('}'),
                        TemplatePart::Arg(idx) => {
                            if let Some(arg) = args.get(*idx) {
                                use std::fmt::Write;
                                let _ = write!(&mut result, "{}", arg);
                            } else {
                                result.push_str(&format!("{{{}}}", idx));
                            }
                        }
                    }
                }
                result
            } else {
                key.to_string()
            }
        })
    }

    /// Returns `true` if the given key exists in the current locale or the fallback locale.
    ///
    /// Performs the same lookup chain as [`localize`](Self::localize) without allocating
    /// a result string.
    #[must_use]
    pub fn has_key(&self, key: &str) -> bool {
        self.with_entry(key, |entry| entry.is_some())
    }
}

// Compile-time assertion that VernacularContext is Send + Sync.
const _: () = {
    fn _assert<T: Send + Sync>() {}
    fn _check() { _assert::<VernacularContext>(); }
};

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------
// NOTE: The global context is initialized once per process via `OnceLock`.
// It cannot be reset, so tests that use the global free functions
// (`set_locale`, `localize`, etc.) may interfere with each other.
// Prefer `VernacularContext::new()` in test code for isolation.

static GLOBAL_CONTEXT: OnceLock<VernacularContext> = OnceLock::new();

fn get_global_context() -> &'static VernacularContext {
    GLOBAL_CONTEXT.get_or_init(VernacularContext::new)
}

/// Sets the root directory where translation files are located (global context).
///
/// Invalidates all cached translations so that the next lookup re-reads
/// from disk using the new path.
pub fn set_content_path(path: &str) {
    get_global_context().set_content_path(path);
}

/// Sets the fallback locale for the global context.
pub fn set_fallback_locale(locale: &str) {
    get_global_context().set_fallback_locale(locale);
}

/// Sets the active locale for the global context and eagerly loads its files.
pub fn set_locale(locale: &str) {
    get_global_context().set_locale(locale);
}

/// Forcefully clears all cached translations and re-reads from disk (global context).
///
/// If a locale is currently set, its files are re-loaded immediately.
pub fn reload() {
    get_global_context().reload();
}

/// Returns a sorted list of all discoverable locales by inspecting the content path (global context).
pub fn available_locales() -> Result<Vec<String>, error::VernacularError> {
    get_global_context().available_locales()
}

/// Strictly validates all loaded files and returns aggregated errors (global context).
pub fn try_reload() -> Result<(), error::AggregateError> {
    get_global_context().try_reload()
}

#[doc(hidden)]
#[must_use]
pub fn localize(key: &str) -> Arc<str> {
    get_global_context().localize(key)
}

#[doc(hidden)]
#[must_use]
pub fn localize_fmt(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    get_global_context().localize_fmt(key, args)
}

/// Returns `true` if the given key exists (global context).
#[doc(hidden)]
#[must_use]
pub fn has_key(key: &str) -> bool {
    get_global_context().has_key(key)
}

/// Localization macro for looking up and formatting translated strings.
///
/// # Return Types
///
/// - Without arguments: returns `Arc<str>` (zero-copy from the translation cache).
/// - With arguments: returns `String` (newly allocated, with placeholders filled).
///
/// If you need a uniform type, call `.to_string()` on the `Arc<str>` result
/// or use [`VernacularContext::localize_fmt`] with an empty argument slice.
///
/// # Usage
///
/// **Simple key lookup (global context):**
/// ```rust,no_run
/// # use vernacular::loc;
/// let text = loc!("ui.main_menu.start_game");
/// ```
///
/// **Formatted key lookup with arguments (global context):**
/// ```rust,no_run
/// # use vernacular::loc;
/// # let player_name = "Alice";
/// let greeting = loc!("dialogue.greetings", player_name);
/// ```
///
/// **With an explicit [`VernacularContext`]:**
/// ```rust,no_run
/// # use vernacular::{VernacularContext, loc};
/// # let player_name = "Alice";
/// # let ctx = VernacularContext::new();
/// let text = loc!(ctx => "ui.main_menu.start_game");
/// let greeting = loc!(ctx => "dialogue.greetings", player_name);
/// ```
#[macro_export]
macro_rules! loc {
    ($ctx:expr => $key:expr) => {
        $ctx.localize($key.as_ref())
    };
    ($ctx:expr => $key:expr, $($arg:expr),+) => {
        $ctx.localize_fmt($key.as_ref(), &[ $( &$arg as &dyn std::fmt::Display ),+ ])
    };
    ($key:expr) => {
        $crate::localize($key.as_ref())
    };
    ($key:expr, $($arg:expr),+) => {
        $crate::localize_fmt($key.as_ref(), &[ $( &$arg as &dyn std::fmt::Display ),+ ])
    };
}

#[cfg(all(test, feature = "csv", feature = "ron"))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_localization_flow_owned() {
        let ctx = VernacularContext::new();
        ctx.set_content_path("samples");

        // 1. Fallback to en_US and RON loading
        assert_eq!(&*loc!(ctx => "ui.main_menu.start_game"), "Start Game");

        // 2. CSV loading
        assert_eq!(&*loc!(ctx => "items.sword"), "Iron Sword");

        // 3. Argument replacement
        assert_eq!(loc!(ctx => "dialogue.greetings", "Alice"), "Hello there, Alice!");

        // 4. Change locale
        ctx.set_locale("ja_JP");

        // 5. Test new locale
        assert_eq!(&*loc!(ctx => "ui.main_menu.start_game"), "ゲーム開始");
        assert_eq!(&*loc!(ctx => "items.sword"), "鉄の剣");
        assert_eq!(loc!(ctx => "dialogue.greetings", "Bob"), "こんにちは、Bob様！");
        
        // 6. Test reload drops cache and reloads
        ctx.reload();
        assert_eq!(&*loc!(ctx => "ui.main_menu.start_game"), "ゲーム開始");
    }

    #[test]
    fn test_ron_overwrites_csv_owned() {
        let ctx = VernacularContext::new();

        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();

        fs::write(dir.path().join("overrides.csv"), "locale,en_US\nitems.sword,\"Sword from CSV\"").unwrap();
        fs::write(en_path.join("overrides.ron"), "{\"items.sword\": \"Sword from RON\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        assert_eq!(&*loc!(ctx => "items.sword"), "Sword from RON", "RON value should overwrite the CSV value.");
    }

    // #1: Stale-state invalidation test
    #[test]
    fn test_set_content_path_invalidates_cache() {
        let ctx = VernacularContext::new();

        // Start with a bad path — lookup will try to load and fail silently.
        ctx.set_content_path("/nonexistent/bad/path");
        assert_eq!(&*loc!(ctx => "ui.main_menu.start_game"), "ui.main_menu.start_game",
            "Should return raw key for missing content path.");

        // Now set the correct path — cache should be invalidated and re-loaded.
        ctx.set_content_path("samples");
        assert_eq!(&*loc!(ctx => "ui.main_menu.start_game"), "Start Game",
            "Should return translated value after fixing the content path.");
    }


    // #2: Deterministic file load order
    #[test]
    fn test_deterministic_file_load_order() {
        let ctx = VernacularContext::new();

        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();

        // Create two RON files with the same key. Alphabetically, b.ron comes after a.ron,
        // so b.ron's value should win deterministically.
        fs::write(en_path.join("a.ron"), "{\"greeting\": \"from A\"}").unwrap();
        fs::write(en_path.join("b.ron"), "{\"greeting\": \"from B\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        assert_eq!(&*loc!(ctx => "greeting"), "from B",
            "Alphabetically later file should overwrite earlier one.");
    }

    // #3: Per-key fallback
    #[test]
    fn test_per_key_fallback() {
        let ctx = VernacularContext::new();

        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        let ja_path = dir.path().join("ja_JP");
        fs::create_dir(&en_path).unwrap();
        fs::create_dir(&ja_path).unwrap();

        // en_US has both keys; ja_JP only has one.
        fs::write(en_path.join("ui.ron"), "{\"greeting\": \"Hello\", \"farewell\": \"Goodbye\"}").unwrap();
        fs::write(ja_path.join("ui.ron"), "{\"greeting\": \"こんにちは\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        ctx.set_locale("ja_JP");

        // Key present in ja_JP — should use ja_JP value.
        assert_eq!(&*loc!(ctx => "greeting"), "こんにちは");

        // Key missing from ja_JP — should fall back to en_US.
        assert_eq!(&*loc!(ctx => "farewell"), "Goodbye",
            "Missing key should fall back to fallback locale.");

        // Key missing from both — should return raw key.
        assert_eq!(&*loc!(ctx => "nonexistent"), "nonexistent");
    }

    // #3: Per-key fallback with localize_fmt
    #[test]
    fn test_per_key_fallback_fmt() {
        let ctx = VernacularContext::new();
        
        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();
        fs::write(en_path.join("ui.ron"), "{\"greeting\": \"Hello {0}\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        
        assert_eq!(loc!(ctx => "greeting", "Bob"), "Hello Bob");
        assert_eq!(loc!(ctx => "missing", "Bob"), "missing");
    }

    #[test]
    fn test_set_locale_empty() {
        let ctx = VernacularContext::new();
        ctx.set_locale("ja_JP");
        assert_eq!(ctx.current_locale().as_deref(), Some("ja_JP"));

        // Setting empty string should be ignored
        ctx.set_locale("");
        assert_eq!(ctx.current_locale().as_deref(), Some("ja_JP"));
    }

    #[test]
    fn test_error_chain_preserved() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        
        // Malformed RON file
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();
        let bad_ron_path = en_path.join("bad.ron");
        fs::write(&bad_ron_path, "{ \"missing_colon\" \"value\" }").unwrap();
        
        ctx.set_content_path(dir.path().to_str().unwrap());
        
        let errs = ctx.try_reload().unwrap_err();
        assert_eq!(errs.len(), 1);
        
        let outer_err = &errs.errors()[0];
        // Outer error should be Parse
        assert!(matches!(outer_err, crate::error::VernacularError::Parse(_)));
        
        // Walk the chain down to FileParseError
        use std::error::Error;
        let file_err = outer_err.source().and_then(|e| e.downcast_ref::<crate::error::FileParseError>()).unwrap();
        assert_eq!(file_err.path(), bad_ron_path.as_path());
        
        // The file error's source should be the inner parse error (e.g. csv::Error)
        assert!(file_err.source().is_some());
    }



    // New tests from round 3

    #[test]
    fn test_malformed_csv_does_not_panic() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        
        // Write a malformed CSV with an empty row
        fs::write(dir.path().join("global.csv"), "locale,en_US\n\nkey,value").unwrap();
        
        ctx.set_content_path(dir.path().to_str().unwrap());
        ctx.set_locale("en_US");
        
        // Just verify it doesn't panic when trying to read
        assert_eq!(&*loc!(ctx => "key"), "value");
    }
    
    #[test]
    fn test_try_reload_validates_and_aggregates_errors() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();
        
        // Valid file
        fs::write(en_path.join("valid.ron"), "{\"good\": \"yes\"}").unwrap();
        // Malformed file in en_US
        fs::write(en_path.join("bad.ron"), "{\"bad\": oops").unwrap();
        
        let ja_path = dir.path().join("ja_JP");
        fs::create_dir(&ja_path).unwrap();
        // Malformed file in ja_JP
        fs::write(ja_path.join("bad.ron"), "{ invalid }").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        
        let errors = ctx.try_reload().unwrap_err();
        assert_eq!(errors.len(), 2, "Should aggregate 2 errors from 2 broken files");
        
        // Ensure valid locale data still populated successfully
        assert_eq!(&*loc!(ctx => "good"), "yes", "Valid data should still be loaded");
    }

    #[test]
    fn test_set_fallback_locale_lazy_loads_mid_session() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        let es_path = dir.path().join("es_ES");
        fs::create_dir(&en_path).unwrap();
        fs::create_dir(&es_path).unwrap();

        fs::write(en_path.join("ui.ron"), "{\"greet\": \"Hello\"}").unwrap();
        fs::write(es_path.join("ui.ron"), "{\"greet\": \"Hola\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        ctx.set_locale("ja_JP");

        assert_eq!(&*loc!(ctx => "greet"), "Hello", "Defaults to en_US fallback");
        
        ctx.set_fallback_locale("es_ES");
        assert_eq!(&*loc!(ctx => "greet"), "Hola", "Switches to es_ES fallback without manual reload");
    }

    #[test]
    fn test_available_locales() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        
        fs::create_dir(dir.path().join("zz_ZZ")).unwrap();
        fs::create_dir(dir.path().join("aa_AA")).unwrap();
        fs::create_dir(dir.path().join("en_US")).unwrap();
        // Files shouldn't be counted
        fs::write(dir.path().join("global.csv"), "").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        let locales = ctx.available_locales().unwrap();
        
        assert_eq!(locales, vec!["aa_AA", "en_US", "zz_ZZ"], "Should discover sorted valid directories only");
    }

    #[test]
    fn test_escaped_braces() {
        let ctx = VernacularContext::new();
        let dir = tempfile::tempdir().unwrap();
        let en_path = dir.path().join("en_US");
        fs::create_dir(&en_path).unwrap();

        fs::write(en_path.join("ui.ron"), "{\"json\": \"{{\\\"key\\\": \\\"value\\\"}}\", \"mixed\": \"{} {{escaped}} {}\"}").unwrap();

        ctx.set_content_path(dir.path().to_str().unwrap());
        ctx.set_locale("en_US");

        assert_eq!(loc!(ctx => "mixed", "A", "B"), "A {escaped} B", "Mixed escaped and normal arguments should work");
    }

    #[test]
    fn test_concurrent_access_and_reloading() {
        use std::sync::Arc;
        use std::thread;

        let ctx = Arc::new(VernacularContext::new());
        ctx.set_content_path("assets/loc");
        ctx.set_locale("en_US");

        let mut handles = vec![];

        for _ in 0..10 {
            let ctx = Arc::clone(&ctx);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let val = ctx.localize("ui.main_menu.start_game");
                    assert!(!val.is_empty());
                }
            }));
        }

        for i in 0..5 {
            let ctx = Arc::clone(&ctx);
            handles.push(thread::spawn(move || {
                for _ in 0..20 {
                    if i % 2 == 0 {
                        ctx.set_locale("ja_JP");
                    } else {
                        ctx.set_locale("en_US");
                    }
                    ctx.reload();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_arc_memory_release() {
        use std::sync::Arc;

        let val = {
            let ctx = VernacularContext::new();
            ctx.set_content_path("assets/loc");
            ctx.set_locale("en_US");
            let val = ctx.localize("ui.main_menu.start_game");
            assert!(Arc::strong_count(&val) > 1);
            val
        };

        assert_eq!(Arc::strong_count(&val), 1);
    }

    #[test]
    fn test_unicode_keys_and_templates() {
        let ctx = VernacularContext::new();
        ctx.set_content_path("assets/loc");
        ctx.set_locale("unicode");

        assert_eq!(&*ctx.localize("ui.🌟.start"), "Start 🌟 Game");
        assert_eq!(&*ctx.localize("ui.arabic"), "مرحبا");
        assert_eq!(&*ctx.localize("ui.cjk"), "你好，世界");

        assert_eq!(
            &*ctx.localize_fmt("ui.template.unicode", &[&"Alice", &"5"]),
            "Hello Alice, you have 5 💖"
        );
    }
}