//! Confirmation: build the domain `Entry` from the form state.

use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue, AGENT_SERVABLE_KEY};
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
                    ValidationIssue::InvalidAgentServable(i) => {
                        format!("Field {} agent servable must be true or false", i + 1)
                    }
                    ValidationIssue::InvalidSshKey(i) => {
                        format!(
                            "Field {}: private key is not a valid OpenSSH private key",
                            i + 1
                        )
                    }
                    ValidationIssue::InvalidTotp(i) => {
                        format!("Field {}: TOTP secret must be base32", i + 1)
                    }
                })
                .collect();
            return FormAction::ValidationError(messages.join("; "));
        }
        let now = Utc::now().timestamp();
        let mut domain_fields: Vec<Field> = Vec::with_capacity(self.fields.len());
        for (i, ff) in self.fields.iter().enumerate() {
            // Sync TextAreaState text back to value for Notes fields
            let value = if let Some(state) = ff.textarea_state.as_ref() {
                state.text()
            } else {
                ff.value_state.text().to_string()
            };
            // The agent-servable gate is authored as the CLI authors it: the
            // canonical key, the FieldType::Boolean type (storage "boolean"), and
            // a plaintext Boolean value.
            if ff.is_agent_servable() {
                let Some(on) = FormFieldType::parse_agent_servable(&value) else {
                    return FormAction::ValidationError(format!(
                        "Field {} agent servable must be true or false",
                        i + 1
                    ));
                };
                domain_fields.push(Field {
                    id: Uuid::new_v4(),
                    key: AGENT_SERVABLE_KEY.to_string(),
                    value: FieldValue::Boolean(on),
                    field_type: FieldType::Boolean,
                    encrypted: false,
                    idx: i as i32,
                });
                continue;
            }
            // An SshKey row is the form door for free-text private keys: the
            // value must parse as an OpenSSH PEM before the Entry is built.
            if ff.field_type == FormFieldType::SshKey {
                if let Some(e) = Self::ssh_key_parse_error(value.as_str()) {
                    return FormAction::ValidationError(format!(
                        "Field {}: private key is not a valid OpenSSH private key ({e})",
                        i + 1
                    ));
                }
            }
            let key = ff.key_state.text().to_string();
            let field_type = ff.field_type.to_domain_field_type(&key);
            domain_fields.push(Field {
                id: Uuid::new_v4(),
                key,
                value: ff.field_type.to_domain_value(&value),
                field_type,
                encrypted: ff.field_type.is_encrypted(),
                idx: i as i32,
            });
        }

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
