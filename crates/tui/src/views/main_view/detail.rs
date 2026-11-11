//! Detail pane — one entry as composed sections in a fixed label gutter.
//!
//! The pane adds the context the entry list cannot carry (key-type badge, the
//! agent-servable pill, created/modified stamps) and shows data only: the keys
//! that act on it live in the footer. Composition mirrors the form's row cells —
//! an enum of cells, each painted by its own widget, no trait objects.

use super::actions::Column;
use super::detail_layout::CategoryLayout;
use super::state::MainView;
use bogita_core::domain::{Entry, Field, FieldType, FieldValue, AGENT_SERVABLE_KEY};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

// region:    --- Constants

/// Mask width. Fixed on purpose: a proportional run leaks the secret length.
pub(super) const MASK_GLYPHS: usize = 10;

/// Label column — the 2-column selection prefix plus a 14-column label. The
/// value column starts at this offset on every row, which is the grid.
const LABEL_GUTTER: u16 = 16;
const PREFIX_WIDTH: usize = 2;
const LABEL_WIDTH: usize = LABEL_GUTTER as usize - PREFIX_WIDTH;

const SECTION_INDENT: &str = " ";
const ROW_INDENT: &str = "  ";
const SELECTED_INDENT: &str = "▶ ";

// endregion: --- Constants

// region:    --- Types

/// Everything a cell reads while painting.
pub(super) struct RenderCtx<'a, 'f> {
    pub(super) frame: &'a mut Frame<'f>,
}

/// One cell of the pane.
pub(super) trait CellWidget {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, area: Rect);
}

/// The pane as an ordered list of cells, painted top-down.
pub(super) enum DetailCell {
    Section(SectionHeader),
    Field(FieldRow),
    Meta(MetaRow),
}

impl DetailCell {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, area: Rect) {
        match self {
            DetailCell::Section(cell) => cell.render(ctx, area),
            DetailCell::Field(cell) => cell.render(ctx, area),
            DetailCell::Meta(cell) => cell.render(ctx, area),
        }
    }
}

/// A section header — plain CAPS row, never a border title: the pane border
/// already carries ` Detail `.
pub(super) struct SectionHeader {
    title: &'static str,
    /// Rendered as ` · BADGE` — the key type rides here instead of a row.
    badge: Option<String>,
}

/// One field row: label in the gutter, value in the remaining columns.
pub(super) struct FieldRow {
    label: String,
    value: RowValue,
    selected: bool,
    revealed: bool,
}

/// How a field row paints its value column.
enum RowValue {
    Text {
        text: String,
        masked: bool,
    },
    /// A derived value: the live TOTP code and its countdown, never masked.
    LiveCode {
        code: String,
        secs: u64,
    },
    Pill(bool),
}

/// A metadata row — created, modified. Not selectable: it carries no field.
pub(super) struct MetaRow {
    label: String,
    text: String,
}

impl MainView {
    /// Compose the pane for `entry`.
    ///
    /// The layout follows the entry's field inventory — the per-field truth the
    /// daemon and git-signing helpers key on, never the stored category column:
    /// sections for the SSH material, credentials, OTP and notes the fields
    /// actually hold, the entry's own extra fields under `OTHER FIELDS` when it
    /// has any, then metadata. Every group is skipped when empty. Top anchored:
    /// no filler rows, the space below the last row stays empty.
    pub(super) fn detail_cells(&self, entry: &Entry) -> Vec<DetailCell> {
        let focused = self.focused == Column::Detail;
        let layout = CategoryLayout::of(entry);

        let mut cells = Vec::new();
        for section in layout.sections {
            cells.push(DetailCell::Section(SectionHeader {
                title: section.title,
                badge: section.badge,
            }));
            for (idx, field) in section.rows {
                cells.push(DetailCell::Field(
                    self.field_row(entry, idx, field, focused),
                ));
            }
        }

        // METADATA always renders: the stamps are entry data, not fields.
        cells.push(DetailCell::Section(SectionHeader {
            title: "METADATA",
            badge: None,
        }));
        for (idx, field) in entry
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.key == AGENT_SERVABLE_KEY)
        {
            cells.push(DetailCell::Field(
                self.field_row(entry, idx, field, focused),
            ));
        }
        cells.push(stamp_row("created", entry.created_at));
        cells.push(stamp_row("modified", entry.modified_at));
        cells
    }

    /// One field row, with its value already resolved to what may be shown.
    fn field_row(&self, entry: &Entry, idx: usize, field: &Field, focused: bool) -> FieldRow {
        let revealed = self.is_field_revealed(idx);
        let hidden = matches!(field.value, FieldValue::Hidden(_));
        let value = if field.key == AGENT_SERVABLE_KEY {
            RowValue::Pill(matches!(field.value, FieldValue::Boolean(true)))
        } else if field.field_type == FieldType::TotpSecret {
            // The code is the daily-use object; the seed is an edit-time field.
            match entry.totp_code(&field.key, epoch_secs()) {
                Some((code, secs)) => RowValue::LiveCode { code, secs },
                None => RowValue::Text {
                    text: masked_secret(),
                    masked: true,
                },
            }
        } else if hidden && !revealed {
            RowValue::Text {
                text: masked_secret(),
                masked: true,
            }
        } else {
            RowValue::Text {
                text: flatten(&field.value.to_string()),
                masked: false,
            }
        };
        FieldRow {
            label: label_for(field),
            value,
            selected: focused && idx == self.detail_field,
            revealed: hidden && revealed && field.field_type != FieldType::TotpSecret,
        }
    }
}

