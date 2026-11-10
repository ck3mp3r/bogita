use crate::views::vault_form::{VaultForm, VaultFormAction};
use bogita_core::domain::Vault;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

fn confirmed_vault(form: &mut VaultForm) -> Vault {
    match form.handle_key(KeyCode::Enter) {
        VaultFormAction::Confirm(vault) => vault,
        VaultFormAction::Cancel => panic!("expected Confirm, got Cancel"),
        VaultFormAction::None => panic!("expected Confirm, got None"),
    }
}

// ── name field — modifier gating ──────────────────────────────────────────────

#[test]
fn ctrl_modified_char_is_ignored_in_name_field() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new(String::new());

    // -- Exec
    form.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    form.handle_key_with_modifiers(KeyCode::Char('r'), KeyModifiers::CONTROL);

    // -- Check
    assert_eq!(confirmed_vault(&mut form).name, "a");
}

#[test]
fn alt_modified_char_is_ignored_in_name_field() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new(String::new());

    // -- Exec
    form.handle_key_with_modifiers(KeyCode::Char('a'), KeyModifiers::NONE);
    form.handle_key_with_modifiers(KeyCode::Char('x'), KeyModifiers::ALT);

    // -- Check
    assert_eq!(confirmed_vault(&mut form).name, "a");
}

#[test]
fn shift_modified_char_is_inserted_in_name_field() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new(String::new());

    // -- Exec
    form.handle_key_with_modifiers(KeyCode::Char('A'), KeyModifiers::SHIFT);

    // -- Check
    assert_eq!(confirmed_vault(&mut form).name, "A");
}

#[test]
fn space_is_inserted_in_name_field() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new(String::new());

    // -- Exec
    form.handle_key(KeyCode::Char('a'));
    form.handle_key(KeyCode::Char(' '));
    form.handle_key(KeyCode::Char('b'));

    // -- Check
    assert_eq!(confirmed_vault(&mut form).name, "a b");
}

// ── name field — existing keys ────────────────────────────────────────────────

#[test]
fn backspace_removes_last_char() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new("ab".to_string());

    // -- Exec
    form.handle_key(KeyCode::Backspace);

    // -- Check
    assert_eq!(confirmed_vault(&mut form).name, "a");
}

#[test]
fn enter_with_empty_name_returns_none() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new(String::new());

    // -- Exec
    let action = form.handle_key(KeyCode::Enter);

    // -- Check
    assert!(matches!(action, VaultFormAction::None));
}

#[test]
fn esc_cancels_the_form() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new("Work".to_string());

    // -- Exec
    let action = form.handle_key(KeyCode::Esc);

    // -- Check
    assert!(matches!(action, VaultFormAction::Cancel));
}

// ── default-vault checkbox cell ───────────────────────────────────────────────

#[test]
fn tab_moves_focus_from_name_to_checkbox() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new("Work".to_string());
    assert!(form.is_name_focused());

    // -- Exec
    form.handle_key(KeyCode::Tab);

    // -- Check
    assert!(!form.is_name_focused());
    form.handle_key(KeyCode::Tab);
    assert!(form.is_name_focused());
}

#[test]
fn alt_t_flips_default_in_checkbox_cell() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new("Work".to_string());
    form.handle_key(KeyCode::Tab);

    // -- Exec
    form.handle_key_with_modifiers(KeyCode::Char('t'), KeyModifiers::ALT);

    // -- Check
    assert!(confirmed_vault(&mut form).is_default);
}

#[test]
fn space_is_inert_in_checkbox_cell() {
    // -- Setup & Fixtures
    let mut form = VaultForm::new("Work".to_string());
    form.handle_key(KeyCode::Tab);

    // -- Exec
    form.handle_key(KeyCode::Char(' '));

    // -- Check
    assert!(!confirmed_vault(&mut form).is_default);
}
