//! Rendering for the entry form: frame, vault/name rows, field rows, popups.

use rat_popup::Placement;
use rat_widget::choice::Choice;
use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::TextInput;
use rat_widget::textarea::TextArea;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::field::{FormFieldType, FormMode, ValidationIssue};
use super::EntryForm;

impl EntryForm {
    // ── rendering ─────────────────────────────────────────────────────────────

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.render_outer(frame, area);

        let visible = self.visible_rows(inner.height);
        self.last_visible_rows = visible;
        let end = self.window_end(visible);
        let field_rows = end - self.scroll_offset;

        let rows = form_rows(inner, field_rows);

        if let Some(row) = rows.first() {
            self.render_vault_row(frame, *row);
        }
        if let Some(row) = rows.get(1) {
            self.render_name_row(frame, *row);
        }
        for rel_i in 0..field_rows {
            let abs_i = self.scroll_offset + rel_i;
            if let Some(row) = rows.get(rel_i + 2) {
                self.render_field_row(frame, *row, abs_i, inner);
            }
        }

        self.render_type_popups(frame, inner);

        if let Some(row) = rows.last() {
            self.render_hint_bar(frame, *row, end);
        }
    }

    /// Draw the double-bordered overlay and return its inner area.
    fn render_outer(&self, frame: &mut Frame, area: Rect) -> Rect {
        let dirty_mark = if self.is_dirty() { " *" } else { "" };
        let title = match self.mode {
            FormMode::Add => format!(" New Entry{dirty_mark} "),
            FormMode::Edit => format!(" Edit Entry{dirty_mark} "),
        };

        // Clear the area first to prevent background content from showing through.
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(title);
        let inner = outer.inner(area);
        frame.render_widget(outer, area);
        inner
    }

    /// Clamp `scroll_offset` to the visible range and return the index one past
    /// the last field to render.
    fn window_end(&mut self, visible: usize) -> usize {
        if self.fields.len() > visible {
            self.scroll_offset = self.scroll_offset.min(self.fields.len() - visible);
        } else {
            self.scroll_offset = 0;
        }
        (self.scroll_offset + visible).min(self.fields.len())
    }

    /// Row 0: vault name (read-only display).
    fn render_vault_row(&self, frame: &mut Frame, row: Rect) {
        let vault_text = if self.vault_name.is_empty() {
            " Vault: (none) ".to_string()
        } else {
            format!(" Vault: {} ", self.vault_name)
        };
        let vault_hint = if self.vault_name.is_empty() {
            String::new()
        } else {
            " [v] change vault ".to_string()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    &vault_text,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(vault_hint, Style::default().fg(Color::DarkGray)),
            ])),
            row,
        );
    }

    /// Row 1: entry name.
    fn render_name_row(&mut self, frame: &mut Frame, row: Rect) {
        let name_focused = self.name_state.focus.get();
        let name_has_error = self.validation_errors.contains(&ValidationIssue::EmptyName);
        let name_border_color = if name_has_error {
            Color::Red
        } else {
            Color::default()
        };
        let name_title = if name_has_error {
            " Name ⚠ "
        } else {
            " Name "
        };
        let name_widget = TextInput::new()
            .style(Style::default())
            .focus_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(if name_focused {
                        BorderType::Double
                    } else {
                        BorderType::Plain
                    })
                    .border_style(Style::default().fg(name_border_color))
                    .title(name_title),
            );
        frame.render_stateful_widget(name_widget, row, &mut self.name_state);
        if self.name_state.focus.get() {
            if let Some((cx, cy)) = self.name_state.screen_cursor() {
                frame.set_cursor_position((cx, cy));
            }
        }
    }

    /// A field row — [key 35%] [value 50%] [type 15%].
    fn render_field_row(&mut self, frame: &mut Frame, row: Rect, abs_i: usize, inner: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(35),
                Constraint::Percentage(50),
                Constraint::Percentage(15),
            ])
            .split(row);

        self.render_key_cell(frame, cols[0], abs_i);
        self.render_value_cell(frame, cols[1], abs_i);
        self.render_type_cell(frame, cols[2], abs_i, inner);
    }

    fn render_key_cell(&mut self, frame: &mut Frame, area: Rect, abs_i: usize) {
        let key_has_error = self
            .validation_errors
            .contains(&ValidationIssue::EmptyKey(abs_i))
            || self
                .validation_errors
                .contains(&ValidationIssue::DuplicateKey(abs_i));
        let Some(ff) = self.fields.get_mut(abs_i) else {
            return;
        };
        let key_f = ff.key_state.focus.get();
        let key_border_color = if key_has_error {
            Color::Red
        } else {
            Color::default()
        };
        let key_title = if key_has_error { " Key ⚠ " } else { " Key " };
        let key_widget = TextInput::new()
            .style(Style::default())
            .focus_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(if key_f {
                        BorderType::Double
                    } else {
                        BorderType::Plain
                    })
                    .border_style(Style::default().fg(key_border_color))
                    .title(key_title),
            );
        frame.render_stateful_widget(key_widget, area, &mut ff.key_state);
        if key_f {
            if let Some((cx, cy)) = ff.key_state.screen_cursor() {
                frame.set_cursor_position((cx, cy));
            }
        }
    }

    /// Value cell — the badge in the title shows the field type label.
    fn render_value_cell(&mut self, frame: &mut Frame, area: Rect, abs_i: usize) {
        let (is_notes, is_obscured, val_f, badge, is_revealed) = {
            let Some(ff) = self.fields.get_mut(abs_i) else {
                return;
            };
            (
                ff.field_type == FormFieldType::Notes,
                ff.field_type.is_obscured(),
                if ff.field_type == FormFieldType::Notes {
                    ff.textarea_state.as_ref().is_some_and(|s| s.focus.get())
                } else {
                    ff.value_state.focus.get()
                },
                ff.field_type.label(),
                self.revealed_secret_fields.contains(&abs_i),
            )
        };
        let value_title = if is_revealed {
            format!(" Value {badge} [revealed] ")
        } else {
            format!(" Value {badge} ")
        };

        if is_notes {
            self.render_notes_value(frame, area, abs_i, &value_title, val_f);
        } else if is_obscured {
            self.render_obscured_value(frame, area, abs_i, &value_title, val_f, is_revealed);
        } else {
            self.render_plain_value(frame, area, abs_i, &value_title, val_f);
        }
    }

    /// Notes value cell — a TextArea, falling back to plain text when the
    /// TextAreaState is missing.
    fn render_notes_value(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        abs_i: usize,
        title: &str,
        val_f: bool,
    ) {
        let Some(ff) = self.fields.get_mut(abs_i) else {
            return;
        };
        if let Some(state) = ff.textarea_state.as_mut() {
            let textarea = TextArea::new()
                .style(if val_f {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                })
                .block(value_block(title, val_f));
            frame.render_stateful_widget(textarea, area, state);
            if val_f {
                if let Some((cx, cy)) = state.screen_cursor() {
                    frame.set_cursor_position((cx, cy));
                }
            }
        } else {
            let val_widget = plain_value_input(title, val_f);
            frame.render_stateful_widget(val_widget, area, &mut ff.value_state);
            if val_f {
                if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                    frame.set_cursor_position((cx, cy));
                }
            }
        }
    }

    /// Obscured value cell — masked unless the field is revealed.
    fn render_obscured_value(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        abs_i: usize,
        title: &str,
        val_f: bool,
        is_revealed: bool,
    ) {
        let Some(ff) = self.fields.get_mut(abs_i) else {
            return;
        };
        let mut val_widget = plain_value_input(title, val_f);
        if !is_revealed {
            val_widget = val_widget.passwd();
        }
        frame.render_stateful_widget(val_widget, area, &mut ff.value_state);
        if val_f {
            if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                frame.set_cursor_position((cx, cy));
            }
        }
    }

    /// Plain value cell.
    fn render_plain_value(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        abs_i: usize,
        title: &str,
        val_f: bool,
    ) {
        let Some(ff) = self.fields.get_mut(abs_i) else {
            return;
        };
        let val_widget = plain_value_input(title, val_f);
        frame.render_stateful_widget(val_widget, area, &mut ff.value_state);
        if val_f {
            if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                frame.set_cursor_position((cx, cy));
            }
        }
    }

    /// Type cell — uses rat-widget Choice for the selector.
    fn render_type_cell(&mut self, frame: &mut Frame, area: Rect, abs_i: usize, inner: Rect) {
        let Some(ff) = self.fields.get_mut(abs_i) else {
            return;
        };
        let type_f = ff.type_state.focus.get();
        let (type_widget, _) = type_selector()
            .style(Style::default())
            .focus_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(if type_f {
                        BorderType::Double
                    } else {
                        BorderType::Plain
                    })
                    .border_style(if type_f {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    })
                    .title(" Type "),
            )
            .popup_block(Block::default().borders(Borders::ALL))
            .popup_placement(Placement::AboveOrBelow)
            .popup_boundary(inner)
            .into_widgets();
        frame.render_stateful_widget(type_widget, area, &mut ff.type_state);
    }

    /// Render popups for any fields with an active popup. Popups must be
    /// rendered AFTER all other widgets so they overlay.
    fn render_type_popups(&mut self, frame: &mut Frame, inner: Rect) {
        for ff in self.fields.iter_mut() {
            if ff.type_state.is_popup_active() {
                let (_, popup) = type_selector()
                    .popup_block(Block::default().borders(Borders::ALL))
                    .popup_placement(Placement::AboveOrBelow)
                    .popup_boundary(inner)
                    .into_widgets();
                frame.render_stateful_widget(popup, ff.type_state.area, &mut ff.type_state);
            }
        }
    }

    /// Hint bar — field management keys plus scroll indicators.
    fn render_hint_bar(&self, frame: &mut Frame, row: Rect, end: usize) {
        let mut hint_text = " [+] add field  [-] remove field ".to_string();
        if self.scroll_offset > 0 {
            hint_text.push_str(" ↑ ");
        }
        if end < self.fields.len() {
            hint_text.push_str(" ↓ ");
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                hint_text,
                Style::default().fg(Color::DarkGray),
            )])),
            row,
        );
    }
}

/// Vertical layout: vault row, name row, one row per visible field, hint row.
fn form_rows(inner: Rect, field_rows: usize) -> Vec<Rect> {
    let row_count = 1 + 1 + field_rows + 1;
    let constraints: Vec<Constraint> = (0..row_count)
        .map(|i| {
            if i == row_count - 1 {
                Constraint::Length(1)
            } else if i == 0 {
                // Vault name row — 1 line
                Constraint::Length(1)
            } else {
                Constraint::Length(3)
            }
        })
        .collect();

    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner)
        .to_vec()
}

/// The bordered block shared by every value cell.
fn value_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .title(title)
}

/// A plain (non-masked) value input.
fn plain_value_input<'a>(title: &'a str, focused: bool) -> TextInput<'a> {
    TextInput::new()
        .style(Style::default())
        .focus_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .block(value_block(title, focused))
}

/// The field-type selector, shared by the inline cell and the open popup.
fn type_selector() -> Choice<'static, usize> {
    Choice::new()
        .item(0, "text")
        .item(1, "username")
        .item(2, "token")
        .item(3, "totp")
        .item(4, "ssh-key")
        .item(5, "notes")
}
