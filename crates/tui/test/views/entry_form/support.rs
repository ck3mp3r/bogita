use crate::views::entry_form::EntryForm;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

pub(super) fn add_form() -> EntryForm {
    EntryForm::new_add(None)
}

pub(super) fn add_form_named(name: &str) -> EntryForm {
    EntryForm::new_add(Some(name.to_string()))
}

/// Press the Alt-n management chord (append a field row).
pub(super) fn add_field(form: &mut EntryForm) {
    form.handle_key_with_modifiers(KeyCode::Char('n'), KeyModifiers::ALT);
}

/// Press the Alt-d management chord (remove the focused field row).
pub(super) fn remove_field(form: &mut EntryForm) {
    form.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::ALT);
}

/// Press Enter on the type slot to open the dropdown.
pub(super) fn open_dropdown(form: &mut EntryForm) {
    form.handle_key(KeyCode::Enter);
}

/// Press the Alt-t management chord (flip a boolean value cell).
pub(super) fn flip_boolean(form: &mut EntryForm) {
    form.handle_key_with_modifiers(KeyCode::Char('t'), KeyModifiers::ALT);
}

/// Create an EntryForm with a single Token field (obscured), focus on the type slot.
pub(super) fn form_with_token_field() -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    add_field(&mut form); // add field, focus on key
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select Token (index 2)
    open_dropdown(&mut form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::Tab); // → value slot
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::BackTab); // back to the type slot
    form
}

pub(super) fn navigate_to_token_value_slot(form: &mut EntryForm) {
    add_field(form); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::Tab); // → value slot (now "token")
}

/// Create an EntryForm with `count` fields by pressing Alt-n `count` times.
pub(super) fn form_with_fields(count: usize) -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    for _ in 0..count {
        add_field(&mut form);
    }
    form
}
