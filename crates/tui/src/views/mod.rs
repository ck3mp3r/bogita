use ratatui::crossterm::event::KeyModifiers;

pub mod entry_form;
pub mod main_view;
pub mod password_gen_view;
pub mod vault_form;

/// `true` when a printable char with these modifiers may insert text.
///
/// SHIFT still inserts: terminals report a shifted printable char with SHIFT
/// set. Ctrl, Alt, Super, and Meta chords are app or widget commands, so they
/// must never reach an input as characters.
pub(crate) fn inserts_text(modifiers: KeyModifiers) -> bool {
    modifiers.difference(KeyModifiers::SHIFT).is_empty()
}

/// Normalize a bracketed-paste payload: `\r\n` and lone `\r` become `\n`.
///
/// A terminal may send either line ending; storage and the widgets expect LF.
pub(crate) fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Flatten a bracketed-paste payload for a single-line input.
///
/// Every line break (`\r\n`, `\r`, `\n`) becomes one space, so a pasted
/// multi-line block never silently collapses into its first line.
pub(crate) fn paste_to_single_line(text: &str) -> String {
    normalize_paste(text).replace('\n', " ")
}

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/views/paste.rs"]
mod paste_test;
// endregion: --- Tests
