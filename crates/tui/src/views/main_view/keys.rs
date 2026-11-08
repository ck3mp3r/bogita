//! Key handling — search, leader mode, navigation, reveal, and vault picker.

use super::actions::{Column, MainViewAction};
use super::state::MainView;
use bogita_core::domain::FieldValue;
use ratatui::crossterm::event::KeyCode;

impl MainView {
    // ── key handling ─────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyCode) -> MainViewAction {
        // Vault picker dropdown — intercept all keys when open.
        if self.vault_picker_open {
            return self.handle_vault_picker_key(key);
        }

        // When the search bar is open, all input goes to the search query.
        if self.searching {
            match key {
                KeyCode::Esc => {
                    self.searching = false;
                    self.search_query.clear();
                    self.reset_entry_selection();
                }
                KeyCode::Enter => {
                    self.searching = false;
                    // query stays active
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.reset_entry_selection();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c.to_lowercase().next().unwrap_or(c));
                    self.reset_entry_selection();
                }
                _ => {}
            }
            return MainViewAction::None;
        }

        // Leader mode: Space was previously pressed — dispatch or cancel.
        if self.leader_mode {
            self.leader_mode = false;
            return match (&self.focused, key) {
                // Entries column: a = add entry, e = edit entry, d = delete entry
                (Column::Entries, KeyCode::Char('a')) => {
                    let vault_id = match self.vault_state.selected() {
                        None | Some(0) => self.vaults.first().map(|v| v.id),
                        Some(idx) => self.vaults.get(idx - 1).map(|v| v.id),
                    };
                    match vault_id {
                        Some(id) => MainViewAction::OpenAddForm { vault_id: id },
                        None => MainViewAction::None,
                    }
                }
                (Column::Entries, KeyCode::Char('e')) => match self.selected_entry() {
                    Some(e) => MainViewAction::OpenEditForm { entry_id: e.id },
                    None => MainViewAction::None,
                },
                (Column::Entries, KeyCode::Char('d')) => match self.selected_entry() {
                    Some(e) => MainViewAction::DeleteEntry { entry_id: e.id },
                    None => MainViewAction::None,
                },
                // Detail column: no leader actions (c and s work directly without leader)
                // Esc or any unrecognised key cancels leader mode silently.
                _ => MainViewAction::None,
            };
        }

        match key {
            KeyCode::Char(' ') => {
                self.leader_mode = true;
                MainViewAction::None
            }
            KeyCode::Char('/') => {
                self.searching = true;
                MainViewAction::None
            }
            KeyCode::Char('v') => {
                self.vault_picker_open = true;
                MainViewAction::None
            }
            KeyCode::Tab => {
                self.focused = self.focused.next();
                if self.focused == Column::Detail {
                    self.detail_field = 0;
                    if let Some(e) = self.selected_entry() {
                        return MainViewAction::SelectEntry {
                            entry_id: e.id,
                            vault_id: e.vault_id,
                        };
                    }
                }
                MainViewAction::None
            }
            KeyCode::BackTab => {
                self.focused = self.focused.next();
                if self.focused == Column::Detail {
                    self.detail_field = 0;
                    if let Some(e) = self.selected_entry() {
                        return MainViewAction::SelectEntry {
                            entry_id: e.id,
                            vault_id: e.vault_id,
                        };
                    }
                }
                MainViewAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_down();
                if self.focused == Column::Entries {
                    if let Some(e) = self.selected_entry() {
                        return MainViewAction::SelectEntry {
                            entry_id: e.id,
                            vault_id: e.vault_id,
                        };
                    }
                }
                MainViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_up();
                if self.focused == Column::Entries {
                    if let Some(e) = self.selected_entry() {
                        return MainViewAction::SelectEntry {
                            entry_id: e.id,
                            vault_id: e.vault_id,
                        };
                    }
                }
                MainViewAction::None
            }
            KeyCode::Char('s') => {
                self.toggle_reveal();
                MainViewAction::None
            }
            KeyCode::Char('c') => {
                if self.focused == Column::Detail {
                    match self.selected_entry() {
                        Some(e) => MainViewAction::CopyField {
                            entry_id: e.id,
                            field_idx: self.detail_field,
                        },
                        None => MainViewAction::None,
                    }
                } else {
                    MainViewAction::None
                }
            }
            _ => MainViewAction::None,
        }
    }

    fn move_down(&mut self) {
        match self.focused {
            Column::Entries => {
                let len = self.visible_entries().len();
                let cur = self.entry_state.selected().unwrap_or(0);
                let next = (cur + 1).min(len.saturating_sub(1));
                self.entry_state.select(Some(next));
                self.detail_field = 0;
                self.revealed.clear();
            }
            Column::Detail => {
                let field_count = self.selected_entry().map(|e| e.fields.len()).unwrap_or(0);
                if field_count > 0 {
                    self.detail_field = (self.detail_field + 1).min(field_count.saturating_sub(1));
                }
            }
        }
    }

    fn move_up(&mut self) {
        match self.focused {
            Column::Entries => {
                let cur = self.entry_state.selected().unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.entry_state.select(Some(next));
                self.detail_field = 0;
                self.revealed.clear();
            }
            Column::Detail => {
                self.detail_field = self.detail_field.saturating_sub(1);
            }
        }
    }

    pub(super) fn reset_entry_selection(&mut self) {
        self.detail_field = 0;
        self.revealed.clear();
        if self.visible_entries().is_empty() {
            self.entry_state.select(None);
        } else {
            self.entry_state.select(Some(0));
        }
    }

    fn toggle_reveal(&mut self) {
        if self.focused != Column::Detail {
            return;
        }
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let Some(field) = entry.fields.get(self.detail_field) else {
            return;
        };
        if !matches!(field.value, FieldValue::Hidden(_)) {
            return;
        }
        if self.revealed.contains(&self.detail_field) {
            self.revealed.remove(&self.detail_field);
        } else {
            self.revealed.insert(self.detail_field);
        }
    }

    fn handle_vault_picker_key(&mut self, key: KeyCode) -> MainViewAction {
        let len = self.vaults.len() + 1; // +1 for "All Vaults"
        match key {
            KeyCode::Char('j') | KeyCode::Down => {
                let cur = self.vault_state.selected().unwrap_or(0);
                let next = (cur + 1).min(len.saturating_sub(1));
                self.vault_state.select(Some(next));
                MainViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let cur = self.vault_state.selected().unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.vault_state.select(Some(next));
                MainViewAction::None
            }
            KeyCode::Enter => {
                self.vault_picker_open = false;
                self.reset_entry_selection();
                MainViewAction::None
            }
            KeyCode::Esc => {
                self.vault_picker_open = false;
                MainViewAction::None
            }
            KeyCode::Char('a') => {
                self.vault_picker_open = false;
                MainViewAction::OpenAddVault
            }
            KeyCode::Char('d') => {
                // Only allow delete on a specific vault (not "All Vaults" at index 0)
                if let Some(id) = self.selected_vault_id() {
                    self.vault_picker_open = false;
                    MainViewAction::DeleteVault { vault_id: id }
                } else {
                    MainViewAction::None
                }
            }
            _ => MainViewAction::None,
        }
    }
}
