use crate::views::main_view::{MainView, MainViewAction};
use bogita_core::domain::EntryType;
use ratatui::crossterm::event::KeyCode;

use super::support::*;

// ── reveal / hide hidden fields ───────────────────────────────────────────────

#[test]
fn s_has_no_effect_outside_detail_column() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    // Focus is Entries — s should do nothing (no panic)
    view.handle_key(KeyCode::Char('s'));
    assert!(view.selected_entry().is_some()); // still alive
}

#[test]
fn s_on_text_field_does_nothing() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // to Detail — field 0 is username (Text)
    view.handle_key(KeyCode::Char('s')); // no-op on Text field — no panic
    assert!(view.selected_entry().is_some());
}

#[test]
fn s_toggles_reveal_on_hidden_field() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // to Detail
    view.handle_key(KeyCode::Char('j')); // to field 1 (password, Hidden)
    view.handle_key(KeyCode::Char('s')); // reveal
    view.handle_key(KeyCode::Char('s')); // re-mask
                                         // No panic = correct behaviour; actual display is verified in render
    assert!(view.selected_entry().is_some());
}

// ── reveal: field_display via public accessor ─────────────────────────────────

#[test]
fn s_reveals_hidden_field_value_in_detail() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // Entries → Detail
    view.handle_key(KeyCode::Char('j')); // detail_field → 1 (password, Hidden)
    view.handle_key(KeyCode::Char('s')); // reveal
                                         // field 1 is Hidden("s3cr3t") — after reveal, display_value should return plaintext
    assert!(
        view.is_field_revealed(1),
        "[s] should reveal the hidden field"
    );
}

#[test]
fn s_again_masks_revealed_field() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // → Detail
    view.handle_key(KeyCode::Char('j')); // → field 1
    view.handle_key(KeyCode::Char('s')); // reveal
    view.handle_key(KeyCode::Char('s')); // mask again
    assert!(
        !view.is_field_revealed(1),
        "second [s] should mask the field again"
    );
}

#[test]
fn reveal_persists_when_tabbing_away_and_back() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // → Detail
    view.handle_key(KeyCode::Char('j')); // → field 1
    view.handle_key(KeyCode::Char('s')); // reveal
    assert!(view.is_field_revealed(1));
    view.handle_key(KeyCode::Tab); // Detail → Entries
    view.handle_key(KeyCode::Tab); // Entries → Detail
    assert!(
        view.is_field_revealed(1),
        "reveal should persist across tab navigation"
    );
}

#[test]
fn reveal_clears_when_entry_selection_changes() {
    let v = make_vault("Work");
    let e1 = make_password_entry("GitHub", v.id);
    let e2 = make_password_entry("GitLab", v.id);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    view.handle_key(KeyCode::Tab); // → Detail
    view.handle_key(KeyCode::Char('j')); // → field 1
    view.handle_key(KeyCode::Char('s')); // reveal
    assert!(view.is_field_revealed(1));
    view.handle_key(KeyCode::Tab); // → Entries
    view.handle_key(KeyCode::Char('j')); // select next entry
    assert!(
        !view.is_field_revealed(1),
        "reveal should clear when a different entry is selected"
    );
}

// ── SelectEntry action on navigation ──────────────────────────────────────────

#[test]
fn j_in_entries_returns_select_entry_action() {
    let v = make_vault("Work");
    let e1 = make_password_entry("GitHub", v.id);
    let e2 = make_password_entry("GitLab", v.id);
    let e2_id = e2.id;
    let e2_vid = e2.vault_id;
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    let action = view.handle_key(KeyCode::Char('j'));
    assert_eq!(
        action,
        MainViewAction::SelectEntry {
            entry_id: e2_id,
            vault_id: e2_vid
        },
        "j in Entries should return SelectEntry for the newly selected entry"
    );
}

#[test]
fn tab_to_detail_returns_select_entry_action() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let eid = e.id;
    let vid = e.vault_id;
    let mut view = MainView::new(vec![v], vec![e]);
    let action = view.handle_key(KeyCode::Tab); // Entries → Detail
    assert_eq!(
        action,
        MainViewAction::SelectEntry {
            entry_id: eid,
            vault_id: vid
        },
        "Tab to Detail should return SelectEntry"
    );
}

#[test]
fn otp_entry_is_selectable() {
    let v = make_vault("Work");
    let e = make_otp_entry("AWS", v.id);
    let view = MainView::new(vec![v], vec![e]);
    assert_eq!(
        view.selected_entry().map(|e| e.entry_type.clone()),
        Some(EntryType::Otp),
    );
}

#[test]
fn switching_vault_resets_entry_selection() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("GitLab", v1.id, EntryType::Token);
    let e3 = make_entry("Twitter", v2.id, EntryType::Token);
    let mut view = MainView::new(vec![v1, v2], vec![e1, e2, e3]);
    // Select second entry in Work vault
    view.handle_key(KeyCode::Char('j'));
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitLab")
    );
    // Open vault picker and select Personal
    view.handle_key(KeyCode::Char('v')); // open picker
    view.handle_key(KeyCode::Char('j')); // All → Work (idx 1)
    view.handle_key(KeyCode::Char('j')); // Work → Personal (idx 2)
    view.handle_key(KeyCode::Enter); // confirm
                                     // Entry selection should have reset to first entry in Personal
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("Twitter")
    );
}
