//! Row composition: one field row as three cells, each painted by its own
//! widget.
//!
//! `RowWidget` owns the row and column layout; the cells own the painting. The
//! value cell routes through the field type, so the paint path follows the same
//! map as the validation widgets in `widgets.rs`.

use rat_popup::Placement;
use rat_widget::choice::Choice;
use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::TextInput;
use rat_widget::textarea::TextArea;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use super::field::{FormField, FormFieldType, ValidationIssue};

// region:    --- Types

/// Everything a row's cells read, computed once per row.
pub(super) struct RenderCtx<'a, 'f> {
    pub(super) frame: &'a mut Frame<'f>,
    /// The form's inner area — the type popup's boundary.
    pub(super) inner: Rect,
    /// The field type label shown in the value cell's badge.
    pub(super) badge: &'static str,
    /// Whether this field's secret value is currently revealed.
    pub(super) revealed: bool,
    /// `EmptyKey`/`DuplicateKey` for this row.
    pub(super) key_error: bool,
    /// `InvalidSshKey`/`InvalidTotp` for this row.
    pub(super) value_error: bool,
}

/// One cell of a field row.
pub(super) trait CellWidget {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField);
}

/// The three cells of one row, in painting order.
pub(super) enum FieldCell {
    Key(KeyCell),
    Type(TypeCell),
    Value(ValueCell),
}

impl FieldCell {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField) {
        match self {
            FieldCell::Key(cell) => cell.render(ctx, field),
            FieldCell::Type(cell) => cell.render(ctx, field),
            FieldCell::Value(cell) => cell.render(ctx, field),
        }
    }
}

pub(super) struct KeyCell {
    area: Rect,
}

pub(super) struct TypeCell {
    area: Rect,
}

pub(super) struct ValueCell {
    area: Rect,
}

/// How a value cell paints, derived from the field type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Notes,
    Obscured,
    Plain,
}

/// One field row: the three cells it paints, in painting order.
pub(super) struct RowWidget {
    cells: [FieldCell; 3],
}

// endregion: --- Types

// region:    --- RowWidget

impl RowWidget {
    /// Split one row into the three cells — [key 35%] [type 15%] [value 50%].
    pub(super) fn new(row: Rect) -> Self {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(35),
                Constraint::Percentage(15),
                Constraint::Percentage(50),
            ])
            .split(row);
        Self {
            cells: [
                FieldCell::Key(KeyCell { area: cols[0] }),
                FieldCell::Type(TypeCell { area: cols[1] }),
                FieldCell::Value(ValueCell { area: cols[2] }),
            ],
        }
    }

    /// Paint every cell of the row through its widget.
    pub(super) fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField) {
        for cell in &self.cells {
            cell.render(ctx, field);
        }
    }
}

// endregion: --- RowWidget

// region:    --- Cells

impl CellWidget for KeyCell {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField) {
        let key_f = field.key_state.focus.get();
        let key_border_color = if ctx.key_error {
            Color::Red
        } else if key_f {
            Color::Yellow
        } else {
            Color::default()
        };
        let key_title = if ctx.key_error { " Key ⚠ " } else { " Key " };
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
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(key_border_color))
                    .title(key_title),
            );
        ctx.frame
            .render_stateful_widget(key_widget, self.area, &mut field.key_state);
        if key_f {
            if let Some((cx, cy)) = field.key_state.screen_cursor() {
                ctx.frame.set_cursor_position((cx, cy));
            }
        }
    }
}

impl CellWidget for TypeCell {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField) {
        let type_f = field.type_state.focus.get();
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
                    .border_type(BorderType::Plain)
                    .border_style(if type_f {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    })
                    .title(" Type "),
            )
            .popup_block(Block::default().borders(Borders::ALL))
            .popup_placement(Placement::AboveOrBelow)
            .popup_boundary(ctx.inner)
            .into_widgets();
        // Rendering the cell records the anchor the popup pass uses later.
        ctx.frame
            .render_stateful_widget(type_widget, self.area, &mut field.type_state);
    }
}

impl CellWidget for ValueCell {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, field: &mut FormField) {
        let kind = value_kind(field.field_type.clone());
        let val_f = if kind == ValueKind::Notes {
            field.textarea_state.as_ref().is_some_and(|s| s.focus.get())
        } else {
            field.value_state.focus.get()
        };
        let suffix = match (ctx.revealed, ctx.value_error) {
            (true, true) => " [revealed] ⚠ ",
            (true, false) => " [revealed] ",
            (false, true) => " ⚠ ",
            (false, false) => " ",
        };
        let title = format!(" Value {}{suffix}", ctx.badge);

        match kind {
            ValueKind::Notes => self.paint_notes(ctx, field, &title, val_f),
            ValueKind::Obscured => self.paint_obscured(ctx, field, &title, val_f),
            ValueKind::Plain => self.paint_plain(ctx, field, &title, val_f),
        }
    }
}

