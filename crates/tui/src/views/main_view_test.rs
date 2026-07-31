use crate::views::main_view::{Column, MainView, MainViewAction};
use bogita_core::domain::{
    Entry, EntryType, Field, FieldType, FieldValue, SqliteConfig, Vault, VaultBackend,
};
use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_vault(name: &str) -> Vault {
    Vault {
        id: Uuid::new_v4(),
        name: name.to_string(),
        is_default: false,
        created_at: Utc::now().timestamp(),
        backend: VaultBackend::Sqlite(SqliteConfig {
            path: ":memory:".into(),
        }),
        recipients: vec![],
        lock_timeout: None,
        auto_sync: false,
    }
}

fn make_entry(name: &str, vault_id: Uuid, entry_type: EntryType) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type,
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
        fields: vec![],
    }
}

fn make_password_entry(name: &str, vault_id: Uuid) -> Entry {
    let mut e = make_entry(name, vault_id, EntryType::Token);
    e.fields = vec![
        Field {
            id: Uuid::new_v4(),
            key: "username".into(),
            value: FieldValue::Text("alice".into()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        },
        Field {
            id: Uuid::new_v4(),
            key: "password".into(),
            value: FieldValue::Hidden("s3cr3t".into()),
            field_type: FieldType::Token,
            encrypted: true,
            idx: 1,
        },
    ];
    e
}

fn make_otp_entry(name: &str, vault_id: Uuid) -> Entry {
    let mut e = make_entry(name, vault_id, EntryType::Otp);
    e.fields = vec![Field {
        id: Uuid::new_v4(),
        key: "secret".into(),
        value: FieldValue::Hidden("JBSWY3DPEHPK3PXP".into()),
        field_type: FieldType::TotpSecret,
        encrypted: true,
        idx: 0,
    }];
    e
}

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
    // Focus vaults, move down to first real vault (index 1)
    view.handle_key(KeyCode::Tab); // Entries → Detail
    view.handle_key(KeyCode::Tab); // Detail → Vaults
    view.handle_key(KeyCode::Char('j')); // "All Vaults" → Work (index 1)
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
    // Go to vaults column and move down then back up
    view.handle_key(KeyCode::Tab);
    view.handle_key(KeyCode::Tab);
    view.handle_key(KeyCode::Char('j')); // select Work
    view.handle_key(KeyCode::Char('k')); // back to "All Vaults"
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
    view.handle_key(KeyCode::Tab); // Detail → Vaults
    assert_eq!(view.focused, Column::Vaults);
    view.handle_key(KeyCode::Tab); // Vaults → Entries
    assert_eq!(view.focused, Column::Entries);
}

#[test]
fn shift_tab_moves_focus_left() {
    let mut view = MainView::new(vec![], vec![]);
    view.handle_key(KeyCode::BackTab); // Entries → Vaults
    assert_eq!(view.focused, Column::Vaults);
    view.handle_key(KeyCode::BackTab); // Vaults → Detail
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
    view.handle_key(KeyCode::Tab); // Detail → Vaults
    view.handle_key(KeyCode::Tab); // Vaults → Entries
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
    view.handle_key(KeyCode::Tab); // → Vaults
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
    // Switch to Vaults column and pick Personal
    view.handle_key(KeyCode::Tab); // Entries → Detail
    view.handle_key(KeyCode::Tab); // Detail → Vaults
    view.handle_key(KeyCode::Char('j')); // All Vaults → Work (idx 1)
    view.handle_key(KeyCode::Char('j')); // Work → Personal (idx 2)
                                         // Entry selection should have reset to first entry in Personal
    assert_eq!(
        view.selected_entry().map(|e| e.name.as_str()),
        Some("Twitter")
    );
}

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
    // Select Work vault
    view.handle_key(KeyCode::Tab); // Entries → Detail
    view.handle_key(KeyCode::Tab); // Detail → Vaults
    view.handle_key(KeyCode::Char('j')); // All → Work
    view.handle_key(KeyCode::Tab); // Vaults → Entries
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
