//! Detail pane pins: section order, the 16-column label gutter, the fixed
//! mask, the truncation cue, and the metadata rows.
//!
//! The lines asserted here are the pane's contract — they are the ASCII target
//! from the lane-2 spec, measured against a 60-column pane.

use crate::views::main_view::{Column, MainView};
use bogita_core::domain::{Entry, Field, FieldType, FieldValue, AGENT_SERVABLE_KEY};
use bogita_core::test_support::{test_entry, test_field};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Terminal;

use super::support::make_vault;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 20;
/// `YYYY-MM-DD HH:MM` for 1 700 000 000 (2023-11-14 22:13:20 UTC).
const FIXED_TS: i64 = 1_700_000_000;
const FIXED_STAMP: &str = "2023-11-14 22:13";

/// An entry with one field in every section plus the metadata rows.
///
/// Field order is the navigation contract: `detail_field` keeps indexing
/// `entry.fields`, so the fixture keeps a stable index per row.
fn full_entry() -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "GitHub",
        vec![
            field(
                "username",
                FieldValue::Text("alice".into()),
                FieldType::Username,
            ),
            hidden("password", "s3cr3t", FieldType::Token),
            field(
                "url",
                FieldValue::Url("https://github.com/login".into()),
                FieldType::Url,
            ),
            hidden("totp_secret", "JBSWY3DPEHPK3PXP", FieldType::TotpSecret),
            hidden(
                "private_key",
                "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\n",
                FieldType::SshPrivateKey,
            ),
            field(
                "public_key",
                FieldValue::Text("ssh-ed25519 AAAA".into()),
                FieldType::SshPublicKey,
            ),
            field(
                "key_type",
                FieldValue::Text("ed25519".into()),
                FieldType::SshKeyType,
            ),
            field(
                AGENT_SERVABLE_KEY,
                FieldValue::Boolean(true),
                FieldType::Boolean,
            ),
        ],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

pub(super) fn field(key: &str, value: FieldValue, field_type: FieldType) -> Field {
    test_field(key, value, field_type, false, 0)
}

pub(super) fn hidden(key: &str, secret: &str, field_type: FieldType) -> Field {
    test_field(key, FieldValue::Hidden(secret.into()), field_type, true, 0)
}

/// An entry carrying a single hidden `password` field of `len` characters.
fn secret_entry(len: usize) -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "Short",
        vec![hidden("password", &"x".repeat(len), FieldType::Token)],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

/// An entry whose single plain value is `value`.
fn value_entry(value: &str) -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "Long",
        vec![field(
            "username",
            FieldValue::Text(value.into()),
            FieldType::Username,
        )],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

pub(super) fn view_for(entry: &Entry) -> MainView {
    MainView::new(vec![], vec![entry.clone()])
}

/// A Token entry carrying only canonical credential and TOTP rows.
pub(super) fn token_entry() -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "GitHub",
        vec![
            field(
                "url",
                FieldValue::Url("https://github.com/login".into()),
                FieldType::Url,
            ),
            hidden("password", "s3cr3t", FieldType::Token),
            hidden("totp_secret", "JBSWY3DPEHPK3PXP", FieldType::TotpSecret),
            field(
                "username",
                FieldValue::Text("alice".into()),
                FieldType::Username,
            ),
        ],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

/// An SshKey entry with its material and the key-type row.
pub(super) fn ssh_entry() -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "Deploy key",
        vec![
            hidden("private_key", "[REDACTED]\n", FieldType::SshPrivateKey),
            field(
                "public_key",
                FieldValue::Text("ssh-ed25519 AAAA".into()),
                FieldType::SshPublicKey,
            ),
            field(
                "key_type",
                FieldValue::Text("ed25519".into()),
                FieldType::SshKeyType,
            ),
        ],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

/// A Note entry whose body is plain text.
pub(super) fn note_entry() -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "Drawer",
        vec![field(
            "notes",
            FieldValue::Text("recovery codes in the drawer".into()),
            FieldType::Notes,
        )],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

/// An Otp entry: one seed row, which renders as the live code.
pub(super) fn otp_entry() -> Entry {
    let vault = make_vault("Work");
    let mut entry = test_entry(
        vault.id,
        "AWS",
        vec![
            field("totp_period", FieldValue::Number(30), FieldType::TotpPeriod),
            hidden("secret", "JBSWY3DPEHPK3PXP", FieldType::TotpSecret),
        ],
    );
    entry.created_at = FIXED_TS;
    entry.modified_at = FIXED_TS;
    entry
}

