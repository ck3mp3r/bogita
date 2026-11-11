type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── scroll clamp, indicator, spacing ─────────────────────────────────────────

#[tokio::test]
async fn test_error_modal_paints_above_help_overlay() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    tui.error_message = Some("boom".to_string());

    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| tui.render(frame))?;
    let symbols: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    assert!(
        symbols.contains("boom"),
        "an error must win over the help overlay when both are open"
    );
    Ok(())
}

#[test]
fn test_help_max_scroll_clamps() {
    assert_eq!(help::max_scroll(200, 24), 176);
    assert_eq!(
        help::max_scroll(10, 24),
        0,
        "content that fits never scrolls"
    );
}

#[test]
fn test_help_layout_spacers_between_sections() -> Result<()> {
    let rows = bindings::help_rows(Surface::MainEntries);
    let lines = help::layout(&rows, 100);

    assert!(
        !matches!(lines.first(), Some(HelpLine::Spacer)),
        "the first section header carries no spacer"
    );
    for (index, line) in lines.iter().enumerate() {
        if matches!(line, HelpLine::Header { .. }) && index > 0 {
            assert!(
                matches!(lines[index - 1], HelpLine::Spacer),
                "the header at line {index} needs one blank line before it"
            );
        }
    }
    Ok(())
}

#[test]
fn test_help_layout_collapse_boundary_at_sixty() {
    let rows: Vec<HelpRow> = (1..=4)
        .map(|_| HelpRow {
            section: "Section".to_string(),
            key: "k",
            action: "a",
            desc: "d",
            focused: false,
        })
        .collect::<Vec<_>>();
    let row_count = |width: u16| {
        help::layout(&rows, width)
            .iter()
            .filter(|line| matches!(line, HelpLine::Row(_)))
            .count()
    };

    assert_eq!(
        row_count(59),
        8,
        "59 columns: one column, key + description"
    );
    assert_eq!(row_count(60), 2, "60 columns: the two-column grid starts");
    assert_eq!(row_count(61), 2, "61 columns: still two columns");
}

/// The modal's inner rect for a terminal of `width` x `height`.
fn modal_inner(width: u16, height: u16) -> Rect {
    let body = Rect::new(0, 1, width, height.saturating_sub(2));
    let modal = crate::app::centered_rect(80, 80, body);
    Rect::new(
        modal.x + 1,
        modal.y + 1,
        modal.width.saturating_sub(2),
        modal.height.saturating_sub(2),
    )
}

#[tokio::test]
async fn test_overlay_scroll_clamps_to_content() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    for _ in 0..200 {
        tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE);
    }

    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| tui.render(frame))?;
    let symbols: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    assert!(
        symbols.contains("Keybindings overlay"),
        "200 j presses must clamp to the last section, not scroll past it"
    );
    Ok(())
}

#[tokio::test]
async fn test_help_overlay_scrollbar_shown_when_overflowing() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    // Scroll all the way down: the thumb must arrive at the bottom edge.
    for _ in 0..200 {
        tui.handle_key_with_modifiers(KeyCode::Char('j'), KeyModifiers::NONE);
    }

    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| tui.render(frame))?;
    let inner = modal_inner(80, 24);
    let column = inner.x + inner.width - 1;
    let glyphs: Vec<&str> = (inner.y..inner.y + inner.height)
        .filter_map(|y| {
            let cell = terminal.backend().buffer().cell((column, y))?;
            Some(cell.symbol())
        })
        .collect();

    assert!(
        glyphs.contains(&"█"),
        "an overflowing overlay shows its scrollbar thumb, got {glyphs:?}"
    );
    assert!(
        !glyphs.contains(&"▲") && !glyphs.contains(&"▼"),
        "the scrollbar carries no arrow heads, got {glyphs:?}"
    );
    let last = glyphs.iter().rev().find(|glyph| **glyph != " ");
    assert_eq!(
        last.copied(),
        Some("█"),
        "at max scroll the thumb must sit on the LAST track cell, got {glyphs:?}"
    );
    Ok(())
}

#[tokio::test]
async fn test_help_overlay_scrollbar_absent_when_it_fits() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;
    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);

    let (width, height) = (300, 200);
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| tui.render(frame))?;
    let inner = modal_inner(width, height);
    let column = inner.x + inner.width - 1;
    let glyphs: Vec<&str> = (inner.y..inner.y + inner.height)
        .filter_map(|y| {
            let cell = terminal.backend().buffer().cell((column, y))?;
            Some(cell.symbol())
        })
        .collect();

    assert!(
        !glyphs.contains(&"█"),
        "the overlay that fits shows no scrollbar, got {glyphs:?}"
    );
    Ok(())
}

use crate::app::bindings::{self, HelpRow, Surface};
use crate::app::help::{self, HelpLine};
use crate::app::Tui;
use crate::context::TuiContext;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::support::make_app;

