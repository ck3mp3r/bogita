//! Deferred actions — queued by key handling, drained after each keypress.

use super::tui::{ActiveView, MessageKind, Tui};
use crate::views::entry_form::FormMode;
use bogita_core::agent::VaultService;
use bogita_core::domain::{Entry, SecretString};
use bogita_core::error::Result;
use bogita_core::service::clipboard::{system, CLIPBOARD_TIMEOUT_SECS};
use uuid::Uuid;

/// A deferred async action set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`] in the async event loop.
pub(super) enum AppAction {
    /// Derive the identity from the lock screen's passphrase.
    Unlock {
        passphrase: SecretString,
    },
    SaveEntry {
        entry: Entry,
        mode: FormMode,
        select_id: Uuid,
    },
    DeleteEntry {
        entry_id: Uuid,
    },
    /// Fetch and decrypt one entry for the detail view.
    LoadDetail {
        entry_id: Uuid,
    },
    /// Save a new vault.
    SaveVault {
        vault: bogita_core::domain::Vault,
    },
    /// Delete a vault.
    DeleteVault {
        vault_id: Uuid,
    },
}

/// A deferred clipboard copy set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`]. Kept separate from `AppAction` so the clipboard
/// path never blocks a vault mutation and vice-versa.
pub struct PendingCopy {
    pub entry_id: Uuid,
    pub vault_id: Uuid,
    pub field_idx: usize,
}

impl<V: VaultService> Tui<V> {
    /// Drain `self.pending`: persist mutations and reload entries into `main_view`.
    ///
    /// Called by the event loop after every keypress. No-op when pending is `None`.
    /// On backend failure the error is stored in `self.error_message` and displayed
    /// as an overlay modal rather than propagating up and crashing the TUI.
    pub async fn flush_pending(&mut self) -> Result<()> {
        if let Some(action) = self.pending.take() {
            // The unlock is the one action that runs while the vault is still
            // locked, so it comes before the locked guard below.
            if let AppAction::Unlock { passphrase } = action {
                self.complete_unlock(&passphrase).await;
            } else if self.service.is_locked() {
                self.pending_copy = None;
                return Ok(());
            } else {
                match self.do_flush(action).await {
                    Ok(Some(message)) => self.set_status(message, MessageKind::Success),
                    Ok(None) => {}
                    Err(e) => self.set_status(e.to_string(), MessageKind::Error),
                }
            }
        }

        // Drain clipboard copy.
        if let Some(pc) = self.pending_copy.take() {
            match self.do_copy(pc).await {
                Ok(()) => self.set_status(
                    format!("Copied to clipboard ({CLIPBOARD_TIMEOUT_SECS}s timeout)"),
                    MessageKind::Success,
                ),
                Err(e) => self.set_status(e.to_string(), MessageKind::Error),
            }
        }

        Ok(())
    }

    /// Run the lock screen's unlock: on success enter the main view with the
    /// vault loaded, on failure keep the lock screen, show the error, clear the
    /// input.
    async fn complete_unlock(&mut self, passphrase: &SecretString) {
        match self.service.unlock(passphrase).await {
            Ok(_) => {
                self.active = ActiveView::Main;
                match self.reload_entries().await {
                    Ok(()) => self.set_status("Unlocked", MessageKind::Success),
                    Err(e) => self.set_status(e.to_string(), MessageKind::Error),
                }
            }
            Err(e) => {
                if let ActiveView::Locked {
                    passphrase_input,
                    error,
                } = &mut self.active
                {
                    *error = Some(e.to_string());
                    passphrase_input.set_text("");
                } else {
                    self.error_message = Some(e.to_string());
                }
            }
        }
    }

    /// Reload vaults and entries through the service, dropping the detail entry.
    async fn reload_entries(&mut self) -> Result<()> {
        let vaults = self.service.list_vaults().await?;
        let all_entries = self.service.all_entries().await?;
        self.main_view.reload_vaults(vaults);
        self.main_view.reload_entries(all_entries);
        self.detail_entry = None;
        Ok(())
    }

    /// Inner flush implementation — returns `Ok(Some(msg))` on success with a
    /// status message, `Ok(None)` on success without a message, or `Err` on failure.
    async fn do_flush(&mut self, action: AppAction) -> Result<Option<String>> {
        match action {
            AppAction::Unlock { .. } => Ok(None),
            AppAction::SaveEntry {
                entry,
                mode,
                select_id,
            } => {
                self.service.save_entry(&entry).await?;

                // Reload and re-select the saved entry
                let all_entries = self.service.all_entries().await?;
                self.main_view
                    .reload_entries_select(all_entries, Some(select_id));

                let msg = match mode {
                    FormMode::Add => "Entry created",
                    FormMode::Edit => "Entry saved",
                };
                Ok(Some(msg.to_string()))
            }
            AppAction::DeleteEntry { entry_id } => {
                self.service.delete_entry(entry_id).await?;

                // Reload after delete — reset to index 0
                let all_entries = self.service.all_entries().await?;
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

                Ok(Some("Entry deleted".to_string()))
            }
            AppAction::LoadDetail { entry_id } => {
                self.detail_entry = self.service.get_entry(entry_id).await?;

                Ok(None) // No status message for loading detail
            }
            AppAction::SaveVault { vault } => {
                self.service.add_vault(&vault).await?;
                if vault.is_default {
                    self.service.set_default(vault.id).await?;
                }
                self.reload_entries().await?;

                Ok(Some("Vault created".to_string()))
            }
            AppAction::DeleteVault { vault_id } => {
                self.service.remove_vault(vault_id).await?;
                self.reload_entries().await?;

                Ok(Some("Vault deleted".to_string()))
            }
        }
    }

    async fn do_copy(&mut self, pc: PendingCopy) -> Result<()> {
        use secrecy::SecretString;

        let entry = match self.detail_entry.as_ref().filter(|e| e.id == pc.entry_id) {
            Some(e) => e,
            None => return Ok(()),
        };
        let Some(text) = self.main_view.copy_value_for(entry, pc.field_idx) else {
            return Ok(());
        };

        let svc = system();
        svc.copy_with_timeout(SecretString::from(text), CLIPBOARD_TIMEOUT_SECS)
            .await
    }
}
