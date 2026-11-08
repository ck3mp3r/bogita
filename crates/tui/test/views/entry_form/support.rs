use crate::views::entry_form::EntryForm;
use ratatui::crossterm::event::KeyCode;

pub(super) fn add_form() -> EntryForm {
    EntryForm::new_add(None)
}

pub(super) fn add_form_named(name: &str) -> EntryForm {
    EntryForm::new_add(Some(name.to_string()))
}

pub(super) fn open_dropdown(form: &mut EntryForm) {
    form.handle_key(KeyCode::Char(' ')); // open dropdown
}

/// Create an EntryForm with a single Token field (obscured).
pub(super) fn form_with_token_field() -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    form.handle_key(KeyCode::Char('+')); // add field, focus on key
    for c in "password".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → value
    for c in "s3cret".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // → type slot
                                   // Open dropdown and select Token (index 2)
    form.handle_key(KeyCode::Char(' ')); // open dropdown
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form
}

pub(super) fn navigate_to_token_value_slot(form: &mut EntryForm) {
    form.handle_key(KeyCode::Char('+')); // add field, focus → key slot
    form.handle_key(KeyCode::Tab); // → value (plain)
    form.handle_key(KeyCode::Tab); // → type slot
    open_dropdown(form);
    form.handle_key(KeyCode::Char('j')); // Username
    form.handle_key(KeyCode::Char('j')); // Token
    form.handle_key(KeyCode::Enter); // confirm selection
    form.handle_key(KeyCode::BackTab); // back to value slot (now "token")
}

/// Create an EntryForm with `count` fields by repeatedly pressing '+'.
pub(super) fn form_with_fields(count: usize) -> EntryForm {
    let mut form = EntryForm::new_add(Some("Test".to_string()));
    for _ in 0..count {
        form.handle_key(KeyCode::Char('+'));
    }
    form
}
