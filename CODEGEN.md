# Vernacular Codegen

Vernacular includes an optional `codegen` feature that allows you to generate a strongly-typed `LocKey` enum from your localization files at build time. 

By leveraging Cargo's `build.rs` pipeline, Vernacular can scrape your asset directories before your Rust code compiles and generate a type-safe enum containing every single localization key found in your project.

## Why Use Codegen?

1. **IDE Autocomplete:** Your IDE will suggest valid localization keys as you type, significantly speeding up development and reducing cognitive load.
2. **Compile-Time Safety:** If you typo a key, or if a key is removed from your CSV/RON files, your Rust code will fail to compile. No more missing translations slipping into production!
3. **No Overhead:** The generated keys implement `AsRef<str>`, meaning the `loc!` macro seamlessly handles them with zero runtime overhead compared to raw strings.

---

## Setup Guide

### 1. Enable the Feature
First, add Vernacular to your `[build-dependencies]` in `Cargo.toml` with the `codegen` feature enabled. You must also keep Vernacular in your normal `[dependencies]`.

```toml
[dependencies]
vernacular = "0.2"

[build-dependencies]
vernacular = { version = "0.2", features = ["codegen"] }
```

### 2. Create your Build Script
Create a `build.rs` file at the root of your project (next to `Cargo.toml`). This script runs before your crate is compiled.

```rust
// build.rs
fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    
    // Tell Cargo to re-run this script only if files in `assets/loc` change
    println!("cargo:rerun-if-changed=assets/loc"); 
    
    // Generate the LocKey enum and save it to the build output directory
    vernacular::codegen::generate_keys("assets/loc", &out_dir);
}
```

### 3. Include the Generated Code
In your source code (usually inside `src/lib.rs` or `src/main.rs`, or a dedicated `src/loc.rs` module), include the generated file:

```rust
// src/loc.rs (or anywhere in your src tree)

// This pulls the generated enum into your codebase
include!(concat!(env!("OUT_DIR"), "/vernacular_keys.rs"));
```

---

## Usage

Once you've included the generated file, the `LocKey` enum is immediately available to use with the `loc!` macro.

```rust
use vernacular::loc;

// Assuming you have a key "ui.main_menu.start_game" in your CSV/RON files
let text = loc!(LocKey::UiMainMenuStartGame);

// It also works with format arguments!
// Assuming you have "dialogue.greeting": "Hello, {}!"
let greeting = loc!(LocKey::DialogueGreeting, "Alice");
```

> [!NOTE]
> The `loc!` macro still supports raw string literals (`loc!("ui.main_menu")`), making it fully backwards compatible. You can mix and match strings and `LocKey` variants as needed.

---

## How Keys Are Converted

Vernacular automatically converts your raw localization keys into Rust-idiomatic PascalCase variants for the `LocKey` enum.

The converter is highly robust and handles several edge cases seamlessly:

- **Standard Keys:** `ui.main_menu.start_game` ➔ `UiMainMenuStartGame`
- **Dash / Underscore separated:** `dialogue-greeting` ➔ `DialogueGreeting`
- **Numbers:** `some_key_with_123` ➔ `SomeKeyWith123`
- **Keys starting with numbers:** `1st_place` ➔ `Key1stPlace` (Since Rust variants cannot start with a number, Vernacular prepends `Key`)
- **Empty / Pure Symbols:** `...` ➔ `Empty` (If a key contains no alphanumeric characters, it defaults to `Empty`)

### Duplicate Keys
If the same key is discovered across multiple files, Vernacular deduplicates them automatically using a `HashSet`. The final list of keys is then sorted alphabetically, ensuring your generated `LocKey` enum (and git diffs, if checked in) remain fully deterministic.

## Error Handling

When `generate_keys` runs, it gracefully iterates over all `.csv` and `.ron` files in the specified directory. 
If a file is malformed and fails to parse, Vernacular will **not** halt the build. Instead, it will emit a standard Cargo warning (which will appear yellow in your terminal), allowing the build to continue while clearly notifying you of the broken asset.

```
warning: Failed to scan keys from assets/loc/broken.csv: CSV parse error
```
