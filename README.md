
# Vernacular

[![CI](https://github.com/dejaime/vernacular/actions/workflows/rust.yml/badge.svg)](https://github.com/dejaime/vernacular/actions/workflows/rust.yml)
[![Crates.io](https://img.shields.io/crates/v/vernacular.svg)](https://crates.io/crates/vernacular)

A dead-simple localization crate for Rust game development.

---

## Features

- **CSV and RON** translation files — use whichever fits your workflow.
- **Fixed fallback** — missing keys in the active locale automatically fall back to `en_US` to survive missing or broken data.
- **Global singleton or owned contexts** — a one-liner API for games, and an explicit `VernacularContext` for tests/tools.
- **Deterministic load order** — files are processed alphabetically; RON overwrites CSV for predictable precedence.
- **Hot-reload** — call `reload()` to re-read all translation files from disk.

---

## Directory Layout

Organize your translation files under a *content path* directory (default: `assets/loc`):

```text
assets/loc/
├── items.csv               # Root-level "unified" CSV (all locales in columns)
├── en_US/
│   ├── items.csv           # Per-locale CSV (key,value pairs)
│   └── ui.ron              # Per-locale RON (key-value map)
└── ja_JP/
    ├── items.csv
    └── ui.ron
```

**Root CSVs** use a header row to name the locale columns:

```csv
locale,en_US,ja_JP
items.sword,Iron Sword,鉄の剣
items.shield,Wooden Shield,木の盾
```

**Per-locale RON files** are simple key-value maps:

```ron
{
    "ui.main_menu.start_game": "Start Game",
    "dialogue.greetings": "Hello there, {}!",
}
```

**Per-locale CSVs** are two-column `key,value` files (no header):

```csv
items.sword,Iron Sword
items.shield,Wooden Shield
```

> [!NOTE]
> If you need to intentionally define an empty string translation (e.g., to hide a label in a specific locale), use a single space `" "` in your CSV, or use a RON file which natively supports empty strings `""`. Completely empty CSV cells are treated as missing data, and will cause Vernacular to fall back to the fallback locale.

---

## Example Usage

You can import everything needed via `vernacular::prelude::*`.

### Global Singleton (simplest)

```rust,no_run
use vernacular::prelude::*;

fn main() {
    set_content_path("assets/loc");

    let start_game_text = loc!("ui.main_menu.start_game");
    // -> "Start Game"

    let greetings_text = loc!("dialogue.greetings", "CoolPlayerName");
    // -> "Hello there, CoolPlayerName!"

    set_locale("ja_JP");

    let start_game_text_ja = loc!("ui.main_menu.start_game");
    // -> "ゲーム開始"

    let greetings_text_ja = loc!("dialogue.greetings", "CoolPlayerName");
    // -> "こんにちは、CoolPlayerName様！"

    // Hot-reload translations from disk:
    reload();
}
```

### Owned Context

Use `VernacularContext` when you need multiple independent translation sets
(e.g. for tests, editor tools, or mod systems):

```rust,no_run
use vernacular::prelude::*;

let ctx = VernacularContext::new();
ctx.set_content_path("assets/loc");
ctx.set_locale("ja_JP");

let text = loc!(ctx => "ui.main_menu.start_game");
let greeting = loc!(ctx => "dialogue.greetings", "Alice");
```

---

## Multiple Content Paths

You can configure Vernacular to load from multiple directories. This is useful for modding support, DLCs, or separating base game assets from engine/library assets.

- `set_content_path(path)`: Clears all registered paths and sets the primary content path.
- `add_content_path(path)`: Registers an additional content path to load from.

When resolving keys, files are loaded in path-registration order, then format order. For example:

```rust
add_content_path("base_assets");
add_content_path("mod_assets");
```

This will load files in the following sequence:
1. `base_assets` CSV files
2. `base_assets` RON files
3. `mod_assets` CSV files
4. `mod_assets` RON files

As a result, subsequent paths will overwrite previous ones, allowing you to easily override base game translations.

---

## Editor-Friendly Codegen

You can generate a strongly-typed `LocKey` enum in your `build.rs` to eliminate typos and enable IDE autocomplete for your localization keys. See the [Codegen Guide](CODEGEN.md) for detailed configuration options.

1. Add `vernacular` to your `[build-dependencies]` with the `codegen` feature enabled:
   ```toml
    [build-dependencies]
    vernacular = { version = "0.3", features = ["codegen"] }
   ```

2. Create a `build.rs` in your project root:
   ```rust
   fn main() {
       let out_dir = std::env::var("OUT_DIR").unwrap();
       // Tell Cargo to re-run this script if the translation files change
       println!("cargo:rerun-if-changed=assets/loc"); 
       vernacular::codegen::generate_keys("assets/loc", &out_dir);
   }
   ```

3. Include the generated file anywhere in your `src/` code:
   ```rust
   include!(concat!(env!("OUT_DIR"), "/vernacular_keys.rs"));
   
   // Now you can use autocomplete!
   // let text = loc!(LocKey::UiMainMenuStartGame);
   ```

---

## Optional Features

Vernacular embraces the "pay for what you use" philosophy. By default, both `csv` and `ron` parsers are enabled. If you only want to use one format, you can disable default features to reduce compile times and binary size:

```toml
[dependencies]
vernacular = { version = "0.3", default-features = false, features = ["csv"] }
```

Available features:
- `csv`: Enables the CSV parser
- `ron`: Enables the RON parser (and brings in `serde`)
- `log`: Routes warnings and errors through the standard `log` crate
- `codegen`: Exposes the `vernacular::codegen` module for generating `LocKey` enums

---

## Roadmap

- [x] RON and CSV based Key-Value Lookup
- [x] Editor Friendly Codegen Key Enum
- [ ] Fluent (.ftl) Support

---

## License

Licensed under your choice of:

- MIT License
- Apache License, Version 2.0
- GNU General Public License, Version 3.0 (GPL-3.0)
- GNU Lesser General Public License, Version 3.0 (LGPL-3.0)