const ALL_SURFACES: &[Surface] = &[
    Surface::MainEntries,
    Surface::MainDetail,
    Surface::MainLeaderEntries,
    Surface::MainLeaderDetail,
    Surface::MainVaultPicker,
    Surface::Search,
    Surface::FormName,
    Surface::FormKey,
    Surface::FormToken,
    Surface::FormValue,
    Surface::FormType,
    Surface::ConfirmSave,
    Surface::ConfirmDiscard,
    Surface::ConfirmDelete,
    Surface::VaultFormName,
    Surface::VaultFormCheckbox,
    Surface::Locked,
    Surface::LockedError,
    Surface::FormVaultPicker,
    Surface::PasswordGen,
    Surface::ErrorOverlay,
];

fn registry_total() -> usize {
    bindings::SECTIONS
        .iter()
        .map(|section| section.table.len())
        .sum()
}

fn row_lines(lines: &[HelpLine]) -> Vec<&str> {
    lines
        .iter()
        .filter_map(|line| match line {
            HelpLine::Row(text) => Some(text.as_str()),
            HelpLine::Header { .. } | HelpLine::Spacer => None,
        })
        .collect()
}

fn header_titles(lines: &[HelpLine]) -> Vec<&str> {
    lines
        .iter()
        .filter_map(|line| match line {
            HelpLine::Header { title, .. } => Some(title.as_str()),
            HelpLine::Row(_) | HelpLine::Spacer => None,
        })
        .collect()
}

#[test]
fn test_help_rows_cover_the_whole_registry_two_way() -> Result<()> {
    for surface in ALL_SURFACES {
        let rows = bindings::help_rows(*surface);
        assert_eq!(
            rows.len(),
            registry_total(),
            "{surface:?}: the overlay must render every registry row once"
        );

        // Every row that renders is a registry row, and no registry row is
        // invented: compare the (section, key, action) triples both ways.
        for row in &rows {
            assert!(
                bindings::SECTIONS
                    .iter()
                    .any(
                        |section| section.table.iter().any(|binding| binding.key == row.key
                            && binding.action == row.action
                            && row
                                .section
                                .contains(&bindings::section_title(section, false)))
                    ),
                "{surface:?}: overlay row `{} {}` has no registry entry",
                row.key,
                row.action
            );
        }
    }
    Ok(())
}

#[test]
fn test_help_every_binding_is_described() -> Result<()> {
    for section in bindings::SECTIONS {
        for binding in section.table {
            assert!(
                !binding.desc.trim().is_empty(),
                "`{}` has no description",
                binding.key
            );
            assert!(
                binding.desc.chars().count() <= 60,
                "`{}` description is longer than 60 chars: {}",
                binding.key,
                binding.desc
            );
        }
    }
    Ok(())
}

#[test]
fn test_help_focused_section_comes_first() -> Result<()> {
    for surface in ALL_SURFACES {
        let rows = bindings::help_rows(*surface);
        let first = rows.first().ok_or("the overlay must have rows")?;
        // The focused section is the registry section that owns this surface's
        // keys — `LockedError` shares the lock screen's section.
        let focused_label = bindings::section_label(*surface);
        assert!(
            first.focused,
            "{surface:?}: the first row must belong to the focused section"
        );
        assert!(
            first.section.contains(focused_label),
            "{surface:?}: the first section must be `{focused_label}`, got {}",
            first.section
        );
    }
    Ok(())
}

#[test]
fn test_help_two_column_density() -> Result<()> {
    let rows = bindings::help_rows(Surface::MainEntries);
    let lines = help::layout(&rows, 100);

    // Two rows share one line, so a section of n rows costs ceil(n/2) lines.
    let expected_rows: usize = bindings::SECTIONS
        .iter()
        .map(|section| section.table.len().div_ceil(2))
        .sum();
    assert_eq!(
        row_lines(&lines).len(),
        expected_rows,
        "the two-column grid must pack two bindings per line"
    );
    assert_eq!(
        header_titles(&lines).len(),
        bindings::SECTIONS.len(),
        "every section needs its heading"
    );
    for line in row_lines(&lines) {
        assert!(
            line.chars().count() <= 100,
            "a row overran the overlay width: {line}"
        );
    }
    Ok(())
}

#[test]
fn test_help_single_column_below_sixty() -> Result<()> {
    let rows = bindings::help_rows(Surface::MainDetail);
    let lines = help::layout(&rows, 40);
    let texts = row_lines(&lines);

    // One column: each binding takes a key/action line and a description line.
    assert_eq!(texts.len(), registry_total() * 2);
    for line in &texts {
        assert!(
            line.chars().count() <= 40,
            "a narrow column overran its width: {line}"
        );
    }
    Ok(())
}