impl ValueCell {
    /// Notes value cell — a TextArea, falling back to plain text when the
    /// TextAreaState is missing.
    fn paint_notes(
        &self,
        ctx: &mut RenderCtx<'_, '_>,
        field: &mut FormField,
        title: &str,
        val_f: bool,
    ) {
        let error = ctx.value_error;
        if let Some(state) = field.textarea_state.as_mut() {
            let textarea = TextArea::new()
                .style(if val_f {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                })
                .block(value_block(title, val_f, error));
            ctx.frame.render_stateful_widget(textarea, self.area, state);
            if val_f {
                if let Some((cx, cy)) = state.screen_cursor() {
                    ctx.frame.set_cursor_position((cx, cy));
                }
            }
        } else {
            let val_widget = plain_value_input(title, val_f, error);
            ctx.frame
                .render_stateful_widget(val_widget, self.area, &mut field.value_state);
            if val_f {
                if let Some((cx, cy)) = field.value_state.screen_cursor() {
                    ctx.frame.set_cursor_position((cx, cy));
                }
            }
        }
    }

    /// Obscured value cell — masked unless the field is revealed.
    fn paint_obscured(
        &self,
        ctx: &mut RenderCtx<'_, '_>,
        field: &mut FormField,
        title: &str,
        val_f: bool,
    ) {
        let mut val_widget = plain_value_input(title, val_f, ctx.value_error);
        if !ctx.revealed {
            val_widget = val_widget.passwd();
        }
        ctx.frame
            .render_stateful_widget(val_widget, self.area, &mut field.value_state);
        if val_f {
            if let Some((cx, cy)) = field.value_state.screen_cursor() {
                ctx.frame.set_cursor_position((cx, cy));
            }
        }
    }

    /// Plain value cell.
    fn paint_plain(
        &self,
        ctx: &mut RenderCtx<'_, '_>,
        field: &mut FormField,
        title: &str,
        val_f: bool,
    ) {
        let val_widget = plain_value_input(title, val_f, ctx.value_error);
        ctx.frame
            .render_stateful_widget(val_widget, self.area, &mut field.value_state);
        if val_f {
            if let Some((cx, cy)) = field.value_state.screen_cursor() {
                ctx.frame.set_cursor_position((cx, cy));
            }
        }
    }
}

// endregion: --- Cells

// region:    --- Support

/// Vertical layout: vault row, name row, one row per visible field, one status
/// row for the form's own validation feedback (never for keys).
pub(super) fn form_rows(inner: Rect, field_rows: usize) -> Vec<Rect> {
    let row_count = 1 + 1 + field_rows + 1;
    let constraints: Vec<Constraint> = (0..row_count)
        .map(|i| {
            if i == 0 {
                // Vault name row — 1 line
                Constraint::Length(1)
            } else if i == row_count - 1 {
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

/// The paint path for each form field type.
///
/// Exhaustive on purpose: a new `FormFieldType` variant does not compile until
/// it names its paint path, mirroring the widget map in `widgets.rs`.
fn value_kind(field_type: FormFieldType) -> ValueKind {
    match field_type {
        FormFieldType::Notes => ValueKind::Notes,
        FormFieldType::Token | FormFieldType::Totp | FormFieldType::SshKey => ValueKind::Obscured,
        FormFieldType::Username
        | FormFieldType::SshPublicKey
        | FormFieldType::Text
        | FormFieldType::Boolean => ValueKind::Plain,
    }
}

/// `true` when this row's key carries a structural error.
pub(super) fn key_has_error(
    errors: &std::collections::HashSet<ValidationIssue>,
    index: usize,
) -> bool {
    errors.contains(&ValidationIssue::EmptyKey(index))
        || errors.contains(&ValidationIssue::DuplicateKey(index))
}

/// `true` when this row's value carries a value-format error.
pub(super) fn value_has_error(
    errors: &std::collections::HashSet<ValidationIssue>,
    index: usize,
) -> bool {
    errors.contains(&ValidationIssue::InvalidSshKey(index))
        || errors.contains(&ValidationIssue::InvalidTotp(index))
}

/// The bordered block shared by every value cell.
///
/// Children draw single borders: the modal frame is the only double border, so
/// focus shows as a yellow edge and an error as a red one.
fn value_block<'a>(title: &'a str, focused: bool, error: bool) -> Block<'a> {
    let color = if error {
        Color::Red
    } else if focused {
        Color::Yellow
    } else {
        Color::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(color))
        .title(title)
}

/// A plain (non-masked) value input.
fn plain_value_input<'a>(title: &'a str, focused: bool, error: bool) -> TextInput<'a> {
    TextInput::new()
        .style(Style::default())
        .focus_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .block(value_block(title, focused, error))
}

/// The field-type selector, shared by the inline cell and the open popup.
///
/// The items derive from [`FormFieldType::TABLE`], so the picker order and the
/// labels cannot drift from the type list.
fn type_selector() -> Choice<'static, usize> {
    FormFieldType::TABLE
        .iter()
        .enumerate()
        .fold(Choice::new(), |choice, (index, (_, label, _))| {
            choice.item(index, *label)
        })
}

/// Render popups for any fields with an active popup. Popups must be rendered
/// AFTER all other widgets so they overlay, anchored to the area the type cell
/// recorded while painting.
pub(super) fn render_type_popups(frame: &mut Frame, fields: &mut [FormField], inner: Rect) {
    for ff in fields.iter_mut() {
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

/// The form's own feedback row — the refused-save reason, or nothing. It carries
/// no chords: the keys live behind `?`.
pub(super) fn render_status_row(frame: &mut Frame, row: Rect, summary: Option<String>) {
    if let Some(text) = summary {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!(" ⚠ {text}"),
                Style::default().fg(Color::Red),
            )])),
            row,
        );
    }
}

// endregion: --- Support
