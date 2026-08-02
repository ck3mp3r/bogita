//! Entry add/edit form — collects and validates entry data before persisting.
//!
//! ## Focus system
//!
//! Focus is managed by **rat-focus** [`FocusFlags`] — one flag per slot:
//! name, key, value, and type for each user-defined field.
//!
//! Tab / BackTab navigation uses [`Focus::next_force`] / [`Focus::prev_force`]
//! to cycle through all slots without delegating the key event to the focused
//! widget (which would consume Tab as a character insertion).
//!
//! ## Flat-index backward compatibility
//!
//! [`focused_field()`](Self::focused_field) returns a flat index for
//! compatibility with `app.rs` status-bar calculations:
//!
//! - `0`        → name
//! - `1 + i*3`  → key slot of user-defined field `i`
//! - `2 + i*3`  → value slot of user-defined field `i`
//! - `3 + i*3`  → type slot of field `i`
//!
//! Total slots = `1 + fields.len() * 3`
//!
//! ## Form-level keys
//!
//! `Enter` always confirms (with validation) regardless of which slot is focused.
//! `Esc` always cancels the form.
//! On a type slot, `j`/`k`/`Up`/`Down` navigate the selector and open the popup; the type is applied when the popup closes.
//! `+` / `-` add / remove fields (available from any slot on the name row or key/type slots).

use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue};
use chrono::Utc;
use rat_event::{HandleEvent, Outcome, Regular};
use rat_focus::{Focus, FocusBuilder};
use rat_popup::Placement;
use rat_widget::choice::{Choice, ChoiceState};
use rat_widget::event::ChoiceOutcome;
use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::{TextInput, TextInputState};
use rat_widget::textarea::{TextArea, TextAreaState};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::collections::HashSet;
use uuid::Uuid;

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
}

// ── FormFieldType ─────────────────────────────────────────────────────────────

/// Form-layer field type — drives rendering, encryption, and generator availability.
/// This is NOT the same as domain `FieldType`; it maps to domain types on confirm.
///
/// Variants (ordered by dropdown index):
///   0 = Text      — plain text, not encrypted
///   1 = Username  — plain text, maps to FieldType::Username
///   2 = Token  — masked, encrypted, [g] available
///   3 = Totp      — masked, encrypted, live OTP in detail
///   4 = SshKey    — masked, encrypted, SSH agent support
///   5 = Notes     — multi-line text, not encrypted
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldType {
    Text,
    Username,
    Token,
    Totp,
    SshKey,
    Notes,
}

/// All variants in dropdown order — used for index ↔ variant conversion.
const ALL_VARIANTS: [FormFieldType; 6] = [
    FormFieldType::Text,
    FormFieldType::Username,
    FormFieldType::Token,
    FormFieldType::Totp,
    FormFieldType::SshKey,
    FormFieldType::Notes,
];

impl FormFieldType {
    /// Display label shown in the type slot and dropdown.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Username => "username",
            Self::Token => "token",
            Self::Totp => "totp",
            Self::SshKey => "ssh-key",
            Self::Notes => "notes",
        }
    }

    /// Whether the value should be visually obscured (masked with bullets).
    pub fn is_obscured(&self) -> bool {
        matches!(self, Self::Token | Self::Totp | Self::SshKey)
    }

    /// Whether the value is stored encrypted.
    pub fn is_encrypted(&self) -> bool {
        matches!(self, Self::Token | Self::Totp | Self::SshKey)
    }

    /// Index in the selector list (matches `ALL_VARIANTS`).
    fn selector_index(&self) -> usize {
        match self {
            Self::Text => 0,
            Self::Username => 1,
            Self::Token => 2,
            Self::Totp => 3,
            Self::SshKey => 4,
            Self::Notes => 5,
        }
    }

    /// Map from a domain `FieldType` to the closest `FormFieldType`.
    fn from_domain(ft: &FieldType) -> Self {
        match ft {
            FieldType::Username => Self::Username,
            FieldType::Token => Self::Token,
            FieldType::TotpSecret => Self::Totp,
            FieldType::SshPrivateKey => Self::SshKey,
            FieldType::Notes => Self::Notes,
            // Url and all Custom variants → plain Text
            _ => Self::Text,
        }
    }

    /// Map to the domain `FieldType` for this form type (key used for Custom).
    fn to_domain_field_type(&self, key: &str) -> FieldType {
        match self {
            Self::Text => FieldType::Custom(key.to_string()),
            Self::Username => FieldType::Username,
            Self::Token => FieldType::Token,
            Self::Totp => FieldType::TotpSecret,
            Self::SshKey => FieldType::SshPrivateKey,
            Self::Notes => FieldType::Notes,
        }
    }

    /// Map to the domain `FieldValue` for this form type.
    fn to_domain_value(&self, value: &str) -> FieldValue {
        match self {
            Self::Token | Self::Totp | Self::SshKey => FieldValue::Hidden(value.to_string()),
            _ => FieldValue::Text(value.to_string()),
        }
    }
}

// ── Field descriptor ──────────────────────────────────────────────────────────

struct FormField {
    key_state: TextInputState,
    value_state: TextInputState,
    field_type: FormFieldType,
    /// ChoiceState for the type selector — replaces the custom dropdown overlay.
    /// Uses `usize` as the value type (index into `ALL_VARIANTS`).
    type_state: ChoiceState<usize>,
    /// TextAreaState for Notes fields — `Some` when field_type is Notes, `None` otherwise.
    textarea_state: Option<TextAreaState>,
    /// Whether the user has typed in the key slot. Used to avoid flagging
    /// newly-added blank fields as validation errors immediately.
    touched: bool,
}

impl FormField {
    fn empty() -> Self {
        Self {
            key_state: TextInputState::named("key"),
            value_state: TextInputState::named("value"),
            field_type: FormFieldType::Text,
            type_state: ChoiceState::named("type"),
            textarea_state: None,
            touched: false,
        }
    }
}

// ── EntryForm ─────────────────────────────────────────────────────────────────

/// Snapshot of the original entry for dirty-state tracking.
/// `(name, Vec<(key, value, field_type)>)`.
type OriginalSnapshot = (String, Vec<(String, String, FormFieldType)>);

