//! Tui application shell — struct, constructor, event loop, and lock.

use super::actions::{AppAction, PendingCopy};
use super::signals;
use crate::context::TuiContext;
use crate::views::entry_form::EntryForm;
use crate::views::main_view::MainView;
use crate::views::password_gen_view::PasswordGenView;
use crate::views::vault_form::VaultForm;
use bogita_core::agent::socket::default_socket_path;
use bogita_core::agent::{client, VaultService};
use bogita_core::domain::{Entry, Vault};
use bogita_core::error::Result;
use rat_widget::text_input::TextInputState;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;
use std::path::PathBuf;
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
pub struct Tui<V: VaultService> {
    pub context: TuiContext,
    pub state: RunState,
    /// The composed vault service: the TUI's only owner of the registry and the lock.
    pub service: V,
    /// Idle auto-lock timeout in seconds, from the vault configuration.
    pub lock_timeout: Option<u64>,
    /// Socket of the agent daemon the exit lock asks to lock. The derived default in
    /// production; an explicit path when a caller serves elsewhere.
    daemon_socket: PathBuf,
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
    /// `Some(scroll)` while the `?` keybindings overlay is open.
    pub(super) help: Option<u16>,
    /// Rows the help overlay last painted — half of this is one Ctrl-d step.
    pub(super) help_viewport: u16,
    /// The furthest the help overlay may scroll, as of the last frame.
    pub(super) help_max_scroll: u16,
    /// Whether a `g` is waiting for its second `g` in the help overlay. One key
    /// of memory, cleared by any other key.
    pub(super) help_pending_g: bool,
}