// endregion: --- Compose

// region:    --- Paint

/// Paint the cells top-down inside `inner`, stopping at the last visible row.
pub(super) fn paint(frame: &mut Frame, inner: Rect, cells: &[DetailCell]) {
    let mut ctx = RenderCtx { frame };
    for (row, cell) in cells.iter().take(usize::from(inner.height)).enumerate() {
        let area = Rect {
            x: inner.x,
            y: inner.y + row as u16,
            width: inner.width,
            height: 1,
        };
        cell.render(&mut ctx, area);
    }
}

impl CellWidget for SectionHeader {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, area: Rect) {
        let mut spans = vec![Span::styled(
            format!("{SECTION_INDENT}{}", self.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )];
        if let Some(badge) = &self.badge {
            spans.push(Span::styled(
                format!(" · {badge}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        ctx.frame
            .render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl CellWidget for FieldRow {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, area: Rect) {
        let (label_area, value_area) = gutter_split(area);

        let prefix = if self.selected {
            SELECTED_INDENT
        } else {
            ROW_INDENT
        };
        let label = Line::from(vec![
            Span::styled(prefix.to_string(), Style::default().fg(Color::Yellow)),
            Span::styled(
                pad_label(&self.label),
                Style::default().fg(if self.selected {
                    Color::Yellow
                } else {
                    Color::Cyan
                }),
            ),
        ]);
        ctx.frame.render_widget(Paragraph::new(label), label_area);

        let width = usize::from(value_area.width);
        let value_span = match &self.value {
            RowValue::Pill(on) => Span::styled(pill_label(*on), pill_style(*on)),
            RowValue::LiveCode { code, secs } => Span::styled(
                clamp(&format!("{code}  ({secs}s)"), width),
                Style::default().fg(Color::Cyan),
            ),
            RowValue::Text { text, masked: true } => {
                Span::styled(clamp(text, width), Style::default().fg(Color::DarkGray))
            }
            RowValue::Text {
                text,
                masked: false,
            } => Span::styled(
                clamp(text, width),
                if self.selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
        };
        let mut spans = vec![value_span];
        if self.revealed {
            spans.push(Span::styled(
                " [revealed]",
                Style::default().fg(Color::DarkGray),
            ));
        }
        ctx.frame
            .render_widget(Paragraph::new(Line::from(spans)), value_area);
    }
}

impl CellWidget for MetaRow {
    fn render(&self, ctx: &mut RenderCtx<'_, '_>, area: Rect) {
        let (label_area, value_area) = gutter_split(area);
        let label = Line::from(vec![
            Span::styled(ROW_INDENT.to_string(), Style::default()),
            Span::styled(pad_label(&self.label), Style::default().fg(Color::Cyan)),
        ]);
        ctx.frame.render_widget(Paragraph::new(label), label_area);

        let value = Line::from(Span::styled(
            clamp(&self.text, usize::from(value_area.width)),
            Style::default().fg(Color::DarkGray),
        ));
        ctx.frame.render_widget(Paragraph::new(value), value_area);
    }
}

// endregion: --- Paint

// region:    --- Support

/// Split a pane row into the fixed label gutter and the filling value column.
fn gutter_split(area: Rect) -> (Rect, Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LABEL_GUTTER), Constraint::Min(1)])
        .split(area);
    (cols[0], cols[1])
}

/// The mask run shown instead of an unrevealed secret.
pub(super) fn masked_secret() -> String {
    "•".repeat(MASK_GLYPHS)
}

/// Keep a value on one row: multi-line material would break the grid.
fn flatten(text: &str) -> String {
    text.lines()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Truncate `text` to `width` characters, ending with `…` and a `(n)` cue that
/// carries the true length — a count instead of a scroll tail.
fn clamp(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    let cue = format!(" ({})", compact_count(count));
    let reserved = cue.chars().count() + 1;
    if width <= reserved {
        return text.chars().take(width).collect();
    }
    let mut out: String = text.chars().take(width - reserved).collect();
    out.push('…');
    out.push_str(&cue);
    out
}

/// `1500` prints as `1k`, `100` as `100`.
fn compact_count(count: usize) -> String {
    if count >= 1000 {
        format!("{}k", count / 1000)
    } else {
        count.to_string()
    }
}

/// Truncate the label to the gutter and pad it, so every value starts on the
/// same column.
fn pad_label(label: &str) -> String {
    let mut out: String = label.chars().take(LABEL_WIDTH).collect();
    let len = out.chars().count();
    out.push_str(&" ".repeat(LABEL_WIDTH.saturating_sub(len)));
    out
}

/// The label a row shows — the field key, except where a shorter name is the
/// established one.
fn label_for(field: &Field) -> String {
    if field.key == AGENT_SERVABLE_KEY {
        "servable".to_string()
    } else {
        field.key.clone()
    }
}

fn pill_label(on: bool) -> &'static str {
    if on {
        "[agent-servable]"
    } else {
        "[not servable]"
    }
}

fn pill_style(on: bool) -> Style {
    if on {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Seconds since the epoch — the clock the TOTP code is derived against.
fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn stamp_row(label: &str, ts: i64) -> DetailCell {
    DetailCell::Meta(MetaRow {
        label: label.to_string(),
        text: format_stamp(ts),
    })
}

/// `YYYY-MM-DD HH:MM` in UTC — the same stamp in every timezone, so a pin can
/// assert it.
fn format_stamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

// endregion: --- Support
