//! Main 3-column view: vault list | entry list | entry detail.
//!
//! Column layout (approximate widths):
//!   [Col 1: ~20%] Vault list — "All Vaults" + one row per vault
//!   [Col 2: ~30%] Entry list — filtered by selected vault
//!   [Col 3: ~50%] Entry detail — fields rendered type-aware; live TOTP for OTP entries
//!
//! Navigation:
//!   Tab / Shift+Tab — move focus right / left between columns
//!   j / k / Down / Up — scroll within the focused column
//!   s — toggle reveal on the selected hidden field (Col 3 focused)

use bogita_core::domain::{Entry, FieldType, FieldValue, Vault};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::collections::HashSet;

// ── MainViewAction ────────────────────────────────────────────────────────────

/// Actions returned by [`MainView::handle_key`] that require the parent `Tui`
/// to take async action (open a form, persist a delete, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainViewAction {
    None,
    OpenAddForm {
        vault_id: uuid::Uuid,
    },
    OpenEditForm {
        entry_id: uuid::Uuid,
    },
    DeleteEntry {
        entry_id: uuid::Uuid,
    },
    CopyField {
        entry_id: uuid::Uuid,
        field_idx: usize,
    },
    /// Entry selection changed — caller should fetch and decrypt this entry.
    SelectEntry {
        entry_id: uuid::Uuid,
        vault_id: uuid::Uuid,
    },
}

// ── Column focus ──────────────────────────────────────────────────────────────

/// Which column currently has keyboard focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Column {
    Vaults,
    Entries,
    Detail,
}

impl Column {
    fn next(&self) -> Self {
        match self {
            Column::Vaults => Column::Entries,
            Column::Entries => Column::Detail,
            Column::Detail => Column::Vaults,
        }
    }

    fn prev(&self) -> Self {
        match self {
            Column::Vaults => Column::Detail,
            Column::Entries => Column::Vaults,
            Column::Detail => Column::Entries,
        }
    }
}

// ── MainView ──────────────────────────────────────────────────────────────────

pub struct MainView {
    /// All vaults.  Index 0 corresponds to "All Vaults" (virtual row).
    vaults: Vec<Vault>,
    /// All entries across every vault.
    entries: Vec<Entry>,

    /// Currently focused column.
    pub focused: Column,

    // Col 1 state
    vault_state: ListState,

    // Col 2 state
    entry_state: ListState,

    // Col 3 state — index into the *detail* field list for the selected entry
    detail_field: usize,
    /// Set of field indices in the detail view whose hidden value is revealed.
    revealed: HashSet<usize>,

