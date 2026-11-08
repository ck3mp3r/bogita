//! Entry add/edit form — collects and validates entry data before persisting.
//!
//! ## Focus system
//!
//! Focus is managed by **rat-focus** `FocusFlags` — one flag per slot:
//! name, key, value, and type for each user-defined field.
//!
//! Tab / BackTab navigation uses `Focus::next_force` / `Focus::prev_force`
//! to cycle through all slots without delegating the key event to the focused
//! widget (which would consume Tab as a character insertion).
//!
//! ## Flat-index backward compatibility
//!
//! `focused_field()` returns a flat index for compatibility with `app.rs`
//! status-bar calculations:
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

// region:    --- Modules

mod confirm;
mod events;
mod field;
mod form;
mod render;
mod state;

pub use field::{FormAction, FormFieldType, FormMode, SlotKind, ValidationIssue};
pub use form::EntryForm;

// endregion: --- Modules

// region:    --- Tests
#[cfg(test)]
#[path = "../../../test/views/entry_form/editing.rs"]
mod entry_form_editing_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/mapping_and_labels.rs"]
mod entry_form_mapping_and_labels_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/notes_scroll_validation.rs"]
mod entry_form_notes_scroll_validation_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/reveal_and_types.rs"]
mod entry_form_reveal_and_types_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/support.rs"]
mod support;
// endregion: --- Tests
