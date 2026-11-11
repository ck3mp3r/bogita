//!
//! Type fidelity on save: the type the user chose survives confirm, the value
//! variant follows the chosen arm, the entry keeps its category, and rows the
//! user never re-typed are restored to the shapes the key generator authored.

use crate::views::entry_form::{EntryForm, FormAction, FormFieldType};
use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue, SshKeyMaterial};
use chrono::Utc;
use ratatui::crossterm::event::KeyCode;
use uuid::Uuid;

// ── fixtures ──────────────────────────────────────────────────────────────────

/// An ssh-key entry as the key generator writes it: four authored field types.
fn keygen_ssh_entry() -> Entry {
    let material =
        SshKeyMaterial::keygen_ed25519("type-fidelity@test").expect("keygen an Ed25519 key");
    Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Deploy key".to_string(),
        entry_type: EntryType::SshKey,
        fields: material.to_fields(),
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    }
}

/// An entry with one plain-text row holding `value` under `key`.
fn text_entry(entry_type: EntryType, key: &str, value: &str) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Row".to_string(),
        entry_type,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: key.to_string(),
            value: FieldValue::Text(value.to_string()),
            field_type: FieldType::Custom(key.to_string()),
            encrypted: false,
            idx: 0,
        }],
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
    }
}

/// A valid Ed25519 PEM to stand in as a private key value.
fn keygen_pem() -> String {
    SshKeyMaterial::keygen_ed25519("value@test")
        .expect("keygen an Ed25519 key")
        .private_pem()
        .to_string()
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Tab from the name slot to row `row`'s type slot — name → key → type → value.
fn focus_type_slot(form: &mut EntryForm, row: usize) {
    for _ in 0..(2 + 3 * row) {
        form.handle_key(KeyCode::Tab);
    }
}

/// Tab to row `row`'s type slot and pick the arm labelled `label`.
fn pick_type(form: &mut EntryForm, row: usize, label: &str) {
    let target = arm_index(label);
    focus_type_slot(form, row);
    let count = FormFieldType::count();
    let current = form.fields[row].type_state.value();
    let steps = (target + count - current) % count;
    form.handle_key(KeyCode::Enter); // open the dropdown
    for _ in 0..steps {
        form.handle_key(KeyCode::Char('j'));
    }
    form.handle_key(KeyCode::Enter); // apply the selection
}

/// The index of the arm carrying `label` in the selector table.
fn arm_index(label: &str) -> usize {
    FormFieldType::TABLE
        .iter()
        .position(|(_, candidate, _)| *candidate == label)
        .unwrap_or_else(|| panic!("the picker must offer `{label}`"))
}

/// Confirm and unwrap the entry, failing loudly on a validation error.
fn confirm(form: &mut EntryForm) -> Entry {
    match form.confirm() {
        FormAction::Confirm(entry) => entry,
        FormAction::ValidationError(message) => panic!("unexpected validation error: {message}"),
        other => panic!("expected Confirm, got {other:?}"),
    }
}

// ── the headline scenario ─────────────────────────────────────────────────────

#[test]
fn test_confirm_flip_public_key_row_to_ssh_public_persists() {
    // -- Setup & Fixtures: a keygen entry; its public_key row loads as Text
    let entry = keygen_ssh_entry();
    let public_idx = entry
        .fields
        .iter()
        .position(|f| f.field_type == FieldType::SshPublicKey)
        .expect("the fixture carries a public_key row");
    let public_line = entry.fields[public_idx].value.to_string();
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: the user explicitly chooses ssh-public for that row, then saves
    pick_type(&mut form, public_idx, "ssh-public");
    let confirmed = confirm(&mut form);

    // -- Check: the choice reached storage as SshPublicKey + plain Text
    let stored = &confirmed.fields[public_idx];
    assert_eq!(
        stored.field_type,
        FieldType::SshPublicKey,
        "the chosen arm must be the stored field type, got {:?}",
        stored.field_type
    );
    assert_eq!(
        stored.value,
        FieldValue::Text(public_line),
        "an ssh-public row stores the plaintext value"
    );
    assert!(
        !stored.encrypted,
        "a public key row is not encrypted, got encrypted={}",
        stored.encrypted
    );

    // -- Check: reloading shows the same arm, so the choice is stable
    let reloaded = EntryForm::new_edit(&confirmed);
    assert_eq!(
        reloaded.field_badge(public_idx),
        "ssh-public",
        "the row reloads as the chosen arm"
    );
}

#[test]
fn test_confirm_flip_to_ssh_private_masks_and_encrypts() {
    // -- Setup & Fixtures: a plain custom row whose value is a real PEM
    let entry = text_entry(EntryType::SshKey, "my_key", &keygen_pem());
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: the user chooses ssh-key for that row, then saves
    pick_type(&mut form, 0, "ssh-key");
    let confirmed = confirm(&mut form);

    // -- Check: the arm owns the value shape and the encryption flag
    let stored = &confirmed.fields[0];
    assert_eq!(stored.field_type, FieldType::SshPrivateKey);
    assert!(
        matches!(stored.value, FieldValue::Hidden(_)),
        "an ssh-key row stores a hidden value, got {:?}",
        stored.value
    );
    assert!(
        stored.encrypted,
        "an ssh-key row is encrypted, got encrypted={}",
        stored.encrypted
    );
}

#[test]
fn test_confirm_flip_private_to_ssh_public_unmasks_and_decrypts() {
    // -- Setup & Fixtures: the keygen entry's private_key row is Hidden+encrypted
    let entry = keygen_ssh_entry();
    let private_idx = 0;
    let pem = entry.fields[private_idx].value.to_string();
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: the user re-types that row as ssh-public
    pick_type(&mut form, private_idx, "ssh-public");
    let confirmed = confirm(&mut form);

    // -- Check: the masks and the encryption flip with the chosen arm
    let stored = &confirmed.fields[private_idx];
    assert_eq!(
        stored.field_type,
        FieldType::SshPublicKey,
        "a flipped row takes the chosen domain type"
    );
    assert_eq!(
        stored.value,
        FieldValue::Text(pem),
        "the value becomes plaintext when the arm is public"
    );
    assert!(
        !stored.encrypted,
        "a public arm is not encrypted, got encrypted={}",
        stored.encrypted
    );
}

#[test]
fn test_confirm_flip_public_row_to_ssh_key_needs_a_pem() {
    // -- Setup & Fixtures: a loaded public_key row whose value is not a PEM
    let entry = keygen_ssh_entry();
    let public_idx = entry
        .fields
        .iter()
        .position(|f| f.field_type == FieldType::SshPublicKey)
        .expect("the fixture carries a public_key row");
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: choosing ssh-key drags the row through the private-key door
    pick_type(&mut form, public_idx, "ssh-key");
    let action = form.confirm();

    // -- Check: the PEM rule still gates the flipped row
    let FormAction::ValidationError(message) = action else {
        panic!("a public line is not a private key, got {action:?}");
    };
    assert!(
        message.contains("private key is not a valid OpenSSH private key"),
        "the message names the door: {message}"
    );
}

#[test]
fn test_confirm_untouched_keygen_entry_roundtrip() {
    // -- Setup & Fixtures: the authoring shapes the form collapsed on load
    let entry = keygen_ssh_entry();
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: save without touching a single slot
    let confirmed = confirm(&mut form);

    // -- Check: every row keeps the type the generator authored
    let expected = [
        FieldType::SshPrivateKey,
        FieldType::SshPublicKey,
        FieldType::SshKeyType,
        FieldType::SshComment,
    ];
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(
            confirmed.fields[i].field_type, *want,
            "row {i} must round-trip as {want:?}, got {:?}",
            confirmed.fields[i].field_type
        );
        assert_eq!(
            confirmed.fields[i].value.to_string(),
            entry.fields[i].value.to_string(),
            "row {i} value must survive untouched"
        );
    }
    assert!(
        confirmed.fields[0].encrypted
            && !confirmed.fields[1].encrypted
            && !confirmed.fields[2].encrypted
            && !confirmed.fields[3].encrypted,
        "only the private key is encrypted"
    );
    assert_eq!(confirmed.entry_type, EntryType::SshKey);
}

