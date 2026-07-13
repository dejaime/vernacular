//! The Vernacular Prelude.
//!
//! Re-exports the most commonly used types, macros, and functions for convenience.
//!
//! ```rust,no_run
//! use vernacular::prelude::*;
//!
//! set_content_path("assets/loc");
//! set_locale("en_US");
//! let text = loc!("ui.welcome");
//! ```

pub use crate::{
    add_content_path, available_locales, loc, localize, reload, set_content_path,
    set_fallback_locale, set_locale, try_reload, VernacularContext,
};
