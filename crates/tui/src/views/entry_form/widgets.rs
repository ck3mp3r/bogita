//! Field-widget behaviour: one zero-sized policy impl per form field type.
//!
//! The trait owns the value rule and when that rule may run. The rat-text
//! editors stay the editors — this layer rides above them. Construction happens
//! once per row in `EntryForm::validate`, and `confirm()` calls the same impls,
//! so the realtime door and the save door cannot drift apart.

use bogita_core::domain::SshKeyMaterial;
use bogita_core::service::otp::decode_secret;

// region:    --- Types

/// When a value rule may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Timing {
    /// The realtime loop never judges this value; another layer gates it.
    Never,
    /// Judge on every change, as soon as the value is non-empty.
    OnChange,
    /// Judge only a complete input: a closed armor while the slot is focused,
    /// any non-empty value once the slot is left.
    OnComplete,
}

/// The validation behaviour of one field type.
pub(super) trait FieldWidget {
    /// `Err(reason)` when `value` is not acceptable for this field type.
    fn validate_value(&self, value: &str) -> Result<(), String>;

    /// When the realtime loop may call [`Self::validate_value`].
    fn timing(&self) -> Timing;
}

// endregion: --- Types

// region:    --- Widgets

/// SSH private keys: the value must parse as an OpenSSH PEM.
pub(super) struct SshKeyWidget;

impl FieldWidget for SshKeyWidget {
    fn validate_value(&self, value: &str) -> Result<(), String> {
        SshKeyMaterial::validate(value)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn timing(&self) -> Timing {
        Timing::OnComplete
    }
}

/// TOTP secrets: the value must decode as base32, the same door the entity uses.
pub(super) struct TotpSecretWidget;

impl FieldWidget for TotpSecretWidget {
    fn validate_value(&self, value: &str) -> Result<(), String> {
        match decode_secret(value) {
            Ok(_) => Ok(()),
            Err(_) => Err("secret must be base32".to_string()),
        }
    }

    fn timing(&self) -> Timing {
        Timing::OnChange
    }
}

/// What a password row enforces.
///
/// MVP: advisory only. zxcvbn-style strength stays out of the blocking path, so
/// the rule accepts every value; a later rule lands here without touching the
/// widget plumbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PasswordRule {
    /// Accept any value; strength is advisory, never blocking.
    Advisory,
}

/// Passwords and tokens: advisory strength, no blocking rule yet.
pub(super) struct PasswordWidget {
    pub(super) rule: PasswordRule,
}

impl FieldWidget for PasswordWidget {
    fn validate_value(&self, _value: &str) -> Result<(), String> {
        match self.rule {
            PasswordRule::Advisory => Ok(()),
        }
    }

    fn timing(&self) -> Timing {
        Timing::OnChange
    }
}

/// Usernames and free text: any value is acceptable.
pub(super) struct PlaintextWidget;

impl FieldWidget for PlaintextWidget {
    fn validate_value(&self, _value: &str) -> Result<(), String> {
        Ok(())
    }

    fn timing(&self) -> Timing {
        Timing::Never
    }
}

/// Notes: multi-line free text, any value is acceptable.
pub(super) struct NotesWidget;

impl FieldWidget for NotesWidget {
    fn validate_value(&self, _value: &str) -> Result<(), String> {
        Ok(())
    }

    fn timing(&self) -> Timing {
        Timing::Never
    }
}

/// Boolean rows: the cell holds true/false, gated by the form-level
/// agent-servable rule rather than by this widget.
pub(super) struct BooleanWidget;

impl FieldWidget for BooleanWidget {
    fn validate_value(&self, _value: &str) -> Result<(), String> {
        Ok(())
    }

    fn timing(&self) -> Timing {
        Timing::Never
    }
}

// endregion: --- Widgets

// region:    --- Support

/// `true` when the PEM armor is both opened and closed.
///
/// Only a closed armor is a complete input; anything shorter is still being
/// typed or pasted and must not be judged yet.
pub(super) fn has_complete_armor(value: &str) -> bool {
    value.contains("-----BEGIN") && value.contains("-----END")
}

/// Apply one widget's rule with the focus mercy.
///
/// `focused` is the slot-exit signal: a complete-input rule waits for a closed
/// armor while the slot owns the focus, and judges as soon as the slot is left.
/// Empty values are never judged.
pub(super) fn flags<W: FieldWidget>(widget: &W, value: &str, focused: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    let may_judge = match widget.timing() {
        Timing::Never => false,
        Timing::OnChange => true,
        Timing::OnComplete => !focused || has_complete_armor(value),
    };
    may_judge && widget.validate_value(value).is_err()
}

// endregion: --- Support
