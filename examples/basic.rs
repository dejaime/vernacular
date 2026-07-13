use vernacular::prelude::*;

fn main() {
    // Point to the sample data bundled with the crate.
    set_content_path("assets/loc");

    // With no explicit locale set, the fallback locale is used.
    let start_game_text = loc!("ui.main_menu.start_game");
    println!("Start Game: {}", start_game_text);

    let greetings_text = loc!("dialogue.greetings", "CoolPlayerName");
    println!("Greetings: {}", greetings_text);

    // Switch to Japanese.
    set_locale("ja_JP");

    let start_game_text_ja = loc!("ui.main_menu.start_game");
    println!("Start Game (JA): {}", start_game_text_ja);

    let greetings_text_ja = loc!("dialogue.greetings", "CoolPlayerName");
    println!("Greetings (JA): {}", greetings_text_ja);

    // Hot-reload all translations from disk (useful during development).
    reload();
    println!("After reload: {}", loc!("ui.main_menu.start_game"));
}