/// Draw the pane alone and return the buffer as one string per row, exactly as
/// the terminal sees it (border columns included).
fn pane_rows_raw(view: &MainView, entry: &Entry) -> Result<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT))?;
    terminal.draw(|frame| {
        view.render_detail(frame, Rect::new(0, 0, WIDTH, HEIGHT), Some(entry));
    })?;
    let buffer = terminal.backend().buffer();
    Ok((0..HEIGHT)
        .map(|y| {
            (0..WIDTH)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect())
}

/// The same rows with the right border and the padding dropped, so a pin shows
/// content instead of spaces.
pub(super) fn pane_rows(view: &MainView, entry: &Entry) -> Result<Vec<String>> {
    Ok(pane_rows_raw(view, entry)?
        .iter()
        .map(|row| row.trim_end_matches('│').trim_end().to_string())
        .collect())
}

#[test]
fn detail_pane_masks_with_fixed_ten_glyphs() -> Result<()> {
    // -- Setup & Fixtures: the same field at 4 and at 64 characters
    let short = secret_entry(4);
    let long = secret_entry(64);

    // -- Exec
    let short_rows = pane_rows(&view_for(&short), &short)?;
    let long_rows = pane_rows(&view_for(&long), &long)?;

    // -- Check: one mask width, independent of the secret length
    for rows in [&short_rows, &long_rows] {
        let row = rows
            .iter()
            .find(|row| row.contains("password"))
            .ok_or("missing password row")?;
        let dots = row.chars().filter(|c| *c == '•').count();
        assert_eq!(
            dots, 10,
            "an unrevealed secret always shows exactly 10 mask glyphs, got {dots}: {row}"
        );
        assert!(
            !row.contains('x'),
            "the plaintext must never reach the buffer: {row}"
        );
    }
    Ok(())
}

#[test]
fn detail_pane_truncates_long_values_with_a_count_cue() -> Result<()> {
    // -- Setup & Fixtures
    let value = "a".repeat(100);
    let entry = value_entry(&value);

    // -- Check
    let raw = pane_rows_raw(&view_for(&entry), &entry)?;
    let rows = pane_rows(&view_for(&entry), &entry)?;
    let row = rows
        .iter()
        .find(|row| row.contains("username"))
        .ok_or("missing username row")?;

    // -- Check: ellipsis plus the true length, never a scroll tail
    assert!(
        row.contains('…') && row.ends_with(" (100)"),
        "a long value truncates with `…` and a count cue, got: {row}"
    );
    assert!(
        !row.contains(&"a".repeat(60)),
        "the value must not run past the pane, got: {row}"
    );
    assert!(
        raw.iter()
            .skip(1)
            .take(usize::from(HEIGHT) - 2)
            .all(|row| row.ends_with('│')),
        "every content row keeps both border columns intact: {raw:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_shows_created_and_modified_metadata() -> Result<()> {
    // -- Setup & Fixtures
    let entry = full_entry();

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check
    let created_row = rows
        .iter()
        .position(|row| row.contains("created"))
        .ok_or("missing created row")?;
    let modified_row = rows
        .iter()
        .position(|row| row.contains("modified"))
        .ok_or("missing modified row")?;
    assert!(
        rows[created_row].ends_with(FIXED_STAMP) && rows[modified_row].ends_with(FIXED_STAMP),
        "created/modified must render as YYYY-MM-DD HH:MM, got: {} / {}",
        rows[created_row],
        rows[modified_row]
    );
    assert!(
        created_row < modified_row,
        "created must render above modified, got rows {created_row} and {modified_row}"
    );
    Ok(())
}

#[test]
fn detail_pane_shows_a_servable_pill_not_a_raw_boolean() -> Result<()> {
    // -- Setup & Fixtures: servable and non-servable entries
    let on = full_entry();
    let mut off = full_entry();
    if let Some(field) = off.fields.iter_mut().find(|f| f.key == AGENT_SERVABLE_KEY) {
        field.value = FieldValue::Boolean(false);
    }

    // -- Exec
    let on_rows = pane_rows(&view_for(&on), &on)?;
    let off_rows = pane_rows(&view_for(&off), &off)?;

    // -- Check
    assert!(
        on_rows
            .iter()
            .any(|r| r.contains("servable") && r.contains("[agent-servable]")),
        "a servable entry shows the pill: {on_rows:#?}"
    );
    assert!(
        off_rows
            .iter()
            .any(|r| r.contains("servable") && r.contains("[not servable]")),
        "a non-servable entry shows the negated pill: {off_rows:#?}"
    );
    assert!(
        !on_rows.iter().any(|row| row.contains("true")),
        "the raw boolean must not render: {on_rows:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_has_no_inner_hint_strips() -> Result<()> {
    // -- Setup & Fixtures
    let entry = full_entry();

    // -- Exec: both focus states, since the strips were focus-dependent
    let mut view = view_for(&entry);
    let unfocused = pane_rows(&view, &entry)?.concat();
    view.handle_key(KeyCode::Tab);
    let focused = pane_rows(&view, &entry)?.concat();

    // -- Check: the footer owns the keys, the pane shows data only
    for strip in [
        "[j/k] scroll",
        "[s] reveal",
        "[y] copy",
        "[Tab] focus detail",
    ] {
        assert!(
            !unfocused.contains(strip) && !focused.contains(strip),
            "the pane must not repeat footer keys, found `{strip}`"
        );
    }
    Ok(())
}

#[test]
fn detail_pane_marks_the_selected_field_with_the_chevron() -> Result<()> {
    // -- Setup & Fixtures
    let entry = full_entry();
    let mut view = view_for(&entry);

    // -- Exec: focus the detail column, then step to the second field
    view.handle_key(KeyCode::Tab);
    view.handle_key(KeyCode::Char('j'));

    // -- Check
    assert_eq!(view.focused, Column::Detail);
    let rows = pane_rows(&view, &entry)?;
    assert!(
        rows.iter().any(|row| row.contains("▶ password")),
        "the selected field carries the chevron, rendered: {rows:#?}"
    );
    assert_eq!(
        rows.iter().filter(|row| row.contains('▶')).count(),
        1,
        "exactly one row is selected, rendered: {rows:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_shows_a_revealed_secret_with_its_affordance() -> Result<()> {
    // -- Setup & Fixtures
    let entry = secret_entry(6);
    let mut view = view_for(&entry);

    // -- Exec: focus detail (field 0 is the secret), then reveal
    view.handle_key(KeyCode::Tab);
    view.handle_key(KeyCode::Char('s'));

    // -- Check
    let rows = pane_rows(&view, &entry)?;
    let row = rows
        .iter()
        .find(|row| row.contains("password"))
        .ok_or("missing password row")?;
    assert!(
        row.contains("xxxxxx") && row.contains("[revealed]"),
        "a revealed secret shows its value with the affordance, got: {row}"
    );
    Ok(())
}
