//! Main view state — struct, constructor, accessors, and reload helpers.

use super::actions::Column;
use bogita_core::domain::{Entry, Vault};
use ratatui::widgets::ListState;
use std::collections::HashSet;

// ── MainView ──────────────────────────────────────────────────────────────────

pub struct MainView {
    /// All vaults.  Index 0 corresponds to "All Vaults" (virtual row).
    pub(super) vaults: Vec<Vault>,
    /// All entries across every vault.
    pub(super) entries: Vec<Entry>,

    /// Currently focused column.
    pub focused: Column,

    // Col 1 state
    pub(super) vault_state: ListState,

    // Col 2 state
    pub(super) entry_state: ListState,

    // Col 3 state — index into the *detail* field list for the selected entry
    pub(super) detail_field: usize,
    /// Set of field indices in the detail view whose hidden value is revealed.
    pub(super) revealed: HashSet<usize>,

    // Search / filter state
    /// Current incremental search string (lowercase).
    pub(super) search_query: String,
    /// Whether the search bar is currently accepting input.
    pub(super) searching: bool,
    /// Whether the Space leader key has been pressed and we're awaiting the action key.
    pub(super) leader_mode: bool,
    /// Whether the vault picker dropdown is open.
    pub(super) vault_picker_open: bool,
}

impl MainView {
    pub fn new(vaults: Vec<Vault>, entries: Vec<Entry>) -> Self {
        let mut vault_state = ListState::default();
        vault_state.select(Some(0)); // "All Vaults" selected initially

        let mut entry_state = ListState::default();
        if !entries.is_empty() {
            entry_state.select(Some(0));
        }

        Self {
            vaults,
            entries,
            focused: Column::Entries,
            vault_state,
            entry_state,
            detail_field: 0,
            revealed: HashSet::new(),
            search_query: String::new(),
            searching: false,
            leader_mode: false,
            vault_picker_open: false,
        }
    }

    // ── public accessors ──────────────────────────────────────────────────────

    /// Number of vaults loaded (not counting the virtual "All Vaults" row).
    pub(crate) fn vault_count(&self) -> usize {
        self.vaults.len()
    }

    /// Return a snapshot of all vaults (for use by the parent Tui).
    pub fn vaults_snapshot(&self) -> Vec<bogita_core::domain::Vault> {
        self.vaults.clone()
    }

    /// Whether the Space leader key has been pressed and we're awaiting an action key.
    pub fn is_leader_mode(&self) -> bool {
        self.leader_mode
    }

    /// Whether the vault picker dropdown is open.
    pub fn is_vault_picker_open(&self) -> bool {
        self.vault_picker_open
    }

    /// Whether the search bar is currently accepting input.
    pub(crate) fn is_searching(&self) -> bool {
        self.searching
    }

    /// The current search query string (always lowercase).
    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    /// The vault currently selected in Col 1, if any.
    /// Returns `None` if "All Vaults" (index 0) is selected or no vaults exist.
    pub fn selected_vault_id(&self) -> Option<uuid::Uuid> {
        let idx = self.vault_state.selected()?;
        // Index 0 is "All Vaults" — no specific vault selected
        if idx == 0 {
            return None;
        }
        self.vaults.get(idx - 1).map(|v| v.id)
    }

    /// The entry currently selected in Col 2, if any.
    pub fn selected_entry(&self) -> Option<&Entry> {
        let idx = self.entry_state.selected()?;
        self.visible_entries().into_iter().nth(idx)
    }

    /// Entries visible in Col 2 (filtered by selected vault and search query).
    pub fn visible_entries(&self) -> Vec<&Entry> {
        let vault_filtered: Vec<&Entry> = match self.vault_state.selected() {
            // Index 0 = "All Vaults"
            None | Some(0) => self.entries.iter().collect(),
            Some(vault_idx) => {
                // vault_idx is 1-based because row 0 is "All Vaults"
                let vault = self.vaults.get(vault_idx - 1);
                match vault {
                    None => self.entries.iter().collect(),
                    Some(v) => self.entries.iter().filter(|e| e.vault_id == v.id).collect(),
                }
            }
        };

        if self.search_query.is_empty() {
            vault_filtered
        } else {
            vault_filtered
                .into_iter()
                .filter(|e| e.name.to_lowercase().contains(&self.search_query))
                .collect()
        }
    }

    /// Whether the field at `idx` in the currently selected entry is revealed.
    pub(crate) fn is_field_revealed(&self, idx: usize) -> bool {
        self.revealed.contains(&idx)
    }

    /// Replace the entry list (e.g. after add/edit/delete) and reset selection.
    pub fn reload_entries(&mut self, entries: Vec<bogita_core::domain::Entry>) {
        self.entries = entries;
        self.reset_entry_selection();
    }

    /// Replace the vault list.
    pub fn reload_vaults(&mut self, vaults: Vec<Vault>) {
        self.vaults = vaults;
        self.vault_state.select(Some(0));
    }

    /// Replace the entry list and attempt to re-select the entry with `select_id`.
    /// Falls back to index 0 if the id is not found or `select_id` is `None`.
    pub fn reload_entries_select(
        &mut self,
        entries: Vec<bogita_core::domain::Entry>,
        select_id: Option<uuid::Uuid>,
    ) {
        self.entries = entries;
        self.detail_field = 0;
        self.revealed.clear();

        let visible = self.visible_entries();
        if visible.is_empty() {
            self.entry_state.select(None);
            return;
        }

        let target_idx = select_id
            .and_then(|id| visible.iter().position(|e| e.id == id))
            .unwrap_or(0);
        self.entry_state.select(Some(target_idx));
    }
}
