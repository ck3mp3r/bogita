//! Key handling — maps key presses onto view transitions and deferred actions.

use super::actions::{AppAction, PendingCopy};
use super::tui::{ActiveView, MessageKind, RunState, Tui};
use crate::views::entry_form::{EntryForm, FormAction, FormMode, SlotKind};
use crate::views::main_view::MainViewAction;
use crate::views::password_gen_view::{PasswordGenAction, PasswordGenView};
use crate::views::vault_form::{VaultForm, VaultFormAction};
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::widgets::ListState;
use std::time::Instant;

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> Tui<K, F, I> {
    /// Dismiss the error overlay on Esc/Enter. Returns `true` when an overlay is
    /// showing, in which case the key is consumed either way.
    fn dismiss_error_overlay(&mut self, key: KeyCode) -> bool {
        if self.error_message.is_none() {
            return false;
        }
        if matches!(key, KeyCode::Esc | KeyCode::Enter) {
            self.error_message = None;
        }
        true
    }

    /// Set the transient status-bar message and its kind.
    pub(super) fn set_status(&mut self, message: impl Into<String>, kind: MessageKind) {
        self.status_message = Some((message.into(), Instant::now()));
        self.status_message_kind = kind;
    }

    /// Handle a key press with full modifier info.
    /// Used by the event loop to pass Ctrl-r to the form.
    pub(crate) fn handle_key_with_modifiers(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Ctrl-L: lock the vault (only from Main view, not locked)
        if key == KeyCode::Char('l') && modifiers == KeyModifiers::CONTROL {
            if !self.app.is_locked {
                self.lock();
            }
            return;
        }

        // Error overlay: [Esc] or [Enter] dismisses; any other key is swallowed.
        if self.dismiss_error_overlay(key) {
            return;
        }

        match &mut self.active {
            ActiveView::Form(f) => {
                // [g] on a token slot (obscured value) opens the password generator.
                if key == KeyCode::Char('g')
                    && modifiers.is_empty()
                    && matches!(f.focused_slot(), SlotKind::Value(_))
                    && f.focused_field_is_obscured()
                {
                    let placeholder = EntryForm::new_add(None);
                    let form = std::mem::replace(f, placeholder);
                    self.active = ActiveView::PasswordGen {
                        gen: PasswordGenView::new(),
                        form,
                    };
                    return;
                }
                match f.handle_key_with_modifiers(key, modifiers) {
                    FormAction::Cancel => {
                        if f.mode() == FormMode::Edit {
                            let placeholder = EntryForm::new_add(None);
                            let form = std::mem::replace(f, placeholder);
                            self.active = ActiveView::ConfirmSave { form };
                        } else {
                            self.active = ActiveView::Main;
                        }
                    }
                    FormAction::ConfirmDiscard => {
                        let placeholder = EntryForm::new_add(None);
                        let form = std::mem::replace(f, placeholder);
                        self.active = ActiveView::ConfirmDiscard { form };
                    }
                    FormAction::Confirm(_) => {
                        let mode = f.mode();
                        if mode == FormMode::Edit {
                            let placeholder = EntryForm::new_add(None);
                            let form = std::mem::replace(f, placeholder);
                            self.active = ActiveView::ConfirmSave { form };
                        } else {
                            if let FormAction::Confirm(entry) = f.confirm() {
                                let select_id = entry.id;
                                self.pending = Some(AppAction::SaveEntry {
                                    entry,
                                    mode,
                                    select_id,
                                });
                            }
                            self.active = ActiveView::Main;
                        }
                    }
                    FormAction::OpenVaultPicker => {
                        let placeholder = EntryForm::new_add(None);
                        let form = std::mem::replace(f, placeholder);
                        let mut vault_state = ListState::default();
                        vault_state.select(Some(0));
                        self.active = ActiveView::FormVaultPicker { form, vault_state };
                    }
                    FormAction::None | FormAction::ValidationError(_) => {}
                }
            }
            ActiveView::PasswordGen { .. } => {
                // Destructure by replacing active to get ownership.
                let ActiveView::PasswordGen { mut gen, form } =
                    std::mem::replace(&mut self.active, ActiveView::Main)
                else {
                    unreachable!()
                };
                match gen.handle_key(key) {
                    PasswordGenAction::Accept(pw) => {
                        let mut form = form;
                        form.set_focused_value(pw);
                        self.active = ActiveView::Form(form);
                    }
                    PasswordGenAction::Cancel => {
                        self.active = ActiveView::Form(form);
                    }
                    PasswordGenAction::None => {
                        self.active = ActiveView::PasswordGen { gen, form };
                    }
                }
            }
            ActiveView::FormVaultPicker { form, vault_state } => {
                let vaults = self.main_view.vaults_snapshot();
                let len = vaults.len() + 1; // +1 for "All Vaults"
                match key {
                    KeyCode::Char('j') | KeyCode::Down => {
                        let cur = vault_state.selected().unwrap_or(0);
                        vault_state.select(Some((cur + 1).min(len.saturating_sub(1))));
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        let cur = vault_state.selected().unwrap_or(0);
                        vault_state.select(Some(cur.saturating_sub(1)));
                    }
                    KeyCode::Enter => {
                        // Determine vault_id and vault_name from selection
                        let placeholder = EntryForm::new_add(None);
                        let mut form = std::mem::replace(form, placeholder);
                        let idx = vault_state.selected().unwrap_or(0);
                        if idx == 0 {
                            // "All Vaults" selected — use the first vault
                            if let Some(v) = vaults.first() {
                                form.set_vault(v.id, v.name.clone());
                            }
                        } else if let Some(v) = vaults.get(idx - 1) {
                            form.set_vault(v.id, v.name.clone());
                        }
                        self.active = ActiveView::Form(form);
                    }
                    KeyCode::Esc => {
                        let placeholder = EntryForm::new_add(None);
                        let form = std::mem::replace(form, placeholder);
                        self.active = ActiveView::Form(form);
                    }
                    KeyCode::Char('a') => {
                        self.active = ActiveView::VaultForm(VaultForm::new(String::new()));
                    }
                    _ => {}
                }
            }
            _ => {
                self.handle_key(key);
            }
        }
    }

    /// Handle a key press. Returns the new [`RunState`].
    ///
    /// Kept synchronous so tests can drive it without a real terminal.
    /// Async mutations are deferred to `self.pending`; call `flush_pending`
    /// afterwards to drain them.
    pub fn handle_key(&mut self, key: KeyCode) -> RunState {
        // Error overlay: [Esc] or [Enter] dismisses; any other key is swallowed.
        if self.dismiss_error_overlay(key) {
            return self.state.clone();
        }

        match &mut self.active {
            ActiveView::Main => match key {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.state = RunState::Quit;
                }
                other => match self.main_view.handle_key(other) {
                    MainViewAction::None => {}
                    MainViewAction::OpenAddForm { vault_id } => {
                        let vault_name = self
                            .main_view
                            .vaults_snapshot()
                            .iter()
                            .find(|v| v.id == vault_id)
                            .map(|v| v.name.clone())
                            .unwrap_or_default();
                        let mut form = EntryForm::new_add(None);
                        form.set_vault(vault_id, vault_name);
                        self.active = ActiveView::Form(form);
                    }
                    MainViewAction::OpenEditForm { entry_id } => {
                        let entry = self
                            .main_view
                            .visible_entries()
                            .into_iter()
                            .find(|e| e.id == entry_id)
                            .cloned();
                        if let Some(e) = entry {
                            let mut form = EntryForm::new_edit(&e);
                            let vault_name = self
                                .main_view
                                .vaults_snapshot()
                                .iter()
                                .find(|v| v.id == e.vault_id)
                                .map(|v| v.name.clone())
                                .unwrap_or_default();
                            form.set_vault(e.vault_id, vault_name);
                            self.active = ActiveView::Form(form);
                        }
                    }
                    MainViewAction::DeleteEntry { entry_id } => {
                        let entry = self
                            .main_view
                            .visible_entries()
                            .into_iter()
                            .find(|e| e.id == entry_id)
                            .cloned();
                        if let Some(e) = entry {
                            self.active = ActiveView::ConfirmDelete {
                                entry_id: e.id,
                                vault_id: e.vault_id,
                                name: e.name.clone(),
                            };
                        }
                    }
                    MainViewAction::OpenAddVault => {
                        self.active = ActiveView::VaultForm(VaultForm::new(String::new()));
                    }
                    MainViewAction::DeleteVault { vault_id } => {
                        // Find vault name for the confirm modal
                        let vaults = self.main_view.vaults_snapshot();
                        if let Some(vault) = vaults.iter().find(|v| v.id == vault_id) {
                            self.active = ActiveView::ConfirmDeleteVault {
                                vault_id,
                                name: vault.name.clone(),
                            };
                        }
                    }
                    MainViewAction::CopyField {
                        entry_id,
                        field_idx,
                    } => {
                        // Copy directly from the decrypted detail_entry.
                        if let Some(e) = self.detail_entry.as_ref().filter(|e| e.id == entry_id) {
                            self.pending_copy = Some(PendingCopy {
                                entry_id: e.id,
                                vault_id: e.vault_id,
                                field_idx,
                            });
                        }
                    }
                    MainViewAction::SelectEntry { entry_id, vault_id } => {
                        self.detail_entry = None;
                        self.pending = Some(AppAction::LoadDetail { entry_id, vault_id });
                    }
                },
            },

            ActiveView::ConfirmSave { .. } => {
                // Destructure by replacing active to get ownership.
                let ActiveView::ConfirmSave { mut form } =
                    std::mem::replace(&mut self.active, ActiveView::Main)
                else {
                    unreachable!()
                };
                match key {
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        // Confirm: re-run confirm() to get the entry and save.
                        if let FormAction::Confirm(entry) = form.confirm() {
                            let select_id = entry.id;
                            self.pending = Some(AppAction::SaveEntry {
                                entry,
                                mode: FormMode::Edit,
                                select_id,
                            });
                        }
                        // self.active already set to Main above
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        // Discard changes — go to Main without saving.
                        // self.active already set to Main above
                    }
                    KeyCode::Esc => {
                        // Back to form with edits intact.
                        self.active = ActiveView::Form(form);
                    }
                    _ => {
                        // Any other key: put it back.
                        self.active = ActiveView::ConfirmSave { form };
                    }
                }
            }
            ActiveView::ConfirmDiscard { .. } => {
                // Destructure by replacing active to get ownership.
                let ActiveView::ConfirmDiscard { form } =
                    std::mem::replace(&mut self.active, ActiveView::Main)
                else {
                    unreachable!()
                };
                match key {
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        // Discard changes — go to Main without saving.
                        // self.active already set to Main above
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc => {
                        // Back to form with edits intact.
                        self.active = ActiveView::Form(form);
                    }
                    _ => {
                        // Any other key: put it back.
                        self.active = ActiveView::ConfirmDiscard { form };
                    }
                }
            }
            ActiveView::ConfirmDelete {
                entry_id, vault_id, ..
            } => match key {
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.pending = Some(AppAction::DeleteEntry {
                        entry_id: *entry_id,
                        vault_id: *vault_id,
                    });
                    self.active = ActiveView::Main;
                }
                KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                    self.active = ActiveView::Main;
                }
                _ => {}
            },
            ActiveView::ConfirmDeleteVault { vault_id, .. } => match key {
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.pending = Some(AppAction::DeleteVault {
                        vault_id: *vault_id,
                    });
                    self.active = ActiveView::Main;
                }
                KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                    self.active = ActiveView::Main;
                }
                _ => {}
            },
            ActiveView::VaultForm(form) => match form.handle_key(key) {
                VaultFormAction::Confirm(mut vault) => {
                    match &self.app.identity {
                        Some(identity) => {
                            vault.recipients = vec![identity.to_recipient()];
                        }
                        None => {
                            self.error_message =
                                Some("Vault is locked. Cannot create vault.".to_string());
                            self.active = ActiveView::Main;
                            return self.state.clone();
                        }
                    }
                    self.pending = Some(AppAction::SaveVault { vault });
                    self.active = ActiveView::Main;
                }
                VaultFormAction::Cancel => {
                    self.active = ActiveView::Main;
                }
                VaultFormAction::None => {}
            },
            ActiveView::Locked {
                passphrase_input,
                error,
            } => {
                match key {
                    KeyCode::Esc => {
                        self.state = RunState::Quit;
                    }
                    KeyCode::Enter => {
                        let passphrase =
                            secrecy::SecretString::from(passphrase_input.text().to_string());
                        match self.app.unlock(&passphrase) {
                            Ok(()) => {
                                // Switch to Main and queue entry reload
                                self.active = ActiveView::Main;
                                self.pending = Some(AppAction::ReloadEntries);
                            }
                            Err(e) => {
                                *error = Some(e.to_string());
                                passphrase_input.set_text("");
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        passphrase_input.set_text(format!("{}{}", passphrase_input.text(), c));
                    }
                    KeyCode::Backspace => {
                        let text = passphrase_input.text();
                        if !text.is_empty() {
                            passphrase_input.set_text(text[..text.len() - 1].to_string());
                        }
                    }
                    _ => {}
                }
            }
            // These are handled in handle_key_with_modifiers, never reached here.
            ActiveView::Form(_)
            | ActiveView::PasswordGen { .. }
            | ActiveView::FormVaultPicker { .. } => {}
        }
        self.state.clone()
    }
}
