//!
//! Vim-grade navigation: h/l pane focus, gg/G list bounds, Ctrl-d/u half page.
//!
//! The chords are additive — j/k, Tab and the leader keep their grammar, and every
//! new chord is inert while the search box or an overlay owns the keys.

use crate::views::main_view::{Column, MainView, MainViewAction};
use bogita_core::domain::{FieldType, FieldValue};
use bogita_core::test_support::{test_entry, test_field};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::make_vault;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// A view with `count` entries in one vault, selection on the first.
fn view_with(count: usize) -> MainView {
    let vault = make_vault("Work");
    let entries: Vec<_> = (0..count)
        .map(|i| test_entry(vault.id, &format!("entry-{i}"), vec![]))
        .collect();
    MainView::new(vec![vault], entries)
}

/// A view with one entry holding `field_count` field rows.
fn view_with_fields(field_count: usize) -> MainView {
    let vault = make_vault("Work");
    let fields: Vec<_> = (0..field_count)
        .map(|i| {
            test_field(
                &format!("field-{i}"),
                FieldValue::Text(format!("value-{i}")),
                FieldType::Custom(format!("field-{i}")),
                false,
                i as i32,
            )
        })
        .collect();
    let entries = vec![test_entry(vault.id, "entry-0", fields)];
    MainView::new(vec![vault], entries)
}

/// The name of the selected entry, if any.
fn selected(view: &MainView) -> Option<String> {
    view.selected_entry().map(|e| e.name.clone())
}

// ── h/l pane focus ────────────────────────────────────────────────────────────

#[test]
fn test_vim_hl_pane_focus() {
    // -- Setup & Fixtures
    let mut view = view_with(1);
    assert_eq!(view.focused, Column::Entries);

    // -- Check: l reaches the detail pane the same way Tab does
    let action = view.handle_key(KeyCode::Char('l'));
    assert_eq!(view.focused, Column::Detail);
    assert!(
        matches!(action, MainViewAction::SelectEntry { .. }),
        "reaching the detail pane must ask for the entry, got {action:?}"
    );

    // -- Check: h comes back to the entries pane
    view.handle_key(KeyCode::Char('h'));
    assert_eq!(view.focused, Column::Entries);

    // -- Check: h from the entries pane wraps like BackTab does
    view.handle_key(KeyCode::Char('h'));
    assert_eq!(view.focused, Column::Detail);

    // -- Check: l from the detail pane wraps like Tab does
    view.handle_key(KeyCode::Char('l'));
    assert_eq!(view.focused, Column::Entries);
}

// ── gg / G ────────────────────────────────────────────────────────────────────

#[test]
fn test_vim_gg_g_bounds() {
    // -- Setup & Fixtures
    let mut view = view_with(5);

    // -- Exec: G alone reaches the last row
    view.handle_key(KeyCode::Char('G'));

    // -- Check
    assert_eq!(selected(&view).as_deref(), Some("entry-4"));

    // -- Exec: gg returns to the first row
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('g'));

    // -- Check
    assert_eq!(selected(&view).as_deref(), Some("entry-0"));

    // -- Exec: a stray key between the two presses clears the pending g
    view.handle_key(KeyCode::Char('j')); // → entry-1
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('z')); // clears the pending g
    view.handle_key(KeyCode::Char('g')); // starts a new pending g

    // -- Check: the list did not jump to the top
    assert_eq!(
        selected(&view).as_deref(),
        Some("entry-1"),
        "a cleared pending g must not move the selection"
    );
}

// ── Ctrl-d / Ctrl-u ────────────────────────────────────────────────────────────

#[test]
fn test_vim_ctrl_d_half_page() {
    // -- Setup & Fixtures: a list longer than the window
    let mut view = view_with(20);
    view.list_view_rows = 8; // half page = 4

    // -- Exec
    view.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::CONTROL);

    // -- Check: the selection jumps half the window, not a single row
    assert_eq!(selected(&view).as_deref(), Some("entry-4"));

    // -- Exec: up again
    view.handle_key_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL);

    // -- Check: back at the top, clamped
    assert_eq!(selected(&view).as_deref(), Some("entry-0"));

    // -- Exec: half-pages past the end clamp to the last row
    for _ in 0..10 {
        view.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::CONTROL);
    }

    // -- Check
    assert_eq!(selected(&view).as_deref(), Some("entry-19"));
}

#[test]
fn test_vim_chords_are_inert_while_search_or_overlay_owns_keys() {
    // -- Setup & Fixtures
    let mut view = view_with(4);

    // -- Exec: open the search box, then try the new chords
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('l'));
    view.handle_key(KeyCode::Char('G'));
    view.handle_key(KeyCode::Char('g'));

    // -- Check: focus is untouched; the query took the letters instead
    assert_eq!(view.focused, Column::Entries);
    assert_eq!(
        view.search_query(),
        "lgg",
        "the search box owns printable keys, so the chords never fire"
    );

    // -- Exec: leaving the search restores the list, selection back at the top
    view.handle_key(KeyCode::Esc);

    // -- Check
    assert_eq!(selected(&view).as_deref(), Some("entry-0"));

    // -- Exec: leader mode owns the next key too
    view.handle_key(KeyCode::Esc);
    view.handle_key(KeyCode::Char(' ')); // leader
    view.handle_key(KeyCode::Char('l'));

    // -- Check
    assert_eq!(
        view.focused,
        Column::Entries,
        "leader mode must consume the pane chord"
    );
}

// ── the navigation set reaches the detail pane too ────────────────────────────

#[test]
fn test_vim_navigation_reaches_the_detail_pane() {
    // -- Setup & Fixtures: one entry with six field rows
    let mut view = view_with_fields(6);
    view.handle_key(KeyCode::Char('l')); // → Detail
    assert_eq!(view.focused, Column::Detail);
    assert_eq!(view.detail_field, 0);

    // -- Exec: G jumps to the last field row
    view.handle_key(KeyCode::Char('G'));

    // -- Check
    assert_eq!(view.detail_field, 5);

    // -- Exec: gg returns to the first
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('g'));

    // -- Check
    assert_eq!(view.detail_field, 0);

    // -- Exec: half a page down and up
    view.list_view_rows = 4; // half page = 2
    view.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(view.detail_field, 2);
    view.handle_key_with_modifiers(KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(view.detail_field, 0);

    // -- Exec: half-pages past the end clamp to the last field
    for _ in 0..10 {
        view.handle_key_with_modifiers(KeyCode::Char('d'), KeyModifiers::CONTROL);
    }

    // -- Check: clamped, and the entry selection never moved
    assert_eq!(view.detail_field, 5);
    assert_eq!(selected(&view).as_deref(), Some("entry-0"));
}