pub struct EntryForm {
    mode: FormMode,
    entry_id: Uuid,
    vault_id: Uuid,
    vault_name: String,
    name_state: TextInputState,
    fields: Vec<FormField>,
    focus: Focus,
    /// Pending high-level action set during HandleEvent handling.
    /// Consumed by `handle_key` to return the appropriate `FormAction`.
    pending_action: FormAction,
    /// Scroll offset for field list — how many fields are scrolled past the top.
    /// The name field is always visible (pinned at top, not part of scroll range).
    pub scroll_offset: usize,
    /// Last known visible rows count, updated during render().
    /// Used by auto_scroll_after_tab() and add_field() to determine
    /// how many field rows fit in the current overlay area.
    pub last_visible_rows: usize,
    /// Indices of fields whose secret value is currently shown in plaintext.
    /// Auto-revealed on focus, auto-unrevealed on focus leave.
    /// Can be toggled manually with Ctrl-r.
    pub revealed_secret_fields: HashSet<usize>,
    /// Snapshot of the original entry for dirty-state tracking.
    /// `None` for Add mode; `Some((name, fields))` for Edit mode.
    original: Option<OriginalSnapshot>,
    /// Validation errors detected after the most recent key event.
    /// Updated by `validate()` at the end of every `handle_event_inner()` call.
    pub validation_errors: HashSet<ValidationIssue>,
}

/// Identifies which slot kind is currently focused, without requiring &mut self.
enum SlotKind {
    Name,
    Key(usize),
    Value(usize),
    Type(usize),
}

impl EntryForm {
    // ── constructors ──────────────────────────────────────────────────────────

    pub fn new_add(name: Option<String>) -> Self {
        let mut name_state = TextInputState::named("name");
        name_state.focus.set(true);
        if let Some(n) = name {
            name_state.set_text(n);
            name_state.set_cursor(name_state.len(), false);
        }
        Self {
            mode: FormMode::Add,
            entry_id: Uuid::new_v4(),
            vault_id: Uuid::nil(),
            vault_name: String::new(),
            name_state,
            fields: Vec::new(),
            focus: Focus::default(),
            pending_action: FormAction::None,
            scroll_offset: 0,
            last_visible_rows: 20,
            revealed_secret_fields: HashSet::new(),
            original: None,
            validation_errors: HashSet::new(),
        }
    }

