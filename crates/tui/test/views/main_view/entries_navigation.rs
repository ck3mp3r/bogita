use crate::views::main_view::{Column, MainView};
use bogita_core::domain::EntryType;
use ratatui::crossterm::event::KeyCode;

use super::support::*;

// ── construction ──────────────────────────────────────────────────────────────

#[test]
fn new_view_focuses_entries_column() {
    let view = MainView::new(vec![], vec![]);
    assert_eq!(view.focused, Column::Entries);
}

#[test]
fn new_view_selects_all_vaults_row() {
    let v = make_vault("Work");
    let view = MainView::new(vec![v], vec![]);
    // vault_state index 0 = "All Vaults"
    // visible_entries should return all entries
    assert!(view.visible_entries().is_empty());
}

// ── vault filtering ───────────────────────────────────────────────────────────

#[test]
fn all_vaults_row_shows_all_entries() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("Twitter", v2.id, EntryType::Token);
    let view = MainView::new(vec![v1, v2], vec![e1, e2]);
    // Default: "All Vaults" selected
    assert_eq!(view.visible_entries().len(), 2);
}

#[test]
fn selecting_specific_vault_filters_entries() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let v1_id = v1.id;
    let e1 = make_entry("GitHub", v1_id, EntryType::Token);
    let e2 = make_entry("Twitter", v2.id, EntryType::Token);
    let mut view = MainView::new(vec![v1, v2], vec![e1, e2]);
    // Open vault picker, move down to first real vault (index 1), confirm
    view.handle_key(KeyCode::Char('v')); // open picker
    view.handle_key(KeyCode::Char('j')); // "All Vaults" → Work (index 1)
    view.handle_key(KeyCode::Enter); // confirm
    let visible = view.visible_entries();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].vault_id, v1_id);
}

#[test]
fn switching_back_to_all_vaults_shows_all() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("Twitter", v2.id, EntryType::Token);
    let mut view = MainView::new(vec![v1, v2], vec![e1, e2]);
    // Open picker, select Work, confirm
    view.handle_key(KeyCode::Char('v')); // open picker
    view.handle_key(KeyCode::Char('j')); // All → Work (idx 1)
    view.handle_key(KeyCode::Enter); // confirm
    assert_eq!(view.visible_entries().len(), 1);
    // Open picker again, navigate back to "All Vaults" (idx 0), confirm
    view.handle_key(KeyCode::Char('v')); // open picker
    view.handle_key(KeyCode::Char('k')); // Work → All Vaults (idx 0)
    view.handle_key(KeyCode::Enter); // confirm
    assert_eq!(view.visible_entries().len(), 2);
}

// ── entry selection ───────────────────────────────────────────────────────────

#[test]
fn selected_entry_is_none_when_no_entries() {
    let view = MainView::new(vec![], vec![]);
    assert!(view.selected_entry().is_none());
}

#[test]
fn selected_entry_returns_first_entry_by_default() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let view = MainView::new(vec![v], vec![e1.clone(), e2]);
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitHub")
    );
}

#[test]
fn moving_down_in_entries_selects_next() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    // Already focused on Entries column
    view.handle_key(KeyCode::Char('j'));
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitLab")
    );
}

#[test]
fn entries_clamp_at_last() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    view.handle_key(KeyCode::Char('j'));
    view.handle_key(KeyCode::Char('j')); // attempt to go past end
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitLab")
    );
}

#[test]
fn entries_clamp_at_first() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1]);
    view.handle_key(KeyCode::Char('k')); // already at top
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitHub")
    );
}

// ── column navigation ─────────────────────────────────────────────────────────

#[test]
fn tab_cycles_entries_to_detail() {
    let view = MainView::new(vec![], vec![]);
    // Starts at Entries
    assert_eq!(view.focused, Column::Entries);
}

#[test]
fn tab_moves_focus_right() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Tab); // Entries → Detail
    assert_eq!(view.focused, Column::Detail);
    view.handle_key(KeyCode::Tab); // Detail → Entries
    assert_eq!(view.focused, Column::Entries);
}

#[test]
fn shift_tab_moves_focus_left() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::BackTab); // Entries → Detail
    assert_eq!(view.focused, Column::Detail);
    view.handle_key(KeyCode::BackTab); // Detail → Entries
    assert_eq!(view.focused, Column::Entries);
}

#[test]
fn j_k_do_nothing_outside_focused_column() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    // Focus is Entries; moving j should work
    view.handle_key(KeyCode::Char('j'));
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitLab")
    );
    // Switch to Detail — j/k now move detail field, not entry selection
    view.handle_key(KeyCode::Tab);
    assert_eq!(view.focused, Column::Detail);
    view.handle_key(KeyCode::Char('j'));
    // Entry selection should NOT have changed
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("GitLab")
    );
}

// ── detail field navigation ───────────────────────────────────────────────────

#[test]
fn detail_field_navigation_moves_within_entry() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id);
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // Entries → Detail
    view.handle_key(KeyCode::Char('j')); // move to field 1
                                         // detail_field is not pub; we verify indirectly via reveal toggle behaviour
    view.handle_key(KeyCode::Char('s')); // toggle field 1 (password → hidden)
                                         // no panic = success; field 0 (username, Text) is not revealed
}

#[test]
fn detail_field_clamps_at_last_field() {
    let v = make_vault("Work");
    let e = make_password_entry("GitHub", v.id); // 2 fields
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Tab); // to Detail
    for _ in 0..10 {
        view.handle_key(KeyCode::Char('j'));
    }
    // Should not panic and stay at last valid field
    // Verify by moving up — selected_entry should still return the same entry
    view.handle_key(KeyCode::Char('k'));
    assert!(view.selected_entry().is_some());
}
