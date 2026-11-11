//! Derived `EntryForm` state: validation, focus rebuilding, and scroll upkeep.

use rat_focus::{Focus, FocusBuilder};
use std::collections::HashSet;

use super::field::{FormFieldType, SlotKind, ValidationIssue};
use super::widgets::{
    flags, BooleanWidget, NotesWidget, PasswordRule, PasswordWidget, PlaintextWidget, SshKeyWidget,
    TotpSecretWidget,
};
use super::EntryForm;

impl EntryForm {
    /// Re-run validation and update `validation_errors`.
    /// Called at the end of every `handle_event_inner()` call.
    pub(super) fn validate(&mut self) {
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

        // The agent-servable gate accepts only true or false.
        for (i, f) in self.fields.iter().enumerate() {
            if f.is_agent_servable()
                && FormFieldType::parse_agent_servable(f.value_state.text()).is_none()
            {
                self.validation_errors
                    .insert(ValidationIssue::InvalidAgentServable(i));
            }
        }

        // Value-format rules: each row's field type names the widget that owns
        // its rule and timing. The arms are exhaustive on purpose — a new
        // `FormFieldType` variant does not compile until it names a widget —
        // and the loop only supplies the focus context.
        let focused_slot = self.focused_slot();
        let mut value_issues: Vec<ValidationIssue> = Vec::new();
        for (i, f) in self.fields.iter().enumerate() {
            let value = self.field_value_text(f);
            let slot_focused = matches!(focused_slot, SlotKind::Value(j) if j == i);
            let (flags_here, issue) = match f.field_type {
                FormFieldType::SshKey => (
                    flags(&SshKeyWidget, &value, slot_focused),
                    Some(ValidationIssue::InvalidSshKey(i)),
                ),
                FormFieldType::Totp => (
                    flags(&TotpSecretWidget, &value, slot_focused),
                    Some(ValidationIssue::InvalidTotp(i)),
                ),
                FormFieldType::Token => (
                    flags(
                        &PasswordWidget {
                            rule: PasswordRule::Advisory,
                        },
                        &value,
                        slot_focused,
                    ),
                    None,
                ),
                FormFieldType::Username | FormFieldType::Text | FormFieldType::SshPublicKey => {
                    (flags(&PlaintextWidget, &value, slot_focused), None)
                }
                FormFieldType::Notes => (flags(&NotesWidget, &value, slot_focused), None),
                FormFieldType::Boolean => (flags(&BooleanWidget, &value, slot_focused), None),
            };
            if flags_here {
                value_issues.extend(issue);
            }
        }
        self.validation_errors.extend(value_issues);
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
        if self
            .validation_errors
            .iter()
            .any(|e| matches!(e, ValidationIssue::InvalidAgentServable(_)))
        {
            return "agent servable must be true or false".to_string();
        }
        if let Some(i) = self.validation_errors.iter().find_map(|e| match e {
            ValidationIssue::InvalidSshKey(i) => Some(*i),
            _ => None,
        }) {
            return format!(
                "Field {}: private key is not a valid OpenSSH private key",
                i + 1
            );
        }
        if let Some(i) = self.validation_errors.iter().find_map(|e| match e {
            ValidationIssue::InvalidTotp(i) => Some(*i),
            _ => None,
        }) {
            return format!("Field {}: TOTP secret must be base32", i + 1);
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
    pub(super) fn rebuild_focus(&mut self) -> &mut Focus {
        let old = std::mem::take(&mut self.focus);
        let mut fb = FocusBuilder::new(Some(old));
        fb.widget(&self.name_state);
        for ff in &self.fields {
            fb.widget(&ff.key_state);
            fb.widget(&ff.type_state);
            if ff.field_type == FormFieldType::Notes {
                if let Some(state) = ff.textarea_state.as_ref() {
                    fb.widget(state);
                }
            } else {
                fb.widget(&ff.value_state);
            }
        }
        self.focus = fb.build();
        &mut self.focus
    }

    /// The slot the focus currently sits on.
    pub fn focused_slot(&self) -> SlotKind {
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

    // ── scrolling ────────────────────────────────────────────────────────────

    /// Calculate how many field rows fit in the given area.
    /// Each field row is 3 lines. Name takes 3 lines, hint bar takes 1 line.
    pub(super) fn visible_rows(&self, inner_height: u16) -> usize {
        let available = inner_height.saturating_sub(1 + 3 + 1); // vault(1) + name(3) + hint(1)
        (available / 3) as usize
    }

    /// After Tab/BackTab navigation, sync the reveal state so that only the
    /// currently focused obscured value slot is revealed. All other fields are
    /// hidden. This implements auto-reveal on focus and auto-unreveal on leave.
    pub(super) fn sync_reveal_state(&mut self) {
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
    pub(super) fn auto_scroll_after_tab(&mut self) {
        let visible = self.last_visible_rows.max(1);
        match self.focused_slot() {
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
    pub(super) fn sync_cursor_after_tab(&mut self) {
        match self.focused_slot() {
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
}
