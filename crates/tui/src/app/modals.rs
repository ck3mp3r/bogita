//! Centered modal and overlay painters.

use super::bindings;
use rat_widget::text_input::TextInputState;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

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

/// Layout and content of a centered modal.
struct Modal<'a> {
    width: u16,
    height: u16,
    title: Line<'static>,
    border_color: Color,
    /// `None` fills the modal with the message; `Some(height)` adds a fixed
    /// message row plus a hint row.
    message_band: Option<u16>,
    message: Vec<Line<'static>>,
    hint: &'a str,
}

/// Shared frame for the centered modals: clear the area, draw the double border
/// with the given title and colour, then the message band and the hint row.
fn render_centered_modal(frame: &mut Frame, area: Rect, modal: Modal<'_>) {
    let modal_w = modal.width.min(area.width);
    let modal_h = modal.height.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(modal.border_color))
        .title(modal.title);

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let constraints = match modal.message_band {
        Some(rows) => vec![
            Constraint::Length(rows),
            Constraint::Length(1),
            Constraint::Min(1),
        ],
        None => vec![Constraint::Min(1), Constraint::Length(1)],
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    frame.render_widget(Paragraph::new(modal.message), rows[0]);
    frame.render_widget(
        Paragraph::new(Span::styled(
            modal.hint.to_string(),
            Style::default().fg(Color::DarkGray),
        )),
        rows[1],
    );
}

/// Render a centered error modal over the given area.
pub(super) fn render_error_modal(frame: &mut Frame, area: Rect, message: &str) {
    // Modal: up to 60 wide, at least 5 tall (more if message is long).
    let modal_w = 60u16.min(area.width);
    // Wrap message into modal_w - 4 (2 border + 2 padding) columns.
    let inner_w = (modal_w as usize).saturating_sub(4);
    let wrapped = wrap_text(message, inner_w);
    let modal_h = (wrapped.len() as u16 + 4).min(area.height);
    let lines = wrapped
        .into_iter()
        .map(|text| Line::from(Span::styled(text, Style::default().fg(Color::White))))
        .collect();

    render_centered_modal(
        frame,
        area,
        Modal {
            width: modal_w,
            height: modal_h,
            title: Line::from(Span::styled(
                " Error ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            border_color: Color::Red,
            message_band: None,
            message: lines,
            hint: "[Esc / Enter] dismiss",
        },
    );
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
    let lines = vec![Line::from(vec![
        Span::raw("  Delete "),
        Span::styled(
            entry_name.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ])];

    render_centered_modal(
        frame,
        area,
        Modal {
            width: 44,
            height: 5,
            title: Line::from(" Delete entry "),
            border_color: Color::Red,
            message_band: Some(1),
            message: lines,
            hint: "  [d] delete  [c/Esc] cancel",
        },
    );
}

/// Render a small centered confirmation modal for deleting a vault.
pub(super) fn render_confirm_delete_vault_modal(frame: &mut Frame, area: Rect, vault_name: &str) {
    let lines = vec![Line::from(vec![
        Span::raw("  Delete vault "),
        Span::styled(
            vault_name.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ])];

    render_centered_modal(
        frame,
        area,
        Modal {
            width: 44,
            height: 5,
            title: Line::from(" Delete vault "),
            border_color: Color::Red,
            message_band: Some(1),
            message: lines,
            hint: "  [d] delete  [c/Esc] cancel",
        },
    );
}

/// Render a small centered confirmation modal for saving edits.
pub(super) fn render_confirm_save_modal(frame: &mut Frame, area: Rect, entry_name: &str) {
    let lines = vec![Line::from(vec![
        Span::raw("  Save changes to "),
        Span::styled(
            entry_name.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ])];
    let hint = bindings::hint(bindings::CONFIRM_SAVE);

    render_centered_modal(
        frame,
        area,
        Modal {
            width: 44,
            height: 5,
            title: Line::from(" Save changes "),
            border_color: Color::Yellow,
            message_band: Some(1),
            message: lines,
            hint: &hint,
        },
    );
}

/// Render a small centered confirmation modal for discarding changes.
pub(super) fn render_confirm_discard_modal(frame: &mut Frame, area: Rect) {
    let lines =
        vec![Line::from("  You have unsaved changes. Discard?")
            .style(Style::default().fg(Color::White))];
    let hint = bindings::hint(bindings::CONFIRM_DISCARD);

    render_centered_modal(
        frame,
        area,
        Modal {
            width: 44,
            height: 5,
            title: Line::from(" Discard changes "),
            border_color: Color::Yellow,
            message_band: Some(1),
            message: lines,
            hint: &hint,
        },
    );
}

/// Render the lock screen: a centered modal with a masked passphrase prompt.
pub(super) fn render_lock_screen(
    frame: &mut Frame,
    area: Rect,
    passphrase_input: &TextInputState,
    error: Option<&str>,
) {
    let modal_w = 50u16.min(area.width);
    let modal_h = 8u16.min(area.height);
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

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
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

    // Masked passphrase input
    let masked: String = passphrase_input.text().chars().map(|_| '•').collect();
    let input_widget = Paragraph::new(Span::styled(masked, Style::default().fg(Color::Cyan)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(input_widget, rows[1]);

    // Cursor position indicator
    let cursor_pos = passphrase_input.text().len();
    let cursor_text = if cursor_pos > 0 {
        format!("{} chars", cursor_pos)
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            cursor_text,
            Style::default().fg(Color::DarkGray),
        )),
        rows[2],
    );

    // Hint line
    frame.render_widget(
        Paragraph::new(Span::styled(
            "[Enter] unlock  [Esc] quit",
            Style::default().fg(Color::DarkGray),
        )),
        rows[3],
    );
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
