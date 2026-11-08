//! Rendering — leader overlay, vault picker, entry columns, and detail rows.

use super::actions::Column;
use super::state::MainView;
use bogita_core::domain::{Entry, FieldType, FieldValue};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

impl MainView {
    pub fn render_leader_overlay(&self, frame: &mut Frame, area: Rect) {
        // Determine height and content based on focused column
        let (h, lines): (u16, Vec<Line>) = match self.focused {
            Column::Entries => {
                let lines = vec![
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
                (5, lines)
            }
            Column::Detail => {
                let lines = vec![Line::from(Span::styled(
                    " No actions available ",
                    Style::default().fg(Color::DarkGray),
                ))];
                (3, lines)
            }
        };

        const W: u16 = 34;
        let x = area.x + area.width.saturating_sub(W) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let popup = Rect {
            x,
            y,
            width: W.min(area.width),
            height: h.min(area.height),
        };
        let block = Block::default()
            .title(" Actions ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default().fg(Color::Yellow));
        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, popup);
    }

    pub fn render_vault_picker(&mut self, frame: &mut Frame, area: Rect) {
        // Dropdown anchored at top-left of the entries column area
        let width = 26u16.min(area.width);
        let height = (self.vault_count() as u16 + 4).min(area.height); // border + title + items + hint
        let dropdown = Rect::new(area.x, area.y, width, height);

        // Clear the area beneath the dropdown
        frame.render_widget(Clear, dropdown);

        let block = Block::default()
            .title(" Vault ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(dropdown);
        frame.render_widget(block, dropdown);

        // Build the vault list
        let mut items: Vec<ListItem> = vec![ListItem::new("All Vaults")];
        for v in &self.vaults {
            items.push(ListItem::new(v.name.as_str()));
        }

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, inner, &mut self.vault_state);
    }

    // ── rendering ─────────────────────────────────────────────────────────────

    /// Render only Col 1 (entries), plus the search bar if
    /// active.  Returns the `Rect` reserved for Col 2 so the caller can
    /// render a form or modal there.
    pub fn render_cols_1_2(&mut self, frame: &mut Frame, area: Rect) -> Rect {
        // Reserve a search bar row at the bottom when searching.
        let (main_area, search_area) = if self.is_searching() || !self.search_query().is_empty() {
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
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(main_area);

        self.render_entries(frame, cols[0]);

        if let Some(bar) = search_area {
            self.render_search_bar(frame, bar);
        }

        cols[1]
    }

    fn render_search_bar(&self, frame: &mut Frame, area: Rect) {
        let prefix = if self.is_searching() { "/ " } else { "  " };
        let style = if self.is_searching() {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let text = format!("{prefix}{}", self.search_query());
        let bar = Paragraph::new(text).style(style);
        frame.render_widget(bar, area);
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
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let (code, secs) = entry
                    .totp_code(&field.key, now_secs)
                    .unwrap_or_else(|| ("------".to_string(), 0));
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
            FieldValue::Hidden(secret) if !self.is_field_revealed(idx) => {
                "•".repeat(secret.len().max(6))
            }
            _ => value.to_string(),
        }
    }

    /// Value to place on the clipboard for `field_idx` in `entry`.
    /// TOTP fields return the calculated code; all others return the raw value.
    pub fn copy_value_for(&self, entry: &Entry, field_idx: usize) -> Option<String> {
        let field = entry.fields.get(field_idx)?;
        if field.field_type == FieldType::TotpSecret {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(
                entry
                    .totp_code(&field.key, now_secs)
                    .map(|(code, _)| code)
                    .unwrap_or_else(|| "------".to_string()),
            )
        } else {
            Some(self.field_display(field_idx, &field.value))
        }
    }
}
