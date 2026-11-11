//! Rendering for the entry form: frame, vault/name rows, field rows, popups.
//!
//! Orchestration only — the field rows are painted by [`RowWidget`] cells.

use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::TextInput;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::field::{FormMode, ValidationIssue};
use super::row::{
    form_rows, key_has_error, render_status_row, render_type_popups, value_has_error, RenderCtx,
    RowWidget,
};
use super::EntryForm;
use crate::app::bindings;

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
            let Some(row) = rows.get(rel_i + 2) else {
                continue;
            };
            let Some(field) = self.fields.get_mut(abs_i) else {
                continue;
            };
            let mut ctx = RenderCtx {
                frame,
                inner,
                badge: field.field_type.label(),
                revealed: self.revealed_secret_fields.contains(&abs_i),
                key_error: key_has_error(&self.validation_errors, abs_i),
                value_error: value_has_error(&self.validation_errors, abs_i),
            };
            RowWidget::new(*row).render(&mut ctx, field);
        }

        render_type_popups(frame, &mut self.fields, inner);

        // The last row carries the form's own feedback — never keys.
        if let Some(row) = rows.last() {
            let summary = if self.has_validation_errors() {
                Some(self.validation_error_summary())
            } else {
                None
            };
            render_status_row(frame, *row, summary);
        }
    }

    /// Draw the modal frame and return its inner area.
    ///
    /// The title carries identity — the entry being edited, or the vault a new
    /// entry goes into — because the footer carries keys and the modal carries
    /// the name.
    fn render_outer(&self, frame: &mut Frame, area: Rect) -> Rect {
        let title = self.modal_title(area.width);

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

    /// The modal title: `\` Edit · name *\`` — identity first, dirty mark last.
    ///
    /// Long identities are truncated to 40% of the modal width with an ellipsis
    /// so the frame never stretches because of a name.
    fn modal_title(&self, width: u16) -> String {
        let dirty_mark = if self.is_dirty() { " *" } else { "" };
        let identity = match self.mode {
            FormMode::Add if !self.vault_name.is_empty() => {
                format!("New · {}", self.vault_name)
            }
            FormMode::Add => "New Entry".to_string(),
            FormMode::Edit if !self.name_state.text().is_empty() => {
                format!("Edit · {}", self.name_state.text())
            }
            FormMode::Edit => "Edit Entry".to_string(),
        };
        let cap = (usize::from(width) * 2 / 5).max(8);
        format!(" {}{dirty_mark} ", truncate(&identity, cap))
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
            format!(" {} ", bindings::form_vault_hint())
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
        } else if name_focused {
            Color::Yellow
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
                    .border_type(BorderType::Plain)
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
}

/// Truncate `text` to `cap` characters, marking the cut with an ellipsis.
fn truncate(text: &str, cap: usize) -> String {
    if text.chars().count() <= cap {
        return text.to_string();
    }
    let mut out: String = text.chars().take(cap.saturating_sub(1)).collect();
    out.push('…');
    out
}
