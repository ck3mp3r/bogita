//! Centered modal and overlay painters.

use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::{TextInput, TextInputState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::bindings::{self, Binding};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Wrap `message` into lines of at most `width` characters, splitting on char
/// boundaries so multibyte characters survive.
pub(super) fn wrap_text(message: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![message.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut count = 0;
    for ch in message.chars() {
        current.push(ch);
        count += 1;
        if count == width {
            lines.push(std::mem::take(&mut current));
            count = 0;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// One centered dialog: a title, a message, and the action rows its own
/// binding table defines.
///
/// Dialogs are widgets — they draw their keys in-box, from the same tables the
/// `?` overlay reads, so a dialog cannot advertise a chord nothing routes and
/// cannot drift from the registry. The footer stays the minimal ` [?] help `.
pub(super) struct ConfirmModal {
    width: u16,
    height: u16,
    title: Line<'static>,
    border_color: Color,
    message: Vec<Line<'static>>,
    table: &'static [Binding],
}

impl ConfirmModal {
    /// The dialog's action row, straight from its surface table.
    fn action_rows(&self) -> String {
        bindings::hint(self.table)
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Hug the action row: a dialog that clips its own keys teaches nothing.
        let needed = self.action_rows().chars().count() as u16 + 4;
        let modal_w = self.width.max(needed).min(area.width);
        let modal_h = self.height.min(area.height);
        let x = area.x + area.width.saturating_sub(modal_w) / 2;
        let y = area.y + area.height.saturating_sub(modal_h) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(self.border_color))
            .title(self.title.clone());
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);

        frame.render_widget(Paragraph::new(self.message.clone()), rows[0]);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} ", self.action_rows()),
                Style::default().fg(Color::DarkGray),
            ))),
            rows[1],
        );
    }

    /// Save-changes dialog.
    pub(super) fn save(entry_name: &str) -> Self {
        Self {
            width: 44,
            height: 5,
            title: Line::from(" Save changes "),
            border_color: Color::Yellow,
            message: vec![Line::from(vec![
                Span::raw("  Save changes to "),
                Span::styled(
                    entry_name.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("?"),
            ])],
            table: bindings::table(bindings::Surface::ConfirmSave),
        }
    }

    /// Discard-changes dialog.
    pub(super) fn discard() -> Self {
        Self {
            width: 44,
            height: 5,
            title: Line::from(" Discard changes "),
            border_color: Color::Yellow,
            message: vec![Line::from("  You have unsaved changes. Discard?")
                .style(Style::default().fg(Color::White))],
            table: bindings::table(bindings::Surface::ConfirmDiscard),
        }
    }

    /// Delete-entry dialog.
    pub(super) fn delete_entry(entry_name: &str) -> Self {
        Self {
            width: 44,
            height: 5,
            title: Line::from(" Delete entry "),
            border_color: Color::Red,
            message: vec![Line::from(vec![
                Span::raw("  Delete "),
                Span::styled(
                    entry_name.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("?"),
            ])],
            table: bindings::table(bindings::Surface::ConfirmDelete),
        }
    }

    /// Delete-vault dialog.
    pub(super) fn delete_vault(vault_name: &str) -> Self {
        Self {
            width: 44,
            height: 5,
            title: Line::from(" Delete vault "),
            border_color: Color::Red,
            message: vec![Line::from(vec![
                Span::raw("  Delete vault "),
                Span::styled(
                    vault_name.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("?"),
            ])],
            table: bindings::table(bindings::Surface::ConfirmDelete),
        }
    }

    /// Error dialog — as wide as the message needs, one row of dismiss keys.
    pub(super) fn error(message: &str, area_width: u16, area_height: u16) -> Self {
        let width = 60u16.min(area_width);
        let inner_w = usize::from(width).saturating_sub(4);
        let wrapped = wrap_text(message, inner_w);
        let height = (wrapped.len() as u16 + 4).min(area_height);
        Self {
            width,
            height,
            title: Line::from(Span::styled(
                " Error ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            border_color: Color::Red,
            message: wrapped
                .into_iter()
                .map(|text| Line::from(Span::styled(text, Style::default().fg(Color::White))))
                .collect(),
            table: bindings::table(bindings::Surface::ErrorOverlay),
        }
    }
}

/// Render a centered error modal over the given area.
pub(super) fn render_error_modal(frame: &mut Frame, area: Rect, message: &str) {
    ConfirmModal::error(message, area.width, area.height).render(frame, area);
}

/// The entry-form modal rect: identity-width, hugging its content.
///
/// Width is capped at 60% of the body and never below 40 columns. Height follows
/// the row count — vault row, name row, one three-line row per field, the hint
/// row and the two border rows — capped at 90% of the body, so the modal never
/// keeps dead rows below the last field.
pub fn form_modal_rect(body: Rect, field_count: usize) -> Rect {
    let min_width = 40.min(body.width);
    let width = (body.width * 6 / 10).max(min_width).min(body.width);
    let content = 1 + 3 + 3 * field_count as u16 + 1 + 2;
    let height = content.min((body.height * 9 / 10).max(1)).min(body.height);
    let x = body.x + body.width.saturating_sub(width) / 2;
    let y = body.y + body.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

/// Compute a centered rectangle within `area` at the given width and height
/// percentages. The result is centered both horizontally and vertically.
pub fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render a small centered confirmation modal over the full terminal area.
pub(super) fn render_confirm_delete_modal(frame: &mut Frame, area: Rect, entry_name: &str) {
    ConfirmModal::delete_entry(entry_name).render(frame, area);
}

/// Render a small centered confirmation modal for deleting a vault.
pub(super) fn render_confirm_delete_vault_modal(frame: &mut Frame, area: Rect, vault_name: &str) {
    ConfirmModal::delete_vault(vault_name).render(frame, area);
}

/// Render a small centered confirmation modal for saving edits.
pub(super) fn render_confirm_save_modal(frame: &mut Frame, area: Rect, entry_name: &str) {
    ConfirmModal::save(entry_name).render(frame, area);
}

/// Render a small centered confirmation modal for discarding changes.
pub(super) fn render_confirm_discard_modal(frame: &mut Frame, area: Rect) {
    ConfirmModal::discard().render(frame, area);
}

/// Render the lock screen: a centered modal with a masked passphrase prompt.
pub(super) fn render_lock_screen(
    frame: &mut Frame,
    area: Rect,
    passphrase_input: &mut TextInputState,
    error: Option<&str>,
) {
    let modal_w = 50u16.min(area.width);
    let modal_h = 7u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let border_color = if error.is_some() {
        Color::Red
    } else {
        Color::Yellow
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Locked ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // The input is three rows — its two borders and one content row — because a
    // bordered cell paints nothing at all in a single row, and the mask lives in
    // that content row.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    // Error text
    if let Some(err) = error {
        frame.render_widget(
            Paragraph::new(Span::styled(err, Style::default().fg(Color::Red))),
            rows[0],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Enter passphrase:",
                Style::default().fg(Color::White),
            )),
            rows[0],
        );
    }

    // Masked passphrase input: the same widget the form's obscured cells use, so
    // the mask glyphs and the caret come from the input itself.
    //
    // The passphrase cell is the lock screen's only input, so it always holds
    // focus: that is what paints the focused edge and the screen cursor.
    passphrase_input.focus.set(true);
    let input_widget = TextInput::new()
        .style(Style::default().fg(Color::Cyan))
        .focus_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .passwd()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_stateful_widget(input_widget, rows[1], &mut *passphrase_input);
    if let Some((cx, cy)) = passphrase_input.screen_cursor() {
        frame.set_cursor_position((cx, cy));
    }
}

/// Render a vault picker dropdown overlay on top of the entry form.
pub(super) fn render_form_vault_picker(
    frame: &mut Frame,
    area: Rect,
    vaults: &[bogita_core::domain::Vault],
    state: &mut ListState,
) {
    let width = 26u16.min(area.width);
    let height = (vaults.len() as u16 + 4).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let dropdown = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dropdown);

    let block = Block::default()
        .title(" Vault ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(dropdown);
    frame.render_widget(block, dropdown);

    let mut items: Vec<ListItem> = vec![ListItem::new("All Vaults")];
    for v in vaults {
        items.push(ListItem::new(v.name.as_str()));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, inner, state);
}