#[test]
fn test_confirm_touched_text_row_never_guesses_the_type() {
    // -- Setup & Fixtures: a row whose arm is the collapsed Text shape
    let entry = keygen_ssh_entry();
    let comment_idx = entry
        .fields
        .iter()
        .position(|f| f.field_type == FieldType::SshComment)
        .expect("the fixture carries a comment row");
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: edit that row's value (the arm stays text), then save
    for _ in 0..(3 + 3 * comment_idx) {
        form.handle_key(KeyCode::Tab); // name → key → type → value per row
    }
    form.handle_key(KeyCode::Char('x'));
    let confirmed = confirm(&mut form);

    // -- Check: an edited row loses the fallback, so nothing is inferred
    assert_eq!(
        confirmed.fields[comment_idx].field_type,
        FieldType::Custom("comment".to_string()),
        "a touched text row saves as the arm it shows, got {:?}",
        confirmed.fields[comment_idx].field_type
    );
}

#[test]
fn test_confirm_preserves_entry_type() {
    // -- Setup & Fixtures: a note entry
    let entry = text_entry(EntryType::Note, "notes", "recovery codes in the drawer");
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec
    let confirmed = confirm(&mut form);

    // -- Check: the category survives the save door
    assert_eq!(
        confirmed.entry_type,
        EntryType::Note,
        "confirm must keep the loaded entry type, got {:?}",
        confirmed.entry_type
    );

    // -- Check: the hardcode is gone from the source
    let source = include_str!("../../../src/views/entry_form/confirm.rs");
    assert!(
        !source.contains("EntryType::Token"),
        "confirm.rs must not hardcode the entry type"
    );
}

#[test]
fn test_picker_offers_separate_ssh_arms() {
    // -- Check: the selector table carries both ssh arms, with distinct labels
    let labels: Vec<&str> = FormFieldType::TABLE
        .iter()
        .map(|(_, label, _)| *label)
        .collect();
    assert!(
        labels.contains(&"ssh-key"),
        "the picker must offer the private arm: {labels:?}"
    );
    assert!(
        labels.contains(&"ssh-public"),
        "the picker must offer the public arm: {labels:?}"
    );
    assert_ne!(arm_index("ssh-key"), arm_index("ssh-public"));

    // -- Check: the arms carry their own masking rule
    let private = &FormFieldType::TABLE[arm_index("ssh-key")];
    let public = &FormFieldType::TABLE[arm_index("ssh-public")];
    assert!(private.2, "the private arm is obscured");
    assert!(!public.2, "the public arm is not obscured");
}

#[test]
fn test_chosen_text_row_saves_as_custom() {
    // -- Setup & Fixtures: a row the user explicitly sets back to text
    let entry = text_entry(EntryType::Token, "api_key", "k-123");
    let mut form = EntryForm::new_edit(&entry);

    // -- Exec: choose text (index 0) explicitly
    pick_type(&mut form, 0, "text");
    let confirmed = confirm(&mut form);

    // -- Check: a chosen text arm is a plain custom field, not a guess
    assert_eq!(
        confirmed.fields[0].field_type,
        FieldType::Custom("api_key".to_string())
    );
    assert!(matches!(confirmed.fields[0].value, FieldValue::Text(_)));
    assert!(!confirmed.fields[0].encrypted);
}
