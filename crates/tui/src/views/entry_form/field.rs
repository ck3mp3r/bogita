//! Field types and the per-field widget state for the entry form.
//!
//! `FormFieldType` is the form-layer type: its `TABLE` drives dropdown order,
//! display labels, and the obscured/encrypted flag, and it maps to and from the
//! domain `FieldType`.

use bogita_core::domain::{Entry, FieldType, FieldValue, AGENT_SERVABLE_KEY};
use rat_widget::choice::ChoiceState;
use rat_widget::text_input::TextInputState;
use rat_widget::textarea::TextAreaState;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormAction {
    None,
    Confirm(Entry),
    ValidationError(String),
    Cancel,
    ConfirmDiscard,
    OpenVaultPicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationIssue {
    EmptyName,
    EmptyKey(usize),
    DuplicateKey(usize),
    InvalidAgentServable(usize),
    InvalidSshKey(usize),
    InvalidTotp(usize),
}

// ── FormFieldType ─────────────────────────────────────────────────────────────

/// Form-layer field type — drives rendering, encryption, and generator availability.
/// This is NOT the same as domain `FieldType`; it maps to domain types on confirm.
///
/// `TABLE` is the single source of truth: dropdown order, display label and the
/// obscured/encrypted flag all derive from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldType {
    Text,
    Username,
    Token,
    Totp,
    SshKey,
    Notes,
    Boolean,
}

impl FormFieldType {
    /// Dropdown order, display label, and whether the value is obscured/encrypted.
    pub(super) const TABLE: [(Self, &'static str, bool); 7] = [
        (Self::Text, "text", false),
        (Self::Username, "username", false),
        (Self::Token, "token", true),
        (Self::Totp, "totp", true),
        (Self::SshKey, "ssh-key", true),
        (Self::Notes, "notes", false),
        (Self::Boolean, "boolean", false),
    ];

    /// Number of selectable variants.
    pub(super) fn count() -> usize {
        Self::TABLE.len()
    }

    /// Variant at its dropdown index.
    pub(super) fn from_index(index: usize) -> Option<Self> {
        Self::TABLE.get(index).map(|(variant, ..)| variant.clone())
    }

    /// Display label shown in the type slot and dropdown.
    pub fn label(&self) -> &'static str {
        self.row().1
    }

    /// Whether the value should be visually obscured (masked with bullets).
    pub fn is_obscured(&self) -> bool {
        self.row().2
    }

    /// Whether the value is stored encrypted — same predicate as [`Self::is_obscured`].
    pub fn is_encrypted(&self) -> bool {
        self.is_obscured()
    }

    /// Index in the selector list (matches `TABLE` order).
    pub(super) fn selector_index(&self) -> usize {
        Self::TABLE
            .iter()
            .position(|(variant, ..)| variant == self)
            .unwrap_or(0)
    }

    fn row(&self) -> &'static (Self, &'static str, bool) {
        Self::TABLE
            .iter()
            .find(|(variant, ..)| variant == self)
            .unwrap_or(&Self::TABLE[0])
    }

    /// Map from a domain `FieldType` to the closest `FormFieldType`.
    pub(super) fn from_domain(ft: &FieldType) -> Self {
        match ft {
            FieldType::Username => Self::Username,
            FieldType::Token => Self::Token,
            FieldType::TotpSecret => Self::Totp,
            FieldType::SshPrivateKey => Self::SshKey,
            FieldType::Notes => Self::Notes,
            FieldType::Boolean => Self::Boolean,
            FieldType::Custom(name) if name == AGENT_SERVABLE_KEY => Self::Boolean,
            // Url and all other Custom variants → plain Text
            _ => Self::Text,
        }
    }

    /// Parse the agent-servable value slot text; `None` when it is neither `true`
    /// nor `false`. The value is Boolean on confirm only when this parses, so an
    /// unparsable slot never reaches storage as a gate.
    pub(super) fn parse_agent_servable(value: &str) -> Option<bool> {
        match value.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Map to the domain `FieldType` for this form type (key used for Custom).
    pub(super) fn to_domain_field_type(&self, key: &str) -> FieldType {
        match self {
            Self::Text => FieldType::Custom(key.to_string()),
            Self::Username => FieldType::Username,
            Self::Token => FieldType::Token,
            Self::Totp => FieldType::TotpSecret,
            Self::SshKey => FieldType::SshPrivateKey,
            Self::Notes => FieldType::Notes,
            Self::Boolean => FieldType::Boolean,
        }
    }

    /// Map to the domain `FieldValue` for this form type.
    pub(super) fn to_domain_value(&self, value: &str) -> FieldValue {
        match self {
            Self::Token | Self::Totp | Self::SshKey => FieldValue::Hidden(value.to_string()),
            Self::Boolean => {
                FieldValue::Boolean(Self::parse_agent_servable(value).unwrap_or(false))
            }
            _ => FieldValue::Text(value.to_string()),
        }
    }
}

// ── Field descriptor ──────────────────────────────────────────────────────────

pub(super) struct FormField {
    pub(super) key_state: TextInputState,
    pub(super) value_state: TextInputState,
    pub(super) field_type: FormFieldType,
    /// ChoiceState for the type selector — replaces the custom dropdown overlay.
    /// Uses `usize` as the value type (index into `ALL_VARIANTS`).
    pub(super) type_state: ChoiceState<usize>,
    /// TextAreaState for Notes fields — `Some` when field_type is Notes, `None` otherwise.
    pub(super) textarea_state: Option<TextAreaState>,
    /// Whether the user has typed in the key slot. Used to avoid flagging
    /// newly-added blank fields as validation errors immediately.
    pub(super) touched: bool,
    /// Whether the loaded row was recognised as the agent-servable gate. The
    /// stored `Custom("agent_servable")` field type is lost in `from_domain`,
    /// so the row carries the recognition itself.
    pub(super) agent_servable: bool,
}

impl FormField {
    pub(super) fn empty() -> Self {
        Self {
            key_state: TextInputState::named("key"),
            value_state: TextInputState::named("value"),
            field_type: FormFieldType::Text,
            type_state: ChoiceState::named("type"),
            textarea_state: None,
            touched: false,
            agent_servable: false,
        }
    }

    /// Whether this row IS the agent-servable gate: recognised at load from the
    /// stored key or field type, or recognised now from the key slot text.
    pub(super) fn is_agent_servable(&self) -> bool {
        self.agent_servable || self.key_state.text() == AGENT_SERVABLE_KEY
    }
}

/// Identifies which slot kind is currently focused, without requiring &mut self.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Name,
    Key(usize),
    Value(usize),
    Type(usize),
}
