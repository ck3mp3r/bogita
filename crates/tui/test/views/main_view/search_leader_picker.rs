use crate::views::main_view::{MainView, MainViewAction};
use bogita_core::domain::EntryType;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::*;

// ── search / filter ───────────────────────────────────────────────────────────

#[test]
fn slash_enters_search_mode() {
    let view = MainView::new(vec![], vec![]);
    assert!(!view.is_searching());
    // handled in handle_key; slash is sent to Tui which passes through to MainView
}

#[test]
fn search_mode_entered_via_slash_key() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));
    assert!(view.is_searching());
}

#[test]
fn typing_in_search_mode_builds_query() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('i'));
    view.handle_key(KeyCode::Char('t'));
    assert_eq!(view.search_query(), "git");
}

#[test]
fn modified_chars_are_ignored_in_search_query() {
    // -- Setup & Fixtures
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));

    // -- Exec
    view.handle_key_with_modifiers(KeyCode::Char('g'), KeyModifiers::NONE);
    view.handle_key_with_modifiers(KeyCode::Char('x'), KeyModifiers::CONTROL);
    view.handle_key_with_modifiers(KeyCode::Char('y'), KeyModifiers::ALT);

    // -- Check
    assert_eq!(view.search_query(), "g");
}

#[test]
fn shifted_char_is_inserted_in_search_query() {
    // -- Setup & Fixtures
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));

    // -- Exec
    view.handle_key_with_modifiers(KeyCode::Char('G'), KeyModifiers::SHIFT);

    // -- Check
    assert_eq!(view.search_query(), "g");
}

#[test]
fn backspace_removes_last_char() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('i'));
    view.handle_key(KeyCode::Backspace);
    assert_eq!(view.search_query(), "g");
}

#[test]
fn esc_clears_query_and_exits_search() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Esc);
    assert!(!view.is_searching());
    assert_eq!(view.search_query(), "");
}

#[test]
fn enter_exits_search_mode_but_keeps_query() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Enter);
    assert!(!view.is_searching());
    assert_eq!(view.search_query(), "g");
}

#[test]
fn search_filters_visible_entries_by_name() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let e3 = make_entry("AWS", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2, e3]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('i'));
    view.handle_key(KeyCode::Char('t'));
    let visible = view.visible_entries();
    assert_eq!(visible.len(), 2);
    assert!(visible.iter().any(|e| e.name == "GitHub"));
    assert!(visible.iter().any(|e| e.name == "GitLab"));
}

#[test]
fn search_is_case_insensitive() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("AWS", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('G'));
    view.handle_key(KeyCode::Char('I'));
    view.handle_key(KeyCode::Char('T'));
    assert_eq!(view.visible_entries().len(), 1);
    assert_eq!(view.visible_entries()[0].name, "GitHub");
}

#[test]
fn clearing_search_restores_all_entries() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("AWS", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2]);
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    assert_eq!(view.visible_entries().len(), 1);
    view.handle_key(KeyCode::Esc);
    assert_eq!(view.visible_entries().len(), 2);
}

#[test]
fn search_combined_with_vault_filter() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("GitLab", v1.id, EntryType::Token);
    let e3 = make_entry("GitHub Personal", v2.id, EntryType::Token);
    let mut view = MainView::new(vec![v1, v2], vec![e1, e2, e3]);
    // Open vault picker and select Work
    view.handle_key(KeyCode::Char('v')); // open picker
    view.handle_key(KeyCode::Char('j')); // All → Work
    view.handle_key(KeyCode::Enter); // confirm
                                     // Now search for "git"
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('g'));
    view.handle_key(KeyCode::Char('i'));
    view.handle_key(KeyCode::Char('t'));
    // Only Work vault entries matching "git"
    let visible = view.visible_entries();
    assert_eq!(visible.len(), 2);
    assert!(visible
        .iter()
        .all(|e| e.name.to_lowercase().contains("git")));
}

#[test]
fn search_resets_entry_selection_to_first_match() {
    let v = make_vault("Work");
    let e1 = make_entry("GitHub", v.id, EntryType::Token);
    let e2 = make_entry("GitLab", v.id, EntryType::Token);
    let e3 = make_entry("AWS", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e1, e2, e3]);
    // Move to last entry
    view.handle_key(KeyCode::Char('j'));
    view.handle_key(KeyCode::Char('j'));
    // Search for "aws" (unique prefix — only AWS matches)
    view.handle_key(KeyCode::Char('/'));
    view.handle_key(KeyCode::Char('a'));
    view.handle_key(KeyCode::Char('w'));
    assert_eq!(view.selected_entry().map(|e| e.name.as_str()), Some("AWS"));
}

// ── leader key (Space) ────────────────────────────────────────────────────────

#[test]
fn space_enters_leader_mode() {
    let mut view = MainView::new(vec![], vec![]);
    assert!(!view.is_leader_mode());
    view.handle_key(KeyCode::Char(' '));
    assert!(view.is_leader_mode());
}

#[test]
fn esc_cancels_leader_mode() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char(' '));
    view.handle_key(KeyCode::Esc);
    assert!(!view.is_leader_mode());
}

