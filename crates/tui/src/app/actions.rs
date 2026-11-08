//! Deferred actions — queued by key handling, drained after each keypress.

use super::tui::{MessageKind, Tui};
use crate::views::entry_form::FormMode;
use bogita_core::domain::Entry;
use bogita_core::error::Result;
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use bogita_core::service::clipboard::{system, CLIPBOARD_TIMEOUT_SECS};
use uuid::Uuid;

/// A deferred async action set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`] in the async event loop.
pub(super) enum AppAction {
    SaveEntry {
        entry: Entry,
        mode: FormMode,
        select_id: Uuid,
    },
    DeleteEntry {
        entry_id: Uuid,
        vault_id: Uuid,
    },
    /// Fetch and decrypt one entry for the detail view.
    LoadDetail {
        entry_id: Uuid,
        vault_id: Uuid,
    },
    /// Save a new vault.
    SaveVault {
        vault: bogita_core::domain::Vault,
    },
    /// Delete a vault.
    DeleteVault {
        vault_id: Uuid,
    },
    /// Reload entries after unlock.
    ReloadEntries,
}

/// A deferred clipboard copy set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`]. Kept separate from `AppAction` so the clipboard
/// path never blocks a vault mutation and vice-versa.
pub struct PendingCopy {
    pub entry_id: Uuid,
    pub vault_id: Uuid,
    pub field_idx: usize,
}

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> Tui<K, F, I> {
    /// Drain `self.pending`: persist mutations and reload entries into `main_view`.
    ///
    /// Called by the event loop after every keypress. No-op when pending is `None`.
    /// On backend failure the error is stored in `self.error_message` and displayed
    /// as an overlay modal rather than propagating up and crashing the TUI.
    pub async fn flush_pending(&mut self) -> Result<()> {
        // If locked, skip all pending operations
        if self.app.is_locked {
            self.pending = None;
            self.pending_copy = None;
            return Ok(());
        }

        // Drain vault mutation (save / delete).
        if let Some(action) = self.pending.take() {
            match self.do_flush(action).await {
                Ok(Some(message)) => self.set_status(message, MessageKind::Success),
                Ok(None) => {}
                Err(e) => self.set_status(e.to_string(), MessageKind::Error),
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

    /// Inner flush implementation — returns `Ok(Some(msg))` on success with a
    /// status message, `Ok(None)` on success without a message, or `Err` on failure.
    async fn do_flush(&mut self, action: AppAction) -> Result<Option<String>> {
        let vaults = self.app.registry.list_vaults().await?;
        if self.app.identity.is_none() {
            return Ok(None); // locked — no-op
        }

        match action {
            AppAction::SaveEntry {
                entry,
                mode,
                select_id,
            } => {
                // Find the vault that owns this entry
                let vault = vaults.iter().find(|v| v.id == entry.vault_id);
                if let Some(svc) = vault.and_then(|v| self.app.service_for(v)) {
                    svc.save_entry(&entry).await?;
                }

                // Reload and re-select the saved entry
                let all_entries = self.app.all_entries().await?;
                self.main_view
                    .reload_entries_select(all_entries, Some(select_id));

                let msg = match mode {
                    FormMode::Add => "Entry created",
                    FormMode::Edit => "Entry saved",
                };
                Ok(Some(msg.to_string()))
            }
            AppAction::DeleteEntry { entry_id, vault_id } => {
                let vault = vaults.iter().find(|v| v.id == vault_id);
                if let Some(svc) = vault.and_then(|v| self.app.service_for(v)) {
                    svc.delete_entry(entry_id).await?;
                }

                // Reload after delete — reset to index 0
                let all_entries = self.app.all_entries().await?;
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

                Ok(Some("Entry deleted".to_string()))
            }
            AppAction::LoadDetail { entry_id, vault_id } => {
                let vault = vaults.iter().find(|v| v.id == vault_id);
                if let Some(svc) = vault.and_then(|v| self.app.service_for(v)) {
                    self.detail_entry = svc.get_entry(entry_id).await?;
                }

                Ok(None) // No status message for loading detail
            }
            AppAction::SaveVault { vault } => {
                self.app.registry.add_vault(&vault).await?;
                if vault.is_default {
                    self.app.registry.set_default(vault.id).await?;
                }
                // Reload vaults and entries
                let vaults = self.app.registry.list_vaults().await?;
                let all_entries = self.app.all_entries().await?;
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);

                Ok(Some("Vault created".to_string()))
            }
            AppAction::DeleteVault { vault_id } => {
                self.app.registry.remove_vault(vault_id).await?;
                // Reload vaults and entries
                let vaults = self.app.registry.list_vaults().await?;
                let all_entries = self.app.all_entries().await?;
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

                Ok(Some("Vault deleted".to_string()))
            }
            AppAction::ReloadEntries => {
                // Reload entries after unlock
                let vaults = self.app.registry.list_vaults().await?;
                if self.app.identity.is_none() {
                    return Ok(None);
                }
                let all_entries = self.app.all_entries().await?;
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;
                Ok(Some("Unlocked".to_string()))
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
