//!
//! Alt-x on a form slot: select everything, cut it into the widget's own
//! buffer, and leave the field empty — restorable with Ctrl-v.

use crate::views::entry_form::EntryForm;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

// ── helpers ───────────────────────────────────────────────────────────────────

/// A form with one text row whose value slot holds `value`, focus on that slot.
fn form_on_value(value: &str) -> EntryForm {
    let mut form = add_form_named("Cut");
    add_field(&mut form);
    for c in "api_key".chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form.handle_key(KeyCode::Tab); // key → type slot
    form.handle_key(KeyCode::Tab); // type → value slot
    for c in value.chars() {
        form.handle_key(KeyCode::Char(c));
    }
    form
}

fn alt_x(form: &mut EntryForm) {
    form.handle_key_with_modifiers(KeyCode::Char('x'), KeyModifiers::ALT);
}

fn ctrl_v(form: &mut EntryForm) {
    form.handle_key_with_modifiers(KeyCode::Char('v'), KeyModifiers::CONTROL);
}

// ── Alt-x ────────────────────────────────────────────────────────────────────

#[test]
fn test_altx_cut_and_paste_restore() {
    // -- Setup & Fixtures
    let mut form = form_on_value("hello");

    // -- Exec: cut the whole slot
    alt_x(&mut form);

    // -- Check: the field is empty
    assert_eq!(
        form.fields[0].value_state.text(),
        "",
        "Alt-x must empty the focused value slot"
    );

    // -- Exec: bring it back from the widget's own buffer
    ctrl_v(&mut form);

    // -- Check
    assert_eq!(
        form.fields[0].value_state.text(),
        "hello",
        "Ctrl-v must restore what Alt-x cut"
    );
}

#[test]
fn test_altx_empty_slot_is_a_noop() {
    // -- Setup & Fixtures: an empty value slot
    let mut form = form_on_value("");

    // -- Exec
    alt_x(&mut form);

    // -- Check: nothing to cut, nothing breaks
    assert_eq!(form.fields[0].value_state.text(), "");
    assert_eq!(form.field_count(), 1);
}

#[test]
fn test_altx_on_the_type_slot_does_nothing() {
    // -- Setup & Fixtures: focus the type slot of a row that holds a value
    let mut form = form_on_value("hello");
    form.handle_key(KeyCode::Tab); // value → next row key (wrap) — go back instead
    form.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::SHIFT); // → value
    form.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::SHIFT); // → type

    // -- Exec
    alt_x(&mut form);

    // -- Check: the type slot is not a text cell
    assert_eq!(
        form.fields[0].value_state.text(),
        "hello",
        "Alt-x must not touch the row from the type slot"
    );
    assert_eq!(form.field_badge(0), "text");
}

#[test]
fn test_altx_cuts_the_key_slot_and_the_name_slot() {
    // -- Setup & Fixtures
    let mut form = add_form_named("Keep me");
    add_field(&mut form);
    for c in "api_key".chars() {
        form.handle_key(KeyCode::Char(c));
    }

    // -- Exec: the key slot is focused after typing
    alt_x(&mut form);

    // -- Check
    assert_eq!(
        form.fields[0].key_state.text(),
        "",
        "Alt-x must empty the focused key slot"
    );

    // -- Exec: back to the name slot (key → name) and cut that too
    form.handle_key_with_modifiers(KeyCode::BackTab, KeyModifiers::SHIFT);
    alt_x(&mut form);

    // -- Check
    assert_eq!(
        form.name_state.text(),
        "",
        "Alt-x must empty the focused name slot"
    );
}