#[test]
fn unrecognised_key_in_leader_mode_cancels_it() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char(' '));
    view.handle_key(KeyCode::Char('z')); // not a bound key
    assert!(!view.is_leader_mode());
}

#[test]
fn space_a_opens_add_form() {
    let v = make_vault("Work");
    let mut view = MainView::new(vec![v.clone()], vec![]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('a'));
    assert!(
        matches!(action, MainViewAction::OpenAddForm { vault_id } if vault_id == v.id),
        "expected OpenAddForm"
    );
    assert!(!view.is_leader_mode());
}

#[test]
fn space_a_with_all_vaults_selected_uses_first_vault() {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let mut view = MainView::new(vec![v1.clone(), v2], vec![]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('a'));
    assert!(
        matches!(action, MainViewAction::OpenAddForm { vault_id } if vault_id == v1.id),
        "expected first vault's id"
    );
}

#[test]
fn space_a_with_no_vaults_returns_none() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('a'));
    assert_eq!(action, MainViewAction::None);
}

#[test]
fn space_e_on_selected_entry_returns_open_edit_form() {
    let v = make_vault("Work");
    let e = make_entry("GitHub", v.id, EntryType::Token);
    let eid = e.id;
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('e'));
    assert!(
        matches!(action, MainViewAction::OpenEditForm { entry_id } if entry_id == eid),
        "expected OpenEditForm"
    );
    assert!(!view.is_leader_mode());
}

#[test]
fn space_e_with_no_selection_returns_none() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('e'));
    assert_eq!(action, MainViewAction::None);
}

#[test]
fn space_d_on_selected_entry_returns_delete_entry() {
    let v = make_vault("Work");
    let e = make_entry("GitHub", v.id, EntryType::Token);
    let eid = e.id;
    let mut view = MainView::new(vec![v], vec![e]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('d'));
    assert!(
        matches!(action, MainViewAction::DeleteEntry { entry_id } if entry_id == eid),
        "expected DeleteEntry"
    );
    assert!(!view.is_leader_mode());
}

#[test]
fn space_d_with_no_selection_returns_none() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::Char(' '));
    let action = view.handle_key(KeyCode::Char('d'));
    assert_eq!(action, MainViewAction::None);
}

#[test]
fn direct_a_e_d_keys_do_nothing() {
    let v = make_vault("Work");
    let e = make_entry("GitHub", v.id, EntryType::Token);
    let mut view = MainView::new(vec![v], vec![e]);
    assert_eq!(view.handle_key(KeyCode::Char('a')), MainViewAction::None);
    assert_eq!(view.handle_key(KeyCode::Char('e')), MainViewAction::None);
    assert_eq!(view.handle_key(KeyCode::Char('d')), MainViewAction::None);
}

// ── vault picker ───────────────────────────────────────────────────────────────

#[test]
fn v_opens_vault_picker() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    assert!(view.is_vault_picker_open());
}

#[test]
fn esc_closes_vault_picker() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    assert!(view.is_vault_picker_open());
    view.handle_key(KeyCode::Esc);
    assert!(!view.is_vault_picker_open());
}

#[test]
fn enter_confirms_vault_selection() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    view.handle_key(KeyCode::Char('j')); // move to first vault
    view.handle_key(KeyCode::Enter);
    assert!(!view.is_vault_picker_open());
    // selection moved to the first vault (row index 1 behind the picker)
    assert_eq!(view.selected_vault_id(), Some(view.vaults_snapshot()[0].id));
}

#[test]
fn j_navigates_vault_picker_down() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    view.handle_key(KeyCode::Char('j'));
    assert_eq!(view.selected_vault_id(), Some(view.vaults_snapshot()[0].id));
}

#[test]
fn k_navigates_vault_picker_up() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    view.handle_key(KeyCode::Char('j'));
    view.handle_key(KeyCode::Char('k'));
    // back on the "All Vaults" row, so no specific vault is selected
    assert_eq!(view.selected_vault_id(), None);
}

#[test]
fn a_in_picker_returns_open_add_vault() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    let action = view.handle_key(KeyCode::Char('a'));
    assert!(!view.is_vault_picker_open());
    assert_eq!(action, MainViewAction::OpenAddVault);
}

#[test]
fn d_in_picker_returns_delete_vault() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    view.handle_key(KeyCode::Char('j')); // select first vault
    let action = view.handle_key(KeyCode::Char('d'));
    assert!(!view.is_vault_picker_open());
    assert!(matches!(action, MainViewAction::DeleteVault { .. }));
}

#[test]
fn d_in_picker_on_all_vaults_is_noop() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    // "All Vaults" is selected by default (index 0)
    let action = view.handle_key(KeyCode::Char('d'));
    assert!(view.is_vault_picker_open()); // still open
    assert_eq!(action, MainViewAction::None);
}

#[test]
fn vault_picker_blocks_other_keys() {
    let mut view = make_view_with_vaults();
    view.handle_key(KeyCode::Char('v'));
    let action = view.handle_key(KeyCode::Char('x'));
    assert!(view.is_vault_picker_open()); // still open
    assert_eq!(action, MainViewAction::None);
}
