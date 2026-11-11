//! Category layout pins: what each entry category owns, and what it must not
//! invent. The pane's shared fixtures and paint helpers live in `detail.rs`.

use super::main_view_detail_test::{
    field, hidden, note_entry, otp_entry, pane_rows, ssh_entry, token_entry, view_for,
};
use super::support::make_vault;
use bogita_core::domain::{FieldType, FieldValue};
use bogita_core::test_support::test_entry;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn detail_pane_token_category_layout() -> Result<()> {
    // -- Setup & Fixtures: a Token entry with canonical credential + TOTP rows
    let entry = token_entry();
    let view = view_for(&entry);

    // -- Exec
    let rows = pane_rows(&view, &entry)?;

    // -- Check: the pane frame names its surface and its entry
    assert!(
        rows[0].starts_with("┌ Detail · GitHub ─"),
        "the pane border must carry the surface and the entry name, got: {}",
        rows[0]
    );

    // -- Check: one CREDENTIALS group holds the canonical rows, no TOTP group
    let header = rows
        .iter()
        .position(|row| row == "│ CREDENTIALS")
        .ok_or_else(|| format!("missing the credential header: {rows:#?}"))?;
    let expected = [
        "│  username      alice",
        "│  password      ••••••••••",
        "│  url           https://github.com/login",
    ];
    let mut cursor = header;
    for line in expected {
        let found = rows
            .iter()
            .position(|row| row == line)
            .ok_or_else(|| format!("missing pinned row `{line}`, rendered: {rows:#?}"))?;
        assert!(
            found > cursor,
            "`{line}` must render after the previous row (canonical order)"
        );
        cursor = found;
    }
    for invented in ["TOTP", "SSH", "OTHER FIELDS"] {
        assert!(
            !rows.iter().any(|row| row.contains(invented)),
            "a canonical Token entry must not gain a `{invented}` group: {rows:#?}"
        );
    }

    // -- Check: the totp row is the LIVE code, unmasked, with its countdown
    let code_row = rows
        .iter()
        .find(|row| row.contains("totp_secret"))
        .ok_or("missing the totp row")?;
    assert!(
        code_row.contains('(') && code_row.ends_with("s)"),
        "the code row carries the live countdown, got: {code_row}"
    );
    assert!(
        !code_row.contains('•'),
        "the derived code is shown, not masked: {code_row}"
    );

    // -- Check: metadata stays last and the seed never reaches the buffer
    let rendered = rows.join("\n");
    assert!(
        rendered.contains("│ METADATA"),
        "the metadata group always renders: {rows:#?}"
    );
    assert!(
        !rendered.contains("JBSWY3DPEHPK3PXP"),
        "the base32 seed must not reach the detail buffer"
    );
    Ok(())
}