impl<V: VaultService> Tui<V> {
    /// Create a new `Tui` over the composed vault service.
    ///
    /// Loads all vaults and their entries through the service so `MainView`
    /// starts with real data rather than empty lists.
    pub async fn new(service: V, context: TuiContext) -> Result<Self> {
        // Load vaults
        let vaults = service.list_vaults().await?;

        // Load entries from every vault (empty while locked)
        let all_entries = service.all_entries().await?;

        let main_view = MainView::new(vaults.clone(), all_entries.clone());
        let detail_entry = all_entries.into_iter().next();
        // A locked service boots on its own lock screen: the passphrase is asked in
        // here, never on a terminal prompt. The startup context is dropped in
        // that case — its intent is re-dispatched after unlock (deferred).
        let (active, error_message) = if service.is_locked() {
            (
                ActiveView::Locked {
                    passphrase_input: TextInputState::named("passphrase"),
                    error: None,
                },
                None,
            )
        } else {
            match &context {
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
                    match find_named_entry(&service, &vaults, vault.as_deref(), name).await? {
                        Some((entry, vault)) => {
                            let mut form = EntryForm::new_edit(&entry);
                            form.set_vault(vault.id, vault.name.clone());
                            (ActiveView::Form(form), None)
                        }
                        None => (ActiveView::Main, Some(entry_not_found(name))),
                    }
                }
                TuiContext::DeleteEntry { name, vault } => {
                    match find_named_entry(&service, &vaults, vault.as_deref(), name).await? {
                        Some((entry, _)) => (
                            ActiveView::ConfirmDelete {
                                entry_id: entry.id,
                                name: entry.name.clone(),
                            },
                            None,
                        ),
                        None => (ActiveView::Main, Some(entry_not_found(name))),
                    }
                }
                TuiContext::Default => (ActiveView::Main, None),
            }
        };
        Ok(Self {
            context,
            state: RunState::Running,
            service,
            lock_timeout: None,
            daemon_socket: default_socket_path(),
            main_view,
            active,
            pending: None,
            pending_copy: None,
            error_message,
            detail_entry,
            status_message: None,
            status_message_kind: MessageKind::Info,
            last_keypress: Instant::now(),
            help: None,
            help_viewport: 0,
            help_max_scroll: 0,
            help_pending_g: false,
        })
    }

    /// Set the idle auto-lock timeout (seconds) the event loop enforces.
    pub fn with_lock_timeout(mut self, lock_timeout: Option<u64>) -> Self {
        self.lock_timeout = lock_timeout;
        self
    }

    /// Point the exit lock at the daemon socket `socket`.
    ///
    /// The derived default is the production socket; this override exists for a
    /// daemon serving elsewhere and for tests that must not reach a live daemon.
    pub fn with_daemon_socket(mut self, socket: impl Into<PathBuf>) -> Self {
        self.daemon_socket = socket.into();
        self
    }

    /// Lock when the idle cap elapsed. Returns true when this call locked.
    ///
    /// A service that is already locked is left alone: the view sync puts the lock
    /// screen up for it.
    pub(super) fn poll_idle_lock(&mut self) -> bool {
        let Some(timeout) = self.lock_timeout else {
            return false;
        };
        if self.service.is_locked() || self.last_keypress.elapsed() < Duration::from_secs(timeout) {
            return false;
        }
        self.lock();
        true
    }

    /// Show the lock screen whenever the service reports itself locked.
    ///
    /// The service owns the lock, so a lock taken through it is the only signal the
    /// view needs: the idle cap, an explicit request, or a failed unlock all leave
    /// the same is_locked answer.
    pub(super) fn sync_lock_view(&mut self) {
        if self.service.is_locked() && !matches!(self.active, ActiveView::Locked { .. }) {
            self.clear_locked_view();
        }
    }

    /// Run the TUI event loop.
    ///
    /// Every exit path ends here: quit keys, Ctrl-C and the termination signals
    /// all stop the loop, then the terminal is restored and the service locked.
    /// Restore comes first so a lock still leaves a usable terminal.
    pub async fn run(mut self) -> Result<()> {
        if let Err(e) = signals::install() {
            eprintln!("warning: cannot install the exit signal handlers: {e}");
        }
        let terminal = ratatui::init();
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::EnableBracketedPaste,
            ratatui::crossterm::cursor::SetCursorStyle::SteadyBar
        )?;
        let result = self.event_loop(terminal).await;
        // Give the terminal its plain paste mode back before anything else can
        // fail; a locked or dead terminal must not keep the bracketed mode on.
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableBracketedPaste
        );
        ratatui::restore();
        self.lock_at_exit();
        result
    }

    async fn event_loop(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.state == RunState::Running {
            // A termination signal ends the loop on the next tick; the poll
            // below bounds the delay to 250 ms.
            if signals::termination_requested() {
                self.state = RunState::Quit;
                break;
            }
            terminal.draw(|f| self.render(f))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.last_keypress = Instant::now();
                        self.handle_key_with_modifiers(key.code, key.modifiers);
                        self.flush_pending().await?;
                    }
                    Event::Paste(payload) => {
                        self.handle_paste(&payload);
                        self.flush_pending().await?;
                    }
                    _ => {}
                }
            }
            // Auto-lock on the vault's idle cap, then mirror the service's lock
            // state in the view: the service owns the lock, so the view follows it.
            self.poll_idle_lock();
            self.sync_lock_view();
            // Auto-dismiss expired status messages
            if let Some((_, created_at)) = &self.status_message {
                if created_at.elapsed() >= Duration::from_secs(3) {
                    self.status_message = None;
                }
            }
        }
        Ok(())
    }

    /// Lock the vault: drop the identity, clear entries, show lock screen.
    pub(super) fn lock(&mut self) {
        self.service.lock();
        self.clear_locked_view();
    }

    /// Lock on the way out, after `ratatui::restore()`.
    ///
    /// The lock is not optional: leaving the identity held keeps the agent
    /// daemon serving keys with no owner. Two locks run here. The local
    /// `VaultService::lock` drops the held identity and evicts this process's
    /// ring and cannot fail. The daemon owns its own lock, so the same request
    /// goes over the socket as well; a daemon that is missing or dead is
    /// already locked and answers nothing, so its error is ignored.
    pub(super) fn lock_at_exit(&mut self) {
        self.lock();
        let _ = client::lock(&self.daemon_socket);
    }

    /// Clear every decrypted entry and show the lock screen.
    fn clear_locked_view(&mut self) {
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
async fn find_named_entry<V: VaultService>(
    service: &V,
    vaults: &[Vault],
    vault_name: Option<&str>,
    entry_name: &str,
) -> Result<Option<(Entry, Vault)>> {
    let Some(vault) = resolve_vault(vaults, vault_name) else {
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
