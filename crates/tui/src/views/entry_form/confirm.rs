//! Confirmation: build the domain `Entry` from the form state.

use bogita_core::domain::{Entry, EntryType, Field};
use chrono::Utc;
use uuid::Uuid;

use super::field::{FormAction, FormFieldType, ValidationIssue};
use super::EntryForm;

/// Snapshot of the original entry for dirty-state tracking.
/// `(name, Vec<(key, value, field_type)>)`.
pub(super) type OriginalSnapshot = (String, Vec<(String, String, FormFieldType)>);

impl EntryForm {
    pub fn confirm(&mut self) -> FormAction {
        // Clear reveal state before constructing the Entry.
        self.revealed_secret_fields.clear();
        // Close any open type popups, then sync field_type from the Choice widget.
        self.clear_type_popups();
        for f in &mut self.fields {
            let selected = f.type_state.value();
            if let Some(new_type) = FormFieldType::from_index(selected) {
                f.field_type = new_type;
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
}
