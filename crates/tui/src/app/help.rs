//! Help-overlay layout — the geometry between the binding registry and the
//! rendered `?` modal.
//!
//! Pure and terminal-free: [`layout`] turns the registry rows into headings and
//! text lines, and the renderer only maps those to styled `Line`s. That keeps
//! the density and completeness guarantees testable without a terminal.

use super::bindings;

/// Below this width the two-column grid collapses into one column.
const TWO_COLUMN_MIN_WIDTH: u16 = 60;

/// A laid-out help line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum HelpLine {
    /// A section heading; `focused` is set for the caller's own surface.
    Header { title: String, focused: bool },
    /// A rendered binding row (or, in one-column mode, a description row).
    Row(String),
    /// Breathing room between two sections.
    Spacer,
}

/// The furthest the overlay may scroll for `content_lines` of text shown in a
/// viewport of `viewport_lines`. Zero when everything fits.
pub(crate) fn max_scroll(content_lines: usize, viewport_lines: u16) -> u16 {
    content_lines
        .saturating_sub(viewport_lines as usize)
        .min(u16::MAX as usize) as u16
}

/// Lay the registry rows out for the given inner width.
///
/// Two columns at [`TWO_COLUMN_MIN_WIDTH`] and above; one column below it, where
/// each binding takes a key/action line plus an indented description line so the
/// sentence survives a narrow terminal. Every cell is clamped to its column, so
/// nothing can overrun the modal border.
pub(crate) fn layout(rows: &[bindings::HelpRow], width: u16) -> Vec<HelpLine> {
    let width = width as usize;
    if width == 0 {
        return Vec::new();
    }
    let two_column = width >= TWO_COLUMN_MIN_WIDTH as usize;
    let mut lines = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        let section = rows[index].section.clone();
        let start = index;
        while index < rows.len() && rows[index].section == section {
            index += 1;
        }
        let group = &rows[start..index];
        // Sections breathe: one blank line before every header but the first.
        if !lines.is_empty() {
            lines.push(HelpLine::Spacer);
        }
        lines.push(HelpLine::Header {
            title: section,
            focused: group.first().is_some_and(|row| row.focused),
        });
        if two_column {
            // Two cells plus a two-space gutter must fit inside `width`.
            let column = width.saturating_sub(2) / 2;
            for pair in group.chunks(2) {
                let left = cell(&pair[0], column);
                let right = pair.get(1).map(|row| cell(row, column)).unwrap_or_default();
                lines.push(HelpLine::Row(format!("{left:column$}  {right}")));
            }
        } else {
            for row in group {
                lines.push(HelpLine::Row(clamp(
                    &format!("[{}] {}", row.key, row.action),
                    width,
                )));
                lines.push(HelpLine::Row(clamp(&format!("    {}", row.desc), width)));
            }
        }
    }
    lines
}

/// One binding rendered as a grid cell: `[key] action — description`.
fn cell(row: &bindings::HelpRow, width: usize) -> String {
    clamp(
        &format!("[{}] {} — {}", row.key, row.action, row.desc),
        width,
    )
}

/// Clamp `text` to `width` characters, marking the cut with `…`.
fn clamp(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/help_overlay.rs"]
mod app_help_overlay_test;
// endregion: --- Tests
