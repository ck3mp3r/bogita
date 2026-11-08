//! Tui application shell — struct, constructor, event loop, and lock.

use super::actions::{AppAction, PendingCopy};
use crate::context::TuiContext;
use crate::views::entry_form::EntryForm;
use crate::views::main_view::MainView;
use crate::views::password_gen_view::PasswordGenView;
use crate::views::vault_form::VaultForm;
use bogita_core::app::App;
use bogita_core::domain::{Entry, Vault};
use bogita_core::error::Result;
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use rat_widget::text_input::TextInputState;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Running state of the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    Running,
    Quit,
}

/// Which view is currently active.
pub(super) enum ActiveView {
    Main,
    /// Add/edit form rendered in Col 3.
    Form(EntryForm),
    /// Confirm-save modal for edit mode — shown after Enter or Esc on the edit form.
    /// The form is kept so [Esc] can return to it, and [s] can re-confirm to get the entry.
    ConfirmSave {
        form: EntryForm,
    },
    /// Discard confirmation modal — shown after Esc on a dirty form.
    /// The form is kept so [b/Esc] can return to it, and [d] can discard changes.
    ConfirmDiscard {
        form: EntryForm,
    },
    /// Delete confirmation modal overlay.
    ConfirmDelete {
        entry_id: Uuid,
        vault_id: Uuid,
        /// Entry name shown in the prompt.
        name: String,
    },
    /// Password generator popup, launched from a form value slot.
    /// The wrapped `EntryForm` is the form to return to on accept/cancel.
    PasswordGen {
        gen: PasswordGenView,
        form: EntryForm,
    },
    /// Vault creation form.
    VaultForm(VaultForm),
    /// Vault picker overlay launched from the entry form.
    /// Holds the form while the picker is open so the user can select a vault.
    FormVaultPicker {
        form: EntryForm,
        vault_state: ListState,
    },
    /// Delete vault confirmation modal overlay.
    ConfirmDeleteVault {
        vault_id: Uuid,
        /// Vault name shown in the prompt.
        name: String,
    },
    /// Lock screen — shown when the vault is locked.
    Locked {
        passphrase_input: rat_widget::text_input::TextInputState,
        error: Option<String>,
    },
}

/// Kind of status bar message — controls color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageKind {
    Success,
    Error,
    Info,
}

