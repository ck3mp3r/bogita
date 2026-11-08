//! `EntryForm` state, constructors, and public accessors.

use bogita_core::domain::{Entry, FieldValue};
use rat_focus::Focus;
use rat_widget::choice::ChoiceState;
use rat_widget::text_input::TextInputState;
use rat_widget::textarea::TextAreaState;
use std::collections::HashSet;
use uuid::Uuid;

use super::confirm::OriginalSnapshot;
use super::field::{FormAction, FormField, FormFieldType, FormMode, ValidationIssue};

// ── EntryForm ─────────────────────────────────────────────────────────────────

pub struct EntryForm {
    pub(super) mode: FormMode,
    pub(super) entry_id: Uuid,
    pub(super) vault_id: Uuid,
    pub(super) vault_name: String,
    pub(super) name_state: TextInputState,
    pub(super) fields: Vec<FormField>,
    pub(super) focus: Focus,
    /// Pending high-level action set during HandleEvent handling.
    /// Consumed by `handle_key` to return the appropriate `FormAction`.
    pub(super) pending_action: FormAction,
    /// Scroll offset for field list — how many fields are scrolled past the top.
    /// The name field is always visible (pinned at top, not part of scroll range).
    pub(crate) scroll_offset: usize,
    /// Last known visible rows count, updated during render().
    /// Used by auto_scroll_after_tab() and add_field() to determine
    /// how many field rows fit in the current overlay area.
    pub(crate) last_visible_rows: usize,
    /// Indices of fields whose secret value is currently shown in plaintext.
    /// Auto-revealed on focus, auto-unrevealed on focus leave.
    /// Can be toggled manually with Ctrl-r.
    pub(crate) revealed_secret_fields: HashSet<usize>,
    /// Snapshot of the original entry for dirty-state tracking.
    /// `None` for Add mode; `Some((name, fields))` for Edit mode.
    original: Option<OriginalSnapshot>,
    /// Validation errors detected after the most recent key event.
    /// Updated by `validate()` at the end of every `handle_event_inner()` call.
    pub(crate) validation_errors: HashSet<ValidationIssue>,
}

impl EntryForm {
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
}