#[test]
fn detail_pane_other_fields_only_when_present() -> Result<()> {
    // -- Setup & Fixtures: one Token with a custom row, one without
    let mut with_custom = token_entry();
    with_custom.fields.push(field(
        "api_key",
        FieldValue::Hidden("k-123".into()),
        FieldType::Custom("api_key".into()),
    ));
    let canonical_only = token_entry();

    // -- Exec
    let custom_rows = pane_rows(&view_for(&with_custom), &with_custom)?;
    let plain_rows = pane_rows(&view_for(&canonical_only), &canonical_only)?;

    // -- Check: the custom row gets its own data-owned group, after the category
    let other = custom_rows
        .iter()
        .position(|row| row == "│ OTHER FIELDS")
        .ok_or_else(|| {
            format!("a non-canonical row needs an OTHER FIELDS group: {custom_rows:#?}")
        })?;
    let api = custom_rows
        .iter()
        .position(|row| row.contains("api_key"))
        .ok_or("missing the custom row")?;
    assert!(
        api > other,
        "the custom row renders under OTHER FIELDS, got rows {other} and {api}"
    );
    assert!(
        custom_rows[api].contains("••••••••••"),
        "a custom hidden value stays masked, got: {}",
        custom_rows[api]
    );

    // -- Check: no group when the entry has no non-canonical rows
    assert!(
        !plain_rows.iter().any(|row| row.contains("OTHER FIELDS")),
        "a canonical-only entry shows no OTHER FIELDS group: {plain_rows:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_ssh_category_layout() -> Result<()> {
    // -- Setup & Fixtures
    let entry = ssh_entry();

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check: the SSH header carries the key type as its badge
    assert!(
        rows.iter().any(|row| row == "│ SSH · ED25519"),
        "the SSH group header carries the key type badge: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("key_type")),
        "the key type is a badge, never a row: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("CREDENTIALS")),
        "an SSH key entry owns no credential group: {rows:#?}"
    );

    // -- Check: material rows keep the fixed mask and the visible public key
    let private = rows
        .iter()
        .find(|row| row.contains("private_key"))
        .ok_or("missing private_key row")?;
    assert_eq!(
        private.chars().filter(|c| *c == '•').count(),
        10,
        "the private key is masked with fixed 10 glyphs, got: {private}"
    );
    assert!(
        !private.contains("REDACTED"),
        "the private key plaintext must not render: {private}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("public_key") && row.contains("ssh-ed25519 AAAA")),
        "the public key renders in full: {rows:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_note_category_shows_the_body() -> Result<()> {
    // -- Setup & Fixtures
    let entry = note_entry();

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check: one NOTE group with the body, no invented groups
    assert!(
        rows.iter().any(|row| row == "│ NOTE"),
        "a note shows its NOTE group: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("notes") && row.contains("recovery codes in the drawer")),
        "the note body renders as a row: {rows:#?}"
    );
    for invented in ["CREDENTIALS", "SSH", "OTHER FIELDS"] {
        assert!(
            !rows.iter().any(|row| row.contains(invented)),
            "a note must not gain a `{invented}` group: {rows:#?}"
        );
    }
    Ok(())
}

#[test]
fn detail_pane_otp_category_is_code_first() -> Result<()> {
    // -- Setup & Fixtures
    let entry = otp_entry();

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check: the OTP group renders its live code above every other row
    let header = rows
        .iter()
        .position(|row| row == "│ OTP")
        .ok_or_else(|| format!("missing the OTP group: {rows:#?}"))?;
    let code_row = rows
        .iter()
        .position(|row| row.contains("(") && row.ends_with("s)"))
        .ok_or_else(|| format!("missing the live code row: {rows:#?}"))?;
    let first_row = rows
        .iter()
        .position(|row| row.starts_with("│  "))
        .ok_or("missing the first field row")?;
    assert_eq!(
        code_row, first_row,
        "the code is the first row of the entry, got row {code_row} vs {first_row}: {rows:#?}"
    );
    assert!(
        code_row > header,
        "the code renders under the OTP group: {rows:#?}"
    );
    assert!(
        !rows[code_row].contains('•'),
        "the OTP code is never masked: {}",
        rows[code_row]
    );
    Ok(())
}

#[test]
fn detail_pane_groups_by_field_inventory_not_entry_type() {
    // -- Setup & Fixtures
    let pane = include_str!("../../../src/views/main_view/detail.rs");
    let layout = include_str!("../../../src/views/main_view/detail_layout.rs");

    // -- Check: no field-type sniffing and no entry_type reads anywhere
    for retired in ["Section::of", "enum Section", "Section::Credentials"] {
        assert!(
            !pane.contains(retired) && !layout.contains(retired),
            "the detail pane must not group by field type, found `{retired}`"
        );
    }
    for banned in ["entry_type", "EntryType"] {
        assert!(
            !pane.contains(banned) && !layout.contains(banned),
            "rendering must not read the stored category, found `{banned}`"
        );
    }
    assert!(
        layout.contains("is_ssh_material") && layout.contains("LayoutSection"),
        "the layout comes from the field inventory, sectioned per group"
    );
    assert!(
        pane.contains("CategoryLayout::of(entry)"),
        "the pane must take its rows from the inventory layout"
    );
}

#[test]
fn detail_pane_ssh_material_renders_as_ssh_without_empty_credentials() -> Result<()> {
    // -- Setup & Fixtures: the real vault shape — SSH material on a token-typed
    //    entry, the key type stored as a custom row, the gate as a boolean.
    let vault = make_vault("Work");
    let entry = test_entry(
        vault.id,
        "id_ed25519",
        vec![
            hidden("private_key", "[REDACTED]\n", FieldType::SshPrivateKey),
            field(
                "public_key",
                FieldValue::Text("ssh-ed25519 AAAA".into()),
                FieldType::Custom("public_key".into()),
            ),
            field(
                "key_type",
                FieldValue::Text("Ed25519".into()),
                FieldType::Custom("key_type".into()),
            ),
            field(
                "agent_servable",
                FieldValue::Boolean(true),
                FieldType::Boolean,
            ),
        ],
    );

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check: SSH pane with the badge from the key_type row's value
    assert!(
        rows.iter().any(|row| row == "│ SSH · ED25519"),
        "the SSH header carries the key-type row's value as its badge: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("CREDENTIALS")),
        "an SSH-material entry owns no credential group: {rows:#?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("OTHER FIELDS")),
        "the material rows are canonical, so no OTHER FIELDS group: {rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("[agent-servable]")),
        "the gate renders as the pill in METADATA: {rows:#?}"
    );
    Ok(())
}

#[test]
fn detail_pane_skips_every_empty_group() -> Result<()> {
    // -- Setup & Fixtures: an entry with no fields at all
    let vault = make_vault("Work");
    let entry = test_entry(vault.id, "Empty", vec![]);

    // -- Exec
    let rows = pane_rows(&view_for(&entry), &entry)?;

    // -- Check: metadata is the only group; every other header is absent
    for absent in ["CREDENTIALS", "SSH", "OTP", "NOTE", "OTHER FIELDS"] {
        assert!(
            !rows.iter().any(|row| row.contains(absent)),
            "an empty entry must not show a `{absent}` group: {rows:#?}"
        );
    }
    assert!(
        rows.iter().any(|row| row == "│ METADATA"),
        "metadata always renders: {rows:#?}"
    );
    Ok(())
}