    pub fn new_edit(entry: &Entry) -> Self {
        let fields = entry
            .fields
            .iter()
            .map(|f| {
                let value = match &f.value {
                    FieldValue::Text(s)
                    | FieldValue::Hidden(s)
                    | FieldValue::Url(s)
                    | FieldValue::Email(s) => s.clone(),
                    FieldValue::Boolean(b) => b.to_string(),
                    FieldValue::Number(n) => n.to_string(),
                    FieldValue::Date(ts) => ts.to_string(),
                };
                let field_type = FormFieldType::from_domain(&f.field_type);
                let mut key_state = TextInputState::named("key");
                key_state.set_text(&f.key);
                key_state.set_cursor(key_state.len(), false);
                let mut value_state = TextInputState::named("value");
                value_state.set_text(&value);
                value_state.set_cursor(value_state.len(), false);
                let textarea_state = if field_type == FormFieldType::Notes {
                    let mut state = TextAreaState::named("notes-value");
                    state.set_text(&value);
                    Some(state)
                } else {
                    None
                };
                let mut type_state = ChoiceState::named("type");
                type_state.set_value(field_type.selector_index());
                FormField {
                    key_state,
                    value_state,
                    field_type,
                    type_state,
                    textarea_state,
                    touched: true,
                }
            })
            .collect();

        let mut name_state = TextInputState::named("name");
        name_state.set_text(&entry.name);
        name_state.set_cursor(name_state.len(), false);
        name_state.focus.set(true);

        // Build original snapshot for dirty-state tracking.
        let original_name = entry.name.clone();
        let original_fields: Vec<(String, String, FormFieldType)> = entry
            .fields
            .iter()
            .map(|f| {
                let value = match &f.value {
                    FieldValue::Text(s)
                    | FieldValue::Hidden(s)
                    | FieldValue::Url(s)
                    | FieldValue::Email(s) => s.clone(),
                    FieldValue::Boolean(b) => b.to_string(),
                    FieldValue::Number(n) => n.to_string(),
                    FieldValue::Date(ts) => ts.to_string(),
                };
                (
                    f.key.clone(),
                    value,
                    FormFieldType::from_domain(&f.field_type),
                )
            })
            .collect();

        Self {
            mode: FormMode::Edit,
            entry_id: entry.id,
            vault_id: entry.vault_id,
            vault_name: String::new(),
            name_state,
            fields,
            focus: Focus::default(),
            pending_action: FormAction::None,
            scroll_offset: 0,
            last_visible_rows: 20,
            revealed_secret_fields: HashSet::new(),
            original: Some((original_name, original_fields)),
            validation_errors: HashSet::new(),
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    pub fn mode(&self) -> FormMode {
        self.mode.clone()
    }

    pub fn name(&self) -> &str {
        self.name_state.text()
    }

    pub fn set_vault_id(&mut self, id: Uuid) {
        self.vault_id = id;
    }

    /// Set both vault_id and vault_name. Used when the user picks a vault
    /// from the vault picker or when the default vault is resolved on startup.
    pub fn set_vault(&mut self, vault_id: Uuid, vault_name: String) {
        self.vault_id = vault_id;
        self.vault_name = vault_name;
    }

    /// Overwrite the value in the currently focused value/password slot.
    /// No-op if the focused slot is not a value slot.
    pub fn set_focused_value(&mut self, value: String) {
        for f in &mut self.fields {
            let value_focused = if f.field_type == FormFieldType::Notes {
                f.textarea_state.as_ref().is_some_and(|s| s.focus.get())
            } else {
                f.value_state.focus.get()
            };
            if value_focused {
                if let Some(state) = f.textarea_state.as_mut() {
                    state.set_text(&value);
                } else {
                    f.value_state.set_text(&value);
                    f.value_state.set_cursor(f.value_state.len(), false);
                }
                return;
            }
        }
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Returns the old-style flat slot index for the currently focused slot.
    /// 0 = name, 1 + i*3 = key, 2 + i*3 = value, 3 + i*3 = type.
    /// Used by app.rs for status bar calculations.
    pub fn focused_field(&self) -> usize {
        if self.name_state.focus.get() {
            return 0;
        }
        for (i, f) in self.fields.iter().enumerate() {
            if f.key_state.focus.get() {
                return 1 + i * 3;
            }
            if f.field_type == FormFieldType::Notes {
                if let Some(state) = f.textarea_state.as_ref() {
                    if state.focus.get() {
                        return 2 + i * 3;
                    }
                }
            } else if f.value_state.focus.get() {
                return 2 + i * 3;
            }
            if f.type_state.focus.get() {
                return 3 + i * 3;
            }
        }
        0
    }

    /// `true` when the currently focused slot is an obscured value slot (i.e. the
    /// value should be displayed in plain text because the user is actively editing it).
    pub fn focused_value_is_revealed(&self) -> bool {
        for f in &self.fields {
            let value_focused = if f.field_type == FormFieldType::Notes {
                f.textarea_state.as_ref().is_some_and(|s| s.focus.get())
            } else {
                f.value_state.focus.get()
            };
            if value_focused {
                return f.field_type.is_obscured();
            }
        }
        false
    }

    /// `true` when the currently focused slot belongs to a field whose type is obscured.
    pub fn focused_field_is_obscured(&self) -> bool {
        for f in &self.fields {
            let any_focused = f.key_state.focus.get()
                || f.type_state.focus.get()
                || if f.field_type == FormFieldType::Notes {
                    f.textarea_state.as_ref().is_some_and(|s| s.focus.get())
                } else {
                    f.value_state.focus.get()
                };
            if any_focused {
                return f.field_type.is_obscured();
            }
        }
        false
    }

    /// Human-readable label for the currently focused slot.
    ///
    /// - `"name"` when on the name slot.
    /// - `"key"` when on a field key slot.
    /// - `"value"` when on a plain (non-obscured) field value slot.
    /// - `"token"` when on an obscured field value slot.
    /// - `"type"` when on the type selector slot.
    pub fn focused_slot_label(&self) -> &'static str {
        if self.name_state.focus.get() {
            return "name";
        }
        for f in &self.fields {
            if f.key_state.focus.get() {
                return "key";
            }
            let value_focused = if f.field_type == FormFieldType::Notes {
                f.textarea_state.as_ref().is_some_and(|s| s.focus.get())
            } else {
                f.value_state.focus.get()
            };
            if value_focused {
                return if f.field_type.is_obscured() {
                    "token"
                } else {
                    "value"
                };
            }
            if f.type_state.focus.get() {
                return "type";
            }
        }
        "name"
    }

    /// Badge string for field at `index` — returns the `FormFieldType` label,
    /// or `""` if index is out of bounds.
    pub fn field_badge(&self, index: usize) -> &'static str {
        match self.fields.get(index) {
            Some(f) => f.field_type.label(),
            None => "",
        }
    }

    /// Whether the form has unsaved changes.
    ///
    /// For Add mode (`original == None`): dirty if name is non-empty OR any field
    /// has a non-empty key or value.
    ///
    /// For Edit mode (`original == Some(...)`): dirty if the current name, field
    /// count, or any field's key/value/type differs from the original snapshot.
    pub fn is_dirty(&self) -> bool {
        match &self.original {
            None => {
                // Add mode: dirty if name is non-empty or any field exists.
                if !self.name_state.text().is_empty() {
                    return true;
                }
                if !self.fields.is_empty() {
                    return true;
                }
                false
            }
            Some((orig_name, orig_fields)) => {
                // Edit mode: compare current state to original snapshot.
                if self.name_state.text() != orig_name {
                    return true;
                }
                if self.fields.len() != orig_fields.len() {
                    return true;
                }
                for (i, f) in self.fields.iter().enumerate() {
                    let (ref orig_key, ref orig_value, ref orig_type) = &orig_fields[i];
                    if f.key_state.text() != orig_key {
                        return true;
                    }
                    if self.field_value_text(f) != *orig_value {
                        return true;
                    }
                    if f.field_type != *orig_type {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Get the current text value of a field, handling TextAreaState for Notes.
    fn field_value_text(&self, f: &FormField) -> String {
        if let Some(state) = f.textarea_state.as_ref() {
            state.text().to_string()
        } else {
            f.value_state.text().to_string()
        }
    }

    /// Re-run validation and update `validation_errors`.
    /// Called at the end of every `handle_event_inner()` call.
    fn validate(&mut self) {
        self.validation_errors.clear();

        // Name must not be empty.
        if self.name_state.text().is_empty() {
            self.validation_errors.insert(ValidationIssue::EmptyName);
        }

        // Check each field for empty keys (only if touched) and duplicate keys.
        for (i, f) in self.fields.iter().enumerate() {
            if f.key_state.text().is_empty() && f.touched {
                self.validation_errors.insert(ValidationIssue::EmptyKey(i));
            }
        }

        // Check for duplicate keys.
        for (i, f) in self.fields.iter().enumerate() {
            if !f.key_state.text().is_empty() {
                for (j, f2) in self.fields.iter().enumerate() {
                    if i != j && f.key_state.text() == f2.key_state.text() {
                        self.validation_errors
                            .insert(ValidationIssue::DuplicateKey(i));
                        break;
                    }
                }
            }
        }
    }

    /// Whether the form has any validation errors.
    pub fn has_validation_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    /// A short human-readable summary of the first validation error.
    pub fn validation_error_summary(&self) -> String {
        if self.validation_errors.contains(&ValidationIssue::EmptyName) {
            return "Name must not be empty".to_string();
        }
        let empty_keys = self
            .validation_errors
            .iter()
            .filter(|e| matches!(e, ValidationIssue::EmptyKey(_)))
            .count();
        if empty_keys > 0 {
            return format!(
                "{empty_keys} field{} need keys",
                if empty_keys > 1 { "s" } else { "" }
            );
        }
        let dupes = self
            .validation_errors
            .iter()
            .filter(|e| matches!(e, ValidationIssue::DuplicateKey(_)))
            .count();
        if dupes > 0 {
            return format!("{dupes} duplicate key{}", if dupes > 1 { "s" } else { "" });
        }
        String::new()
    }

    /// Public accessor for validation errors (used in tests).
    pub fn validation_errors(&self) -> &HashSet<ValidationIssue> {
        &self.validation_errors
    }

    // ── focus helpers ─────────────────────────────────────────────────────────

    /// Rebuild the Focus container from the current widget list.
    /// Recycles storage from the previous Focus to avoid reallocation.
    fn rebuild_focus(&mut self) -> &mut Focus {
        let old = std::mem::take(&mut self.focus);
        let mut fb = FocusBuilder::new(Some(old));
        fb.widget(&self.name_state);
        for ff in &self.fields {
            fb.widget(&ff.key_state);
            if ff.field_type == FormFieldType::Notes {
                if let Some(state) = ff.textarea_state.as_ref() {
                    fb.widget(state);
                }
            } else {
                fb.widget(&ff.value_state);
            }
            fb.widget(&ff.type_state);
        }
        self.focus = fb.build();
        &mut self.focus
    }

    fn focused_slot_kind(&self) -> SlotKind {
        if self.name_state.focus.get() {
            return SlotKind::Name;
        }
        for (i, f) in self.fields.iter().enumerate() {
            if f.key_state.focus.get() {
                return SlotKind::Key(i);
            }
            if f.field_type == FormFieldType::Notes {
                if let Some(state) = f.textarea_state.as_ref() {
                    if state.focus.get() {
                        return SlotKind::Value(i);
                    }
                }
            } else if f.value_state.focus.get() {
                return SlotKind::Value(i);
            }
            if f.type_state.focus.get() {
                return SlotKind::Type(i);
            }
        }
        SlotKind::Name
    }

    // ── key handling ──────────────────────────────────────────────────────────

    /// Thin wrapper around `HandleEvent` — preserves the public API so
    /// `app.rs` and all tests work unchanged.
    ///
    /// Converts BackTab → Tab+SHIFT so rat-focus can match it. All other
    /// modifiers are stripped. Ctrl-r reveal toggle is NOT handled here —
    /// use `handle_key_with_modifiers` for that.
    pub fn handle_key(&mut self, key: KeyCode) -> FormAction {
        let modifiers = match key {
            KeyCode::BackTab => KeyModifiers::SHIFT,
            _ => KeyModifiers::empty(),
        };
        self.handle_key_with_modifiers(key, modifiers)
    }

    /// Handle a key event with full modifier info.
    /// Used by app.rs to pass Ctrl-r for toggle reveal.
    pub fn handle_key_with_modifiers(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> FormAction {
        let event = Event::Key(KeyEvent::new_with_kind_and_state(
            key,
            modifiers,
            KeyEventKind::Press,
            KeyEventState::NONE,
        ));
        let _ = self.handle(&event, Regular);
        std::mem::replace(&mut self.pending_action, FormAction::None)
    }

    // ── HandleEvent implementation ───────────────────────────────────────────

    /// Route an event to the appropriate slot handler based on focus.
    /// Structured for future delegation to rat-widget widgets.
    fn handle_event_inner(&mut self, event: &Event) -> Outcome {
        // Re-run validation before every event so visual feedback is immediate
        // and confirm() has up-to-date validation_errors.
        self.validate();

        // Ctrl-V opens the vault picker from anywhere in the form.
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press
                && ke.code == KeyCode::Char('v')
                && ke.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.pending_action = FormAction::OpenVaultPicker;
                return Outcome::Changed;
            }
        }

        // 1. Handle Tab/BackTab navigation directly.
        //    We call Focus::next()/prev() instead of Focus::handle() to avoid
        //    delegating the event to the focused widget (e.g. TextInputState),
        //    which would consume Tab as a tab character insertion.
        //    Note: handle_key() converts BackTab → Tab + SHIFT before creating
        //    the event, so we must check for both BackTab and Tab+SHIFT.
        //    If next()/prev() returns false (e.g. Navigation::Reach on TextAreaState),
        //    we fall through to the slot handler so the widget can handle Tab itself.
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                let is_shift_tab =
                    ke.code == KeyCode::Tab && ke.modifiers.contains(KeyModifiers::SHIFT);
                if ke.code == KeyCode::Tab
                    && !ke.modifiers.contains(KeyModifiers::SHIFT)
                    && self.rebuild_focus().next_force()
                {
                    self.sync_reveal_state();
                    self.auto_scroll_after_tab();
                    self.sync_cursor_after_tab();
                    return Outcome::Changed;
                }
                if (ke.code == KeyCode::BackTab || is_shift_tab)
                    && self.rebuild_focus().prev_force()
                {
                    self.sync_reveal_state();
                    self.auto_scroll_after_tab();
                    self.sync_cursor_after_tab();
                    return Outcome::Changed;
                }
            }
        }

        // 2. Route to the focused slot handler.
        let outcome = match self.focused_slot_kind() {
            SlotKind::Name => self.handle_name_event(event),
            SlotKind::Type(i) => self.handle_type_event(event, i),
            SlotKind::Key(i) | SlotKind::Value(i) => self.handle_text_event(event, i),
        };
        // Re-run validation after every event so visual feedback is immediate.
        self.validate();
        outcome
    }

    /// Handle events when focused on the name slot.
    fn handle_name_event(&mut self, event: &Event) -> Outcome {
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                match ke.code {
                    KeyCode::Esc => {
                        self.pending_action = if self.is_dirty() {
                            FormAction::ConfirmDiscard
                        } else {
                            FormAction::Cancel
                        };
                        self.revealed_secret_fields.clear();
                        return Outcome::Changed;
                    }
                    KeyCode::Enter => {
                        self.pending_action = self.confirm();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('+') => {
                        self.add_field();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('-') => return Outcome::Continue,
                    _ => {}
                }
            }
        }
        self.name_state.handle(event, Regular).into()
    }

    /// Handle events when focused on a type slot.
    ///
    /// 1. Popup open → delegate ALL events to ChoiceState, then sync field_type
    ///    after popup closes. Return early.
    /// 2. Popup closed → intercept form-level keys (Esc=cancel, Enter=confirm,
    ///    +=add_field, -=remove_field). Return early on match.
    /// 3. Popup closed → handle j/k/Up/Down with wrapping (ChoiceState treats j/k
    ///    as character search, so we handle them ourselves). Return early on match.
    /// 4. Popup closed → delegate remaining events to ChoiceState (Space toggles popup).
    fn handle_type_event(&mut self, event: &Event, idx: usize) -> Outcome {
        // 1. If popup is open, handle navigation and delegation
        if idx < self.fields.len() && self.fields[idx].type_state.is_popup_active() {
            // Intercept j/k/Up/Down for navigation (ChoiceState treats j/k as char search)
            if let Event::Key(ke) = event {
                if ke.kind == KeyEventKind::Press {
                    let count = ALL_VARIANTS.len();
                    match ke.code {
                        KeyCode::Char('j') | KeyCode::Down => {
                            if let Some(f) = self.fields.get_mut(idx) {
                                let cur = f.type_state.value();
                                let next = (cur + 1) % count;
                                f.type_state.set_value(next);
                            }
                            return Outcome::Changed;
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if let Some(f) = self.fields.get_mut(idx) {
                                let cur = f.type_state.value();
                                let prev = if cur == 0 { count - 1 } else { cur - 1 };
                                f.type_state.set_value(prev);
                            }
                            return Outcome::Changed;
                        }
                        KeyCode::Esc => {
                            // Esc closes popup without applying selection
                            if let Some(f) = self.fields.get_mut(idx) {
                                f.type_state.set_popup_active(false);
                            }
                            return Outcome::Changed;
                        }
                        _ => {}
                    }
                }
            }
            // Delegate remaining events to ChoiceState (Enter closes popup, etc.)
            if let Some(f) = self.fields.get_mut(idx) {
                let _: ChoiceOutcome = f.type_state.handle(event, Regular);
                if !f.type_state.is_popup_active() {
                    let selected = f.type_state.value();
                    if let Some(new_type) = ALL_VARIANTS.get(selected) {
                        f.field_type = new_type.clone();
                        self.ensure_textarea(idx);
                    }
                }
            }
            return Outcome::Changed;
        }

        // 2. Popup closed — intercept form-level keys
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                match ke.code {
                    KeyCode::Esc => {
                        self.pending_action = if self.is_dirty() {
                            FormAction::ConfirmDiscard
                        } else {
                            FormAction::Cancel
                        };
                        self.revealed_secret_fields.clear();
                        return Outcome::Changed;
                    }
                    KeyCode::Enter => {
                        self.pending_action = self.confirm();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('+') => {
                        self.add_field();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('-') => {
                        self.remove_focused_field();
                        return Outcome::Changed;
                    }
                    _ => {}
                }
            }
        }

        // 3. Handle j/k/Up/Down with wrapping
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                let count = ALL_VARIANTS.len();
                match ke.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            let cur = f.type_state.value();
                            let next = (cur + 1) % count;
                            f.type_state.set_value(next);
                            f.field_type = ALL_VARIANTS[next].clone();
                            self.ensure_textarea(idx);
                        }
                        return Outcome::Changed;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            let cur = f.type_state.value();
                            let prev = if cur == 0 { count - 1 } else { cur - 1 };
                            f.type_state.set_value(prev);
                            f.field_type = ALL_VARIANTS[prev].clone();
                            self.ensure_textarea(idx);
                        }
                        return Outcome::Changed;
                    }
                    _ => {}
                }
            }
        }

        // 4. Delegate remaining events to ChoiceState (Space toggles popup)
        if let Some(f) = self.fields.get_mut(idx) {
            let _: ChoiceOutcome = f.type_state.handle(event, Regular);
        }
        Outcome::Changed
    }

    /// Handle events when focused on a key or value text slot.
    fn handle_text_event(&mut self, event: &Event, idx: usize) -> Outcome {
        let slot = self.focused_slot_kind();

        // Notes value slots: delegate to TextAreaState (Enter = newline)
        if let SlotKind::Value(_) = slot {
            if let Some(f) = self.fields.get_mut(idx) {
                if f.field_type == FormFieldType::Notes {
                    if let Some(state) = f.textarea_state.as_mut() {
                        if matches!(event, Event::Key(ke) if ke.kind == KeyEventKind::Press && ke.code == KeyCode::Esc)
                        {
                            self.pending_action = if self.is_dirty() {
                                FormAction::ConfirmDiscard
                            } else {
                                FormAction::Cancel
                            };
                            self.revealed_secret_fields.clear();
                            return Outcome::Changed;
                        }
                        let outcome: Outcome = state.handle(event, Regular).into();
                        return outcome;
                    }
                }
            }
        }

        // Non-Notes: intercept form-level keys first
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                match ke.code {
                    KeyCode::Esc => {
                        self.pending_action = if self.is_dirty() {
                            FormAction::ConfirmDiscard
                        } else {
                            FormAction::Cancel
                        };
                        self.revealed_secret_fields.clear();
                        return Outcome::Changed;
                    }
                    KeyCode::Enter => {
                        self.pending_action = self.confirm();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('+') if matches!(slot, SlotKind::Key(_)) => {
                        self.add_field();
                        return Outcome::Changed;
                    }
                    KeyCode::Char('-') if matches!(slot, SlotKind::Key(_)) => {
                        self.remove_focused_field();
                        return Outcome::Changed;
                    }
                    // Ctrl-r toggles reveal on the currently focused obscured value slot
                    KeyCode::Char('r') if ke.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let SlotKind::Value(i) = slot {
                            if let Some(f) = self.fields.get(i) {
                                if f.field_type.is_obscured() {
                                    if self.revealed_secret_fields.contains(&i) {
                                        self.revealed_secret_fields.remove(&i);
                                    } else {
                                        self.revealed_secret_fields.insert(i);
                                    }
                                    return Outcome::Changed;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Delegate to TextInputState
        if let Some(f) = self.fields.get_mut(idx) {
            match slot {
                SlotKind::Key(_) => {
                    // Mark the field as touched when the user types in the key slot.
                    // This prevents newly-added blank fields from being flagged immediately.
                    if let Event::Key(ke) = event {
                        if ke.kind == KeyEventKind::Press
                            && matches!(
                                ke.code,
                                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
                            )
                        {
                            f.touched = true;
                        }
                    }
                    return f.key_state.handle(event, Regular).into();
                }
                SlotKind::Value(_) => {
                    return f.value_state.handle(event, Regular).into();
                }
                _ => {}
            }
        }
        Outcome::Changed
    }

    fn add_field(&mut self) {
        // Close any open type popups
        for f in &mut self.fields {
            f.type_state.set_popup_active(false);
        }
        // Clear all focus flags before adding
        self.name_state.focus.set(false);
        for f in &self.fields {
            f.key_state.focus.set(false);
            f.value_state.focus.set(false);
            f.type_state.focus.set(false);
            if let Some(state) = f.textarea_state.as_ref() {
                state.focus.set(false);
            }
        }
        let new_field = FormField::empty();
        new_field.key_state.focus.set(true);
        self.fields.push(new_field);
        // Scroll to show the new field if there are more fields than visible.
        let visible = self.last_visible_rows.max(1);
        if self.fields.len() > visible {
            self.scroll_offset = self.fields.len().saturating_sub(visible);
        }
    }

    /// Ensure the TextAreaState for a Notes field is initialised (or cleaned up
    /// when the type changes away from Notes). Called after a type change.
    fn ensure_textarea(&mut self, field_idx: usize) {
        if let Some(f) = self.fields.get_mut(field_idx) {
            if f.field_type == FormFieldType::Notes {
                if f.textarea_state.is_none() {
                    let mut state = TextAreaState::named("notes-value");
                    state.set_text(f.value_state.text());
                    f.textarea_state = Some(state);
                }
            } else {
                if let Some(state) = f.textarea_state.take() {
                    f.value_state.set_text(state.text());
                    f.value_state.set_cursor(f.value_state.len(), false);
                }
            }
        }
    }

    fn remove_focused_field(&mut self) {
        // Close any open type popups
        for f in &mut self.fields {
            f.type_state.set_popup_active(false);
        }
        let field_idx = match self.focused_slot_kind() {
            SlotKind::Name => return,
            SlotKind::Key(i) | SlotKind::Value(i) | SlotKind::Type(i) => i,
        };
        if field_idx < self.fields.len() {
            self.fields.remove(field_idx);
            // Adjust scroll_offset to avoid empty space at the end.
            let visible = self.last_visible_rows.max(1);
            if self.fields.len() > visible {
                let max_scroll = self.fields.len() - visible;
                if self.scroll_offset > max_scroll {
                    self.scroll_offset = max_scroll;
                }
            } else {
                self.scroll_offset = 0;
            }
            // Set focus to name field
            self.name_state.focus.set(true);
        }
    }

    pub fn confirm(&mut self) -> FormAction {
        // Clear reveal state before constructing the Entry.
        self.revealed_secret_fields.clear();
        // Close any open type popups and sync field_type from Choice widget.
        for f in &mut self.fields {
            f.type_state.set_popup_active(false);
            let selected = f.type_state.value();
            if let Some(new_type) = ALL_VARIANTS.get(selected) {
                f.field_type = new_type.clone();
            }
        }
        if !self.validation_errors.is_empty() {
            let messages: Vec<String> = self
                .validation_errors
                .iter()
                .map(|e| match e {
                    ValidationIssue::EmptyName => "Name must not be empty".to_string(),
                    ValidationIssue::EmptyKey(i) => {
                        format!("Field {} key must not be empty", i + 1)
                    }
                    ValidationIssue::DuplicateKey(i) => {
                        format!("Field {} has a duplicate key", i + 1)
                    }
                })
                .collect();
            return FormAction::ValidationError(messages.join("; "));
        }
        let now = Utc::now().timestamp();
        let domain_fields: Vec<Field> = self
            .fields
            .iter()
            .enumerate()
            .map(|(i, ff)| {
                // Sync TextAreaState text back to value for Notes fields
                let value = if let Some(state) = ff.textarea_state.as_ref() {
                    state.text()
                } else {
                    ff.value_state.text().to_string()
                };
                Field {
                    id: Uuid::new_v4(),
                    key: ff.key_state.text().to_string(),
                    value: ff.field_type.to_domain_value(&value),
                    field_type: ff.field_type.to_domain_field_type(ff.key_state.text()),
                    encrypted: ff.field_type.is_encrypted(),
                    idx: i as i32,
                }
            })
            .collect();

        FormAction::Confirm(Entry {
            id: self.entry_id,
            vault_id: self.vault_id,
            name: self.name_state.text().trim().to_string(),
            entry_type: EntryType::Token,
            created_at: now,
            modified_at: now,
            fields: domain_fields,
        })
    }

    // ── scrolling ────────────────────────────────────────────────────────────

    /// Calculate how many field rows fit in the given area.
    /// Each field row is 3 lines. Name takes 3 lines, hint bar takes 1 line.
    fn visible_rows(&self, inner_height: u16) -> usize {
        let available = inner_height.saturating_sub(1 + 3 + 1); // vault(1) + name(3) + hint(1)
        (available / 3) as usize
    }

    /// After Tab/BackTab navigation, sync the reveal state so that only the
    /// currently focused obscured value slot is revealed. All other fields are
    /// hidden. This implements auto-reveal on focus and auto-unreveal on leave.
    fn sync_reveal_state(&mut self) {
        // Collect indices of all currently focused obscured value slots.
        // There should be at most one, but we handle the general case.
        let mut to_reveal = HashSet::new();
        for (i, f) in self.fields.iter().enumerate() {
            let value_focused = if f.field_type == FormFieldType::Notes {
                f.textarea_state.as_ref().is_some_and(|s| s.focus.get())
            } else {
                f.value_state.focus.get()
            };
            if value_focused && f.field_type.is_obscured() {
                to_reveal.insert(i);
            }
        }
        self.revealed_secret_fields = to_reveal;
    }

    /// After Tab/BackTab navigation, adjust scroll_offset so the focused field
    /// is visible. Name slot is always visible (not part of scroll range).
    /// Uses last_visible_rows (updated during render()) to determine how many
    /// field rows fit in the current overlay area.
    fn auto_scroll_after_tab(&mut self) {
        let visible = self.last_visible_rows.max(1);
        match self.focused_slot_kind() {
            SlotKind::Name => {
                // Name is always visible, scroll to top
                self.scroll_offset = 0;
            }
            SlotKind::Key(i) | SlotKind::Value(i) | SlotKind::Type(i) => {
                if i < self.scroll_offset {
                    // Focused field is above visible range
                    self.scroll_offset = i;
                } else if i >= self.scroll_offset + visible {
                    // Focused field is below visible range
                    self.scroll_offset = i.saturating_sub(visible - 1);
                }
            }
        }
    }

    /// After Tab/BackTab navigation, set the cursor to the end of the text
    /// in the newly focused slot and clear the overwrite flag.
    /// This prevents the TextInputState from replacing existing text when the
    /// user types a character (TextFocusGained::Overwrite default behavior).
    fn sync_cursor_after_tab(&mut self) {
        match self.focused_slot_kind() {
            SlotKind::Value(i) => {
                if let Some(f) = self.fields.get_mut(i) {
                    if f.field_type == FormFieldType::Notes {
                        if let Some(state) = f.textarea_state.as_mut() {
                            let len = state.text().len();
                            state.set_cursor((0, len as u32), false);
                        }
                    } else {
                        let len = f.value_state.len();
                        f.value_state.set_cursor(len, false);
                        f.value_state.set_overwrite(false);
                    }
                }
            }
            SlotKind::Key(i) => {
                if let Some(f) = self.fields.get_mut(i) {
                    let len = f.key_state.len();
                    f.key_state.set_cursor(len, false);
                    f.key_state.set_overwrite(false);
                }
            }
            SlotKind::Name => {
                let len = self.name_state.len();
                self.name_state.set_cursor(len, false);
                self.name_state.set_overwrite(false);
            }
            SlotKind::Type(_) => {}
        }
    }

    // ── rendering ─────────────────────────────────────────────────────────────

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let dirty_mark = if self.is_dirty() { " *" } else { "" };
        let title = match self.mode {
            FormMode::Add => format!(" New Entry{dirty_mark} "),
            FormMode::Edit => format!(" Edit Entry{dirty_mark} "),
        };

        // Clear the area first to prevent background content from showing through.
        frame.render_widget(Clear, area);

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title(title);
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        // Calculate visible field rows based on available height.
        let visible = self.visible_rows(inner.height);
        self.last_visible_rows = visible;

        // Clamp scroll_offset to valid range.
        if self.fields.len() > visible {
            self.scroll_offset = self.scroll_offset.min(self.fields.len() - visible);
        } else {
            self.scroll_offset = 0;
        }

        // Determine which fields to render.
        let end = (self.scroll_offset + visible).min(self.fields.len());
        let visible_fields = &mut self.fields[self.scroll_offset..end];

        // Rows: vault name + name + one row per visible field + hint bar.
        // Vault name is 1 line, name is 3 lines, fields are 3 lines each, hint is 1 line.
        let row_count = 1 + 1 + visible_fields.len() + 1;
        let constraints: Vec<Constraint> = (0..row_count)
            .map(|i| {
                if i == row_count - 1 {
                    Constraint::Length(1)
                } else if i == 0 {
                    // Vault name row — 1 line
                    Constraint::Length(1)
                } else {
                    Constraint::Length(3)
                }
            })
            .collect();

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        // Row 0: vault name (read-only display)
        let vault_text = if self.vault_name.is_empty() {
            " Vault: (none) ".to_string()
        } else {
            format!(" Vault: {} ", self.vault_name)
        };
        let vault_hint = if self.vault_name.is_empty() {
            String::new()
        } else {
            " [v] change vault ".to_string()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    &vault_text,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(vault_hint, Style::default().fg(Color::DarkGray)),
            ])),
            rows[0],
        );

        // Row 1: name
        let name_focused = self.name_state.focus.get();
        let name_has_error = self.validation_errors.contains(&ValidationIssue::EmptyName);
        let name_border_color = if name_has_error {
            Color::Red
        } else {
            Color::default()
        };
        let name_title = if name_has_error {
            " Name ⚠ "
        } else {
            " Name "
        };
        let name_widget = TextInput::new()
            .style(Style::default())
            .focus_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(if name_focused {
                        BorderType::Double
                    } else {
                        BorderType::Plain
                    })
                    .border_style(Style::default().fg(name_border_color))
                    .title(name_title),
            );
        frame.render_stateful_widget(name_widget, rows[1], &mut self.name_state);
        if self.name_state.focus.get() {
            if let Some((cx, cy)) = self.name_state.screen_cursor() {
                frame.set_cursor_position((
                    self.name_state.inner.x + cx,
                    self.name_state.inner.y + cy,
                ));
            }
        }

        // Rows 2..: fields — [key 35%] [value 50%] [type 15%]
        for (rel_i, ff) in visible_fields.iter_mut().enumerate() {
            if let Some(row) = rows.get(rel_i + 2) {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(35),
                        Constraint::Percentage(50),
                        Constraint::Percentage(15),
                    ])
                    .split(*row);

                // Key
                let key_f = ff.key_state.focus.get();
                let abs_i = self.scroll_offset + rel_i;
                let key_has_error = self
                    .validation_errors
                    .contains(&ValidationIssue::EmptyKey(abs_i))
                    || self
                        .validation_errors
                        .contains(&ValidationIssue::DuplicateKey(abs_i));
                let key_border_color = if key_has_error {
                    Color::Red
                } else {
                    Color::default()
                };
                let key_title = if key_has_error { " Key ⚠ " } else { " Key " };
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
                            .border_type(if key_f {
                                BorderType::Double
                            } else {
                                BorderType::Plain
                            })
                            .border_style(Style::default().fg(key_border_color))
                            .title(key_title),
                    );
                frame.render_stateful_widget(key_widget, cols[0], &mut ff.key_state);
                if key_f {
                    if let Some((cx, cy)) = ff.key_state.screen_cursor() {
                        frame.set_cursor_position((
                            ff.key_state.inner.x + cx,
                            ff.key_state.inner.y + cy,
                        ));
                    }
                }

                // Value — badge in title shows the field type label
                let val_f = if ff.field_type == FormFieldType::Notes {
                    ff.textarea_state.as_ref().is_some_and(|s| s.focus.get())
                } else {
                    ff.value_state.focus.get()
                };
                let badge = ff.field_type.label();
                let is_revealed = self
                    .revealed_secret_fields
                    .contains(&(self.scroll_offset + rel_i));
                let value_title = if is_revealed {
                    format!(" Value {badge} [revealed] ")
                } else {
                    format!(" Value {badge} ")
                };
                if ff.field_type == FormFieldType::Notes {
                    // Render a TextArea widget for Notes fields
                    if let Some(state) = ff.textarea_state.as_mut() {
                        let textarea = TextArea::new()
                            .style(if val_f {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            })
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_type(if val_f {
                                        BorderType::Double
                                    } else {
                                        BorderType::Plain
                                    })
                                    .title(value_title),
                            );
                        frame.render_stateful_widget(textarea, cols[1], state);
                        if val_f {
                            if let Some((cx, cy)) = state.screen_cursor() {
                                frame.set_cursor_position((state.inner.x + cx, state.inner.y + cy));
                            }
                        }
                    } else {
                        // Fallback: render as plain text if TextAreaState is missing
                        let val_widget = TextInput::new()
                            .style(Style::default())
                            .focus_style(
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_type(if val_f {
                                        BorderType::Double
                                    } else {
                                        BorderType::Plain
                                    })
                                    .title(value_title),
                            );
                        frame.render_stateful_widget(val_widget, cols[1], &mut ff.value_state);
                        if val_f {
                            if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                                frame.set_cursor_position((
                                    ff.value_state.inner.x + cx,
                                    ff.value_state.inner.y + cy,
                                ));
                            }
                        }
                    }
                } else if ff.field_type.is_obscured() {
                    let is_revealed = self
                        .revealed_secret_fields
                        .contains(&(self.scroll_offset + rel_i));
                    let mut val_widget = TextInput::new()
                        .style(Style::default())
                        .focus_style(
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(if val_f {
                                    BorderType::Double
                                } else {
                                    BorderType::Plain
                                })
                                .title(value_title),
                        );
                    if !is_revealed {
                        val_widget = val_widget.passwd();
                    }
                    frame.render_stateful_widget(val_widget, cols[1], &mut ff.value_state);
                    if val_f {
                        if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                            frame.set_cursor_position((
                                ff.value_state.inner.x + cx,
                                ff.value_state.inner.y + cy,
                            ));
                        }
                    }
                } else {
                    let val_widget = TextInput::new()
                        .style(Style::default())
                        .focus_style(
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_type(if val_f {
                                    BorderType::Double
                                } else {
                                    BorderType::Plain
                                })
                                .title(value_title),
                        );
                    frame.render_stateful_widget(val_widget, cols[1], &mut ff.value_state);
                    if val_f {
                        if let Some((cx, cy)) = ff.value_state.screen_cursor() {
                            frame.set_cursor_position((
                                ff.value_state.inner.x + cx,
                                ff.value_state.inner.y + cy,
                            ));
                        }
                    }
                }

                // Type cell — uses rat-widget Choice for the selector.
                let type_f = ff.type_state.focus.get();
                let (type_widget, _) = Choice::new()
                    .item(0, "text")
                    .item(1, "username")
                    .item(2, "token")
                    .item(3, "totp")
                    .item(4, "ssh-key")
                    .item(5, "notes")
                    .style(Style::default())
                    .focus_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(if type_f {
                                BorderType::Double
                            } else {
                                BorderType::Plain
                            })
                            .border_style(if type_f {
                                Style::default().fg(Color::Yellow)
                            } else {
                                Style::default()
                            })
                            .title(" Type "),
                    )
                    .popup_block(Block::default().borders(Borders::ALL))
                    .popup_placement(Placement::AboveOrBelow)
                    .popup_boundary(inner)
                    .into_widgets();
                frame.render_stateful_widget(type_widget, cols[2], &mut ff.type_state);
            }
        }

        // Render popups for any fields with an active popup.
        // Popups must be rendered AFTER all other widgets so they overlay.
        for ff in self.fields.iter_mut() {
            if ff.type_state.is_popup_active() {
                let (_, popup) = Choice::new()
                    .item(0, "text")
                    .item(1, "username")
                    .item(2, "token")
                    .item(3, "totp")
                    .item(4, "ssh-key")
                    .item(5, "notes")
                    .popup_block(Block::default().borders(Borders::ALL))
                    .popup_placement(Placement::AboveOrBelow)
                    .popup_boundary(inner)
                    .into_widgets();
                frame.render_stateful_widget(popup, ff.type_state.area, &mut ff.type_state);
            }
        }

        // Hint bar — field management keys + scroll indicators
        if let Some(hint_row) = rows.last() {
            let mut hint_text = " [+] add field  [-] remove field ".to_string();
            if self.scroll_offset > 0 {
                hint_text.push_str(" ↑ ");
            }
            if end < self.fields.len() {
                hint_text.push_str(" ↓ ");
            }
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    hint_text,
                    Style::default().fg(Color::DarkGray),
                )])),
                *hint_row,
            );
        }
    }
}

// ── HandleEvent implementation ─────────────────────────────────────────────

impl HandleEvent<Event, Regular, Outcome> for EntryForm {
    fn handle(&mut self, event: &Event, _data: Regular) -> Outcome {
        self.handle_event_inner(event)
    }
}
