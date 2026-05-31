
# Vernacular

A dead-simple localization crate for Rust game development.

---

## Example Usage

First, organize your localization strings into domain-specific RON files within locale directories:

**`assets/loc/en_US/ui.ron`**
```ron
{
	"ui.main_menu.start_game": "Start Game",
}
```

**`assets/loc/en_US/dialogue.ron`**
```ron
{
	"dialogue.greetings": "Hello there, {}!",
}
```

**`Equivalents in ja_JP`**
```ron
	"ui.main_menu.start_game": "ゲーム開始",
	//...
	"dialogue.greetings": "こんにちは、{}様！",

```

Then, use the `loc!` macro in your Rust code to fetch and format the strings based on the active locale:

```rust
use vernacular::{set_language, loc};

fn main() {
	let start_game_text = loc!("ui.main_menu.start_game");
	// -> "Start Game"

	let greetings_text = loc!("dialogue.greetings", "CoolPlayerName");
	// -> "Hello there, CoolPlayerName!"

	set_language("ja_JP");

	let start_game_text_ja = loc!("ui.main_menu.start_game");
	// -> "ゲーム開始"

	let greetings_text_ja = loc!("dialogue.greetings", "CoolPlayerName");
	// -> "こんにちは、CoolPlayerName様！"
}
```

---

## Roadmap

- Ron based Key-Value Lookup []
- Editor Friendly Codegen Key Enum []
- Fluent Support []

---

## License

Licensed under your choice of:

- MIT License
- Apache License, Version 2.0
- GPLv3
- LGPLv3
