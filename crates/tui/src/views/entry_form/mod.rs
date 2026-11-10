//! Entry add/edit form — collects and validates entry data before persisting.
//!
//! ## Focus system
//!
//! Focus is managed by **rat-focus** `FocusFlags` — one flag per slot:
//! name, key, type, and value for each user-defined field.
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
//! `Enter` confirms (with validation) from a text slot and opens the type dropdown from a type slot.
//! `Esc` always cancels the form.
//! On a type slot, `j`/`k`/`Up`/`Down` navigate the selector and open the popup; the type is applied when the popup closes.
//! `Alt-n` / `Alt-d` add / remove fields from any field slot; `Alt-g`, `Alt-r`, `Alt-v` and `Alt-t`
//! are the remaining management chords. Unmodified printable keys and Space always belong to the
//! focused input.

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
#[path = "../../../test/views/entry_form/boolean_row.rs"]
mod entry_form_boolean_row_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/cursor.rs"]
mod entry_form_cursor_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/editing.rs"]
mod entry_form_editing_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/layout.rs"]
mod entry_form_layout_test;
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
#[path = "../../../test/views/entry_form/save_chord.rs"]
mod entry_form_save_chord_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/ssh_key_confirm.rs"]
mod entry_form_ssh_key_confirm_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/validation.rs"]
mod entry_form_validation_test;
#[cfg(test)]
#[path = "../../../test/views/entry_form/support.rs"]
mod support;
// endregion: --- Tests