/// Top-level TUI application.
pub struct Tui<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> {
    pub context: TuiContext,
    pub state: RunState,
    pub app: App<K, F, I>,
    pub main_view: MainView,
    pub(super) active: ActiveView,
    pub(super) pending: Option<AppAction>,
    pub pending_copy: Option<PendingCopy>,
    pub error_message: Option<String>,
    /// Currently selected entry, fetched and decrypted on demand when selection changes.
    pub detail_entry: Option<Entry>,
    /// Transient status bar message shown after actions.
    pub status_message: Option<(String, Instant)>,
    /// Kind of the current status bar message.
    pub status_message_kind: MessageKind,
    /// Timestamp of the last keypress — used for auto-lock.
    pub last_keypress: Instant,
}

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> Tui<K, F, I> {
    /// Create a new `Tui` with the given startup context.
    ///
    /// Loads all vaults and their entries from the registry so `MainView`
    /// starts with real data rather than empty lists.
    pub async fn new(app: App<K, F, I>, context: TuiContext) -> bogita_core::error::Result<Self> {
        // Load vaults
        let vaults = app.registry.list_vaults().await?;

        // Load entries from every vault (empty while locked)
        let all_entries = app.all_entries().await?;

        let main_view = MainView::new(vaults.clone(), all_entries.clone());
        let detail_entry = all_entries.into_iter().next();
        let (active, error_message) = match &context {
            TuiContext::AddEntry { name, vault } => {
                let (vault_id, vault_name) = match resolve_vault(&vaults, vault.as_deref()) {
                    Some(vault) => (vault.id, vault.name.clone()),
                    None => (Uuid::nil(), String::new()),
                };
                let mut form = EntryForm::new_add(name.clone());
                form.set_vault(vault_id, vault_name);
                (ActiveView::Form(form), None)
            }
            TuiContext::AddVault { name } => {
                (ActiveView::VaultForm(VaultForm::new(name.clone())), None)
            }
            TuiContext::EditEntry { name, vault } => {
                match find_named_entry(&app, &vaults, vault.as_deref(), name).await? {
                    Some((entry, vault)) => {
                        let mut form = EntryForm::new_edit(&entry);
                        form.set_vault(vault.id, vault.name.clone());
                        (ActiveView::Form(form), None)
                    }
                    None => (ActiveView::Main, Some(entry_not_found(name))),
                }
            }
            TuiContext::DeleteEntry { name, vault } => {
                match find_named_entry(&app, &vaults, vault.as_deref(), name).await? {
                    Some((entry, vault)) => (
                        ActiveView::ConfirmDelete {
                            entry_id: entry.id,
                            vault_id: vault.id,
                            name: entry.name.clone(),
                        },
                        None,
                    ),
                    None => (ActiveView::Main, Some(entry_not_found(name))),
                }
            }
            TuiContext::Default => (ActiveView::Main, None),
        };
        Ok(Self {
            context,
            state: RunState::Running,
            app,
            main_view,
            active,
            pending: None,
            pending_copy: None,
            error_message,
            detail_entry,
            status_message: None,
            status_message_kind: MessageKind::Info,
            last_keypress: Instant::now(),
        })
    }

    /// Run the TUI event loop.
    pub async fn run(mut self) -> Result<()> {
        let terminal = ratatui::init();
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::cursor::SetCursorStyle::SteadyBar
        )?;
        let result = self.event_loop(terminal).await;
        ratatui::restore();
        result
    }

    async fn event_loop(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.state == RunState::Running {
            terminal.draw(|f| self.render(f))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.last_keypress = Instant::now();
                        self.handle_key_with_modifiers(key.code, key.modifiers);
                        self.flush_pending().await?;
                    }
                }
            }
            // Auto-lock check: lock after lock_timeout seconds of inactivity
            if let Some(timeout) = self.app.lock_timeout {
                if !self.app.is_locked
                    && self.last_keypress.elapsed() >= Duration::from_secs(timeout)
                {
                    self.lock();
                }
            }
            // Auto-dismiss expired status messages
            if let Some((_, created_at)) = &self.status_message {
                if created_at.elapsed() >= Duration::from_secs(3) {
                    self.status_message = None;
                }
            }
        }
        Ok(())
    }

    /// Lock the vault: clear identity, clear entries, show lock screen.
    pub(super) fn lock(&mut self) {
        if let Err(e) = self.app.lock() {
            self.error_message = Some(e.to_string());
            return;
        }
        self.main_view.reload_entries(Vec::new());
        self.detail_entry = None;
        self.active = ActiveView::Locked {
            passphrase_input: TextInputState::named("passphrase"),
            error: None,
        };
    }
}

/// Resolve the vault to use: the named vault when it exists, else the default
/// vault, else the first vault.
fn resolve_vault<'a>(vaults: &'a [Vault], name: Option<&str>) -> Option<&'a Vault> {
    name.and_then(|name| vaults.iter().find(|v| v.name == name))
        .or_else(|| vaults.iter().find(|v| v.is_default))
        .or_else(|| vaults.first())
}

/// Find an entry by name in the requested vault, or in the default vault when
/// no vault name is given.
async fn find_named_entry<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore>(
    app: &App<K, F, I>,
    vaults: &[Vault],
    vault_name: Option<&str>,
    entry_name: &str,
) -> Result<Option<(Entry, Vault)>> {
    let Some(vault) = resolve_vault(vaults, vault_name) else {
        return Ok(None);
    };
    let Some(service) = app.service_for(vault) else {
        return Ok(None);
    };
    let entries = service.list_entries(vault.id, None).await?;
    Ok(entries
        .into_iter()
        .find(|e| e.name == entry_name)
        .map(|entry| (entry, vault.clone())))
}

fn entry_not_found(name: &str) -> String {
    format!("entry '{name}' not found")
}
