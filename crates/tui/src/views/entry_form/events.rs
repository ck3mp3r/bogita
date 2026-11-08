//! Key routing and structural field edits for the entry form.

use rat_event::{HandleEvent, Outcome, Regular};
use rat_widget::event::ChoiceOutcome;
use rat_widget::textarea::TextAreaState;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};

use super::field::{FormAction, FormField, FormFieldType, SlotKind};
use super::EntryForm;

impl EntryForm {
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
        let outcome = match self.focused_slot() {
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
                let count = FormFieldType::count();
                match ke.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let Some(f) = self.fields.get_mut(idx) {
                            let cur = f.type_state.value();
                            let next = (cur + 1) % count;
                            f.type_state.set_value(next);
                            if let Some(new_type) = FormFieldType::from_index(next) {
                                f.field_type = new_type;
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
                            }
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
        let slot = self.focused_slot();

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

    /// Close every open type popup. Used before any structural field change.
    pub(super) fn clear_type_popups(&mut self) {
        for f in &mut self.fields {
            f.type_state.set_popup_active(false);
        }
    }

    fn add_field(&mut self) {
        // Close any open type popups
        self.clear_type_popups();
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
        self.clear_type_popups();
        let field_idx = match self.focused_slot() {
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
}

// ── HandleEvent implementation ─────────────────────────────────────────────

impl HandleEvent<Event, Regular, Outcome> for EntryForm {
    fn handle(&mut self, event: &Event, _data: Regular) -> Outcome {
        self.handle_event_inner(event)
    }
}
