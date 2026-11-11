//! Key handling for the type picker: the dropdown that says what a row IS.
//!
//! Choosing an arm here is the one place a row's type becomes the user's word
//! rather than the load shape, so every path that applies a selection marks the
//! row `chosen` — `confirm()` reads that marker before it ever consults the key
//! table.

use rat_event::{HandleEvent, Outcome, Regular};
use rat_widget::event::ChoiceOutcome;
use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind};

use super::field::FormFieldType;
use super::EntryForm;

impl EntryForm {
    /// Handle events when focused on a type slot.
    ///
    /// 1. Popup open → delegate ALL events to ChoiceState, then sync field_type
    ///    after popup closes. Return early.
    /// 2. Popup closed → intercept form-level keys (Esc=cancel, Enter=open popup).
    ///    Return early on match.
    /// 3. Popup closed → handle j/k/Up/Down with wrapping (ChoiceState treats j/k
    ///    as character search, so we handle them ourselves). Return early on match.
    /// 4. Popup closed → swallow everything else: Space and printable chars must
    ///    not reach `ChoiceState::select_by_char`.
    pub(super) fn handle_type_event(&mut self, event: &Event, idx: usize) -> Outcome {
        // 1. If popup is open, handle navigation and delegation
        if idx < self.fields.len() && self.fields[idx].type_state.is_popup_active() {
            // Intercept j/k/Up/Down for navigation (ChoiceState treats j/k as char search)
            if let Event::Key(ke) = event {
                if ke.kind == KeyEventKind::Press {
                    let count = FormFieldType::count();
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
                    if let Some(new_type) = FormFieldType::from_index(selected) {
                        f.field_type = new_type;
                        f.chosen = true;
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
                        self.pending_action = self.esc_action();
                        self.revealed_secret_fields.clear();
                        return Outcome::Changed;
                    }
                    // The type selector is a picker, not a text cell: Enter opens it.
                    KeyCode::Enter => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            f.type_state.set_popup_active(true);
                        }
                        return Outcome::Changed;
                    }
                    _ => {}
                }
            }
        }

        // 3. Handle j/k/Up/Down with wrapping
        if let Event::Key(ke) = event {
            if ke.kind == KeyEventKind::Press {
                let count = FormFieldType::count();
                match ke.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            let cur = f.type_state.value();
                            let next = (cur + 1) % count;
                            f.type_state.set_value(next);
                            if let Some(new_type) = FormFieldType::from_index(next) {
                                f.field_type = new_type;
                                f.chosen = true;
                            }
                            self.ensure_textarea(idx);
                        }
                        return Outcome::Changed;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            let cur = f.type_state.value();
                            let prev = if cur == 0 { count - 1 } else { cur - 1 };
                            f.type_state.set_value(prev);
                            if let Some(new_type) = FormFieldType::from_index(prev) {
                                f.field_type = new_type;
                                f.chosen = true;
                            }
                            self.ensure_textarea(idx);
                        }
                        return Outcome::Changed;
                    }
                    _ => {}
                }
            }
        }

        // 4. Popup closed — Space and printable chars type nothing on this cell.
        Outcome::Changed
    }
}
