//! Key handling — search, leader mode, navigation, reveal, and vault picker.

use super::actions::{Column, MainViewAction};
use super::state::MainView;
use crate::views::inserts_text;
use bogita_core::domain::FieldValue;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

impl MainView {
    // ── key handling ─────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyCode) -> MainViewAction {
        self.handle_key_with_modifiers(key, KeyModifiers::NONE)
    }

    /// Insert a bracketed-paste payload into the search query.
    ///
    /// The query is single-line and lowercased like its typed input, so the
    /// caller hands over an already flattened payload. A paste outside the
    /// open search bar is inert.
    pub fn handle_paste(&mut self, text: &str) {
        if !self.searching {
            return;
        }
        for c in text.chars() {
            self.search_query.push(c.to_lowercase().next().unwrap_or(c));
        }
        self.reset_entry_selection();
    }

    /// Handle a key press with its modifiers.
    ///
    /// The search box inserts only unmodified printable chars, so Ctrl and Alt
    /// chords stay available to the app instead of leaking into the query.
    pub fn handle_key_with_modifiers(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> MainViewAction {
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
                KeyCode::Char(c) if inserts_text(modifiers) => {
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

        // A pending `g` lasts exactly one key: anything but the second `g` clears
        // it, so `g x g` starts over instead of jumping.
        if key != KeyCode::Char('g') {
            self.pending_g = false;
        }
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let shift = modifiers.contains(KeyModifiers::SHIFT);

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
            KeyCode::Tab => self.move_focus(self.focused.next()),
            KeyCode::BackTab => self.move_focus(self.focused.prev()),
            // Vim-grade pane focus: the same transition, spelled h/l.
            KeyCode::Char('h') => self.move_focus(self.focused.prev()),
            KeyCode::Char('l') => self.move_focus(self.focused.next()),
            // gg / G reach the bounds of the focused list.
            //
            // A terminal reports the capital chord in one of two shapes: as the
            // uppercase glyph `G`, or as the lowercase `g` with SHIFT set (kitty
            // and xterm modifyOtherKeys deliver the base code point plus the
            // modifier). Both mean the same chord here, so the SHIFT arms come
            // before the pending-`g` arm and never open a `gg` sequence. Text
            // inputs keep the discipline rule and take the glyph as typed.
            KeyCode::Char('g') if shift => {
                self.pending_g = false;
                self.select_bound(false)
            }
            KeyCode::Char('g') if self.pending_g => {
                self.pending_g = false;
                self.select_bound(true)
            }
            KeyCode::Char('g') => {
                self.pending_g = true;
                MainViewAction::None
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                self.select_bound(false)
            }
            // Ctrl-d / Ctrl-u move half the painted window inside the list.
            KeyCode::Char('d') if ctrl => self.half_page(true),
            KeyCode::Char('u') if ctrl => self.half_page(false),
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
            // `y` yanks the selected field; `c` stays free for Ctrl-c's grammar.
            KeyCode::Char('y') => {
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

    /// Move focus to `target`, mirroring the Tab transition: reaching the detail
    /// pane asks the caller for the entry so the pane can be filled.
    fn move_focus(&mut self, target: Column) -> MainViewAction {
        let entering_detail = target == Column::Detail;
        self.focused = target;
        if entering_detail {
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

    /// `gg` / `G`: select the first (`first`) or last row of the focused list.
    ///
    /// Every scrollable surface owns the whole navigation set: the entries list
    /// moves its selection, the detail pane moves inside the entry's fields.
    fn select_bound(&mut self, first: bool) -> MainViewAction {
        match self.focused {
            Column::Entries => {
                let len = self.visible_entries().len();
                if len == 0 {
                    return MainViewAction::None;
                }
                let idx = if first { 0 } else { len - 1 };
                self.entry_state.select(Some(idx));
                self.detail_field = 0;
                self.revealed.clear();
                self.entry_action()
            }
            Column::Detail => {
                let count = self.field_count();
                if count == 0 {
                    return MainViewAction::None;
                }
                self.detail_field = if first { 0 } else { count - 1 };
                self.revealed.clear();
                MainViewAction::None
            }
        }
    }

    /// `Ctrl-d` / `Ctrl-u`: jump half the painted window inside the list.
    fn half_page(&mut self, down: bool) -> MainViewAction {
        let step = (self.list_view_rows / 2).max(1);
        match self.focused {
            Column::Entries => {
                let len = self.visible_entries().len();
                if len == 0 {
                    return MainViewAction::None;
                }
                let cur = self.entry_state.selected().unwrap_or(0);
                let next = if down {
                    (cur + step).min(len - 1)
                } else {
                    cur.saturating_sub(step)
                };
                self.entry_state.select(Some(next));
                self.detail_field = 0;
                self.revealed.clear();
                self.entry_action()
            }
            Column::Detail => {
                let count = self.field_count();
                if count == 0 {
                    return MainViewAction::None;
                }
                let cur = self.detail_field.min(count - 1);
                self.detail_field = if down {
                    (cur + step).min(count - 1)
                } else {
                    cur.saturating_sub(step)
                };
                self.revealed.clear();
                MainViewAction::None
            }
        }
    }

    /// Number of rows in the focused detail pane.
    fn field_count(&self) -> usize {
        self.selected_entry().map(|e| e.fields.len()).unwrap_or(0)
    }

    /// The `SelectEntry` action for the current selection, or `None`.
    fn entry_action(&self) -> MainViewAction {
        match self.selected_entry() {
            Some(e) => MainViewAction::SelectEntry {
                entry_id: e.id,
                vault_id: e.vault_id,
            },
            None => MainViewAction::None,
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