    // Search / filter state
    /// Current incremental search string (lowercase).
    search_query: String,
    /// Whether the search bar is currently accepting input.
    searching: bool,
    /// Whether the Space leader key has been pressed and we're awaiting the action key.
    leader_mode: bool,
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
        }
    }

    // ── public accessors ──────────────────────────────────────────────────────

    /// Number of vaults loaded (not counting the virtual "All Vaults" row).
    pub fn vault_count(&self) -> usize {
        self.vaults.len()
    }

    /// Whether the Space leader key has been pressed and we're awaiting an action key.
    pub fn is_leader_mode(&self) -> bool {
        self.leader_mode
    }

    /// Whether the search bar is currently accepting input.
    pub fn is_searching(&self) -> bool {
        self.searching
    }

    /// The current search query string (always lowercase).
    pub fn search_query(&self) -> &str {
        &self.search_query
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
    pub fn is_field_revealed(&self, idx: usize) -> bool {
        self.revealed.contains(&idx)
    }

    // ── key handling ─────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyCode) -> MainViewAction {
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
            return match key {
                KeyCode::Char('a') => {
                    let vault_id = match self.vault_state.selected() {
                        None | Some(0) => self.vaults.first().map(|v| v.id),
                        Some(idx) => self.vaults.get(idx - 1).map(|v| v.id),
                    };
                    match vault_id {
                        Some(id) => MainViewAction::OpenAddForm { vault_id: id },
                        None => MainViewAction::None,
                    }
                }
                KeyCode::Char('e') => match self.selected_entry() {
                    Some(e) => MainViewAction::OpenEditForm { entry_id: e.id },
                    None => MainViewAction::None,
                },
                KeyCode::Char('d') => match self.selected_entry() {
                    Some(e) => MainViewAction::DeleteEntry { entry_id: e.id },
                    None => MainViewAction::None,
                },
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
                self.focused = self.focused.prev();
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

    /// Replace the entry list (e.g. after add/edit/delete) and reset selection.
    pub fn reload_entries(&mut self, entries: Vec<bogita_core::domain::Entry>) {        self.entries = entries;
        self.reset_entry_selection();
    }

    /// Replace the entry list and attempt to re-select the entry with `select_id`.
    /// Falls back to index 0 if the id is not found or `select_id` is `None`.
    pub fn reload_entries_select(
        &mut self,
        entries: Vec<bogita_core::domain::Entry>,        select_id: Option<uuid::Uuid>,
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

    fn move_down(&mut self) {
        match self.focused {
            Column::Vaults => {
                let len = self.vaults.len() + 1; // +1 for "All Vaults"
                let cur = self.vault_state.selected().unwrap_or(0);
                let next = (cur + 1).min(len.saturating_sub(1));
                self.vault_state.select(Some(next));
                // Reset entry selection when vault filter changes
                self.reset_entry_selection();
            }
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
            Column::Vaults => {
                let cur = self.vault_state.selected().unwrap_or(0);
                let next = cur.saturating_sub(1);
                self.vault_state.select(Some(next));
                self.reset_entry_selection();
            }
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

    fn reset_entry_selection(&mut self) {
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

    pub fn render_leader_overlay(&self, frame: &mut Frame, area: Rect) {
        // A small centred popup: 3 lines tall, 32 cols wide.
        const W: u16 = 34;
        const H: u16 = 5;
        let x = area.x + area.width.saturating_sub(W) / 2;
        let y = area.y + area.height.saturating_sub(H) / 2;
        let popup = Rect {
            x,
            y,
            width: W.min(area.width),
            height: H.min(area.height),
        };
        let block = Block::default()
            .title(" Actions ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().fg(Color::Yellow));
        let text = vec![
            Line::from(vec![
                Span::styled(
                    " [a] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Add entry"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [e] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Edit entry"),
            ]),
            Line::from(vec![
                Span::styled(
                    " [d] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Delete entry"),
            ]),
        ];
        let para = Paragraph::new(text).block(block);
        frame.render_widget(para, popup);
    }

    // ── rendering ─────────────────────────────────────────────────────────────

    /// Full render: vaults, entries, and detail.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let col3 = self.render_cols_1_2(frame, area);
        self.render_detail(frame, col3, self.selected_entry());
        if self.leader_mode {
            self.render_leader_overlay(frame, area);
        }
    }

    /// Render only Cols 1 (vaults) and 2 (entries), plus the search bar if
    /// active.  Returns the `Rect` reserved for Col 3 so the caller can
    /// render a form or modal there.
    pub fn render_cols_1_2(&mut self, frame: &mut Frame, area: Rect) -> Rect {
        // Reserve a search bar row at the bottom when searching.
        let (main_area, search_area) = if self.searching || !self.search_query.is_empty() {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);
            (rows[0], Some(rows[1]))
        } else {
            (area, None)
        };

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(30),
                Constraint::Percentage(50),
            ])
            .split(main_area);

        self.render_vaults(frame, cols[0]);
        self.render_entries(frame, cols[1]);

        if let Some(bar) = search_area {
            self.render_search_bar(frame, bar);
        }

        cols[2]
    }

    fn render_search_bar(&self, frame: &mut Frame, area: Rect) {
        let prefix = if self.searching { "/ " } else { "  " };
        let style = if self.searching {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let text = format!("{prefix}{}", self.search_query);
        let bar = Paragraph::new(text).style(style);
        frame.render_widget(bar, area);
    }

    fn render_vaults(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focused == Column::Vaults;
        let block = Block::default()
            .title(" Vaults ")
            .borders(Borders::ALL)
            .border_type(if focused {
                BorderType::Double
            } else {
                BorderType::Plain
            });

        let mut items: Vec<ListItem> = vec![ListItem::new("All Vaults")];
        for v in &self.vaults {
            items.push(ListItem::new(v.name.as_str()));
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let state = &mut self.vault_state;
        frame.render_stateful_widget(list, area, state);
    }

    fn render_entries(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focused == Column::Entries;
        let block = Block::default()
            .title(" Entries ")
            .borders(Borders::ALL)
            .border_type(if focused {
                BorderType::Double
            } else {
                BorderType::Plain
            });

        let names: Vec<String> = self
            .visible_entries()
            .iter()
            .map(|e| e.name.clone())
            .collect();
        let items: Vec<ListItem> = names.iter().map(|n| ListItem::new(n.as_str())).collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let state = &mut self.entry_state;
        frame.render_stateful_widget(list, area, state);
    }

    pub fn render_detail(&self, frame: &mut Frame, area: Rect, entry: Option<&Entry>) {
        let focused = self.focused == Column::Detail;
        let Some(entry) = entry else {
            let block = Block::default()
                .title(" Detail ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain);
            let hint = Paragraph::new("Select an entry")
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(hint, area);
            return;
        };

        let title = format!(" {} ", entry.name);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(if focused {
                BorderType::Double
            } else {
                BorderType::Plain
            });
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split inner into per-field rows — one row per field, no extra gauge row.
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                entry
                    .fields
                    .iter()
                    .map(|_| Constraint::Length(2))
                    .chain(std::iter::once(Constraint::Min(1)))
                    .collect::<Vec<_>>(),
            )
            .split(inner);

        let mut row_idx = 0usize;
        for (field_idx, field) in entry.fields.iter().enumerate() {
            let selected = focused && field_idx == self.detail_field;

            let (display_value, is_hidden) = if field.field_type == FieldType::TotpSecret {
                use bogita_core::service::otp::compute_totp;
                let raw = match &field.value {
                    FieldValue::Hidden(s) | FieldValue::Text(s) => s.as_str(),
                    _ => "",
                };
                let code = compute_totp(raw).unwrap_or_else(|| "------".to_string());
                let secs = {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    30 - (now % 30)
                };
                (format!("{code}  ({secs}s)"), false)
            } else {
                let hidden = matches!(field.value, FieldValue::Hidden(_));
                (self.field_display(field_idx, &field.value), hidden)
            };

            self.render_field_row(
                frame,
                rows[row_idx],
                &field.key,
                &display_value,
                selected,
                is_hidden,
            );
            row_idx += 1;
        }

        // Hint bar
        if let Some(hint_area) = rows.get(row_idx) {
            let hints = if focused {
                " [j/k] scroll  [s] reveal  [c] copy  [Tab] columns "
            } else {
                " [Tab] focus detail "
            };
            let hint = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint, *hint_area);
        }
    }

    fn render_field_row(
        &self,
        frame: &mut Frame,
        area: Rect,
        key: &str,
        value: &str,
        selected: bool,
        is_hidden: bool,
    ) {
        let key_style = Style::default().fg(Color::Cyan);
        let val_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let lock = if is_hidden {
            Span::styled(" 🔒", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        };
        let line = Line::from(vec![
            Span::styled(format!("{key:<16} "), key_style),
            Span::styled(value.to_string(), val_style),
            lock,
        ]);
        let prefix = if selected { "▶ " } else { "  " };
        let row = Paragraph::new(line)
            .style(Style::default())
            .block(Block::default().borders(Borders::NONE).title(prefix));
        frame.render_widget(row, area);
    }

    fn field_display(&self, idx: usize, value: &FieldValue) -> String {
        match value {
            FieldValue::Hidden(s) => {
                if self.revealed.contains(&idx) {
                    s.clone()
                } else {
                    "•".repeat(s.len().max(6))
                }
            }
            FieldValue::Text(s) | FieldValue::Url(s) | FieldValue::Email(s) => s.clone(),
            FieldValue::Boolean(b) => b.to_string(),
            FieldValue::Number(n) => n.to_string(),
            FieldValue::Date(ts) => ts.to_string(),
        }
    }

    /// Value to place on the clipboard for `field_idx` in `entry`.
    /// TOTP fields return the calculated code; all others return the raw value.
    pub fn copy_value_for(&self, entry: &Entry, field_idx: usize) -> Option<String> {
        let field = entry.fields.get(field_idx)?;
        if field.field_type == FieldType::TotpSecret {
            use bogita_core::service::otp::compute_totp;
            let raw = match &field.value {
                FieldValue::Hidden(s) | FieldValue::Text(s) => s.as_str(),
                _ => "",
            };
            Some(compute_totp(raw).unwrap_or_else(|| "------".to_string()))
        } else {
            Some(self.field_display(field_idx, &field.value))
        }
    }
}
