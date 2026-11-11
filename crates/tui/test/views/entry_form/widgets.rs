//! Characterisation of the six field widgets: their value rule and timing.
//!
//! The compile-level drift guard lives in `state.rs`: `EntryForm::validate`
//! matches every `FormFieldType` variant without a wildcard, so a new variant
//! does not build until it names its widget.

use crate::views::entry_form::widgets::{
    flags, has_complete_armor, BooleanWidget, FieldWidget, NotesWidget, PasswordRule,
    PasswordWidget, PlaintextWidget, SshKeyWidget, Timing, TotpSecretWidget,
};
use bogita_core::domain::SshKeyMaterial;

// ── SSH keys ──────────────────────────────────────────────────────────────────

#[test]
fn ssh_key_widget_rejects_garbage_and_accepts_a_generated_key() {
    let widget = SshKeyWidget;

    let error = widget
        .validate_value("[REDACTED]")
        .err()
        .unwrap_or_default();
    assert!(!error.is_empty(), "garbage armor must carry a reason");

    let pem = SshKeyMaterial::keygen_ed25519("widget@test")
        .expect("keygen an Ed25519 key")
        .private_pem()
        .to_string();
    assert!(widget.validate_value(&pem).is_ok());
}

#[test]
fn ssh_key_widget_judges_a_complete_input_only() {
    assert_eq!(SshKeyWidget.timing(), Timing::OnComplete);

    let begin = "-----BEGIN OPENSSH PRIVATE KEY-----";
    let end = "-----END OPENSSH PRIVATE KEY-----";
    let open_armor = format!("{begin}\n");
    let closed_armor = format!("{begin}\nAAAA\n{end}\n");

    assert!(has_complete_armor(&closed_armor));
    assert!(!has_complete_armor(&open_armor));
    assert!(
        !flags(&SshKeyWidget, &open_armor, true),
        "half a key must not flag while the slot is focused"
    );
    assert!(
        flags(&SshKeyWidget, &open_armor, false),
        "half a key must flag once the slot is left"
    );
    assert!(
        flags(&SshKeyWidget, &closed_armor, true),
        "a closed armor is judged even while focused"
    );
    assert!(
        !flags(&SshKeyWidget, "", true),
        "an empty value is never judged"
    );
}

// ── TOTP ─────────────────────────────────────────────────────────────────────

#[test]
fn totp_widget_requires_base32() {
    assert_eq!(TotpSecretWidget.timing(), Timing::OnChange);
    assert!(TotpSecretWidget.validate_value("JBSWY3DPEHPK3PXP").is_ok());
    assert!(TotpSecretWidget.validate_value("not base32!").is_err());
    assert!(flags(&TotpSecretWidget, "not base32!", true));
    assert!(
        !flags(&TotpSecretWidget, "", true),
        "an empty secret is never judged"
    );
}

// ── passwords ────────────────────────────────────────────────────────────────

#[test]
fn password_widget_is_advisory_only() {
    let widget = PasswordWidget {
        rule: PasswordRule::Advisory,
    };

    assert_eq!(widget.timing(), Timing::OnChange);
    assert!(widget.validate_value("short").is_ok());
    assert!(widget
        .validate_value("correct horse battery staple")
        .is_ok());
    assert!(
        !flags(&widget, "short", true),
        "advisory strength never blocks a save"
    );
}

// ── pass-through widgets ──────────────────────────────────────────────────────

#[test]
fn free_text_widgets_never_flag() {
    assert_eq!(PlaintextWidget.timing(), Timing::Never);
    assert_eq!(NotesWidget.timing(), Timing::Never);
    assert_eq!(BooleanWidget.timing(), Timing::Never);

    for value in ["", "anything", "multi\nline\nnotes"] {
        assert!(PlaintextWidget.validate_value(value).is_ok());
        assert!(NotesWidget.validate_value(value).is_ok());
        assert!(BooleanWidget.validate_value(value).is_ok());
        assert!(!flags(&PlaintextWidget, value, true));
        assert!(!flags(&NotesWidget, value, true));
        assert!(!flags(&BooleanWidget, value, true));
    }
}
