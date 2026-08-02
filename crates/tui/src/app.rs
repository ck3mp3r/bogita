//! TUI application — event loop and rendering.

use crate::context::TuiContext;
use crate::views::entry_form::{EntryForm, FormAction, FormMode};
use crate::views::main_view::{MainView, MainViewAction};
use crate::views::password_gen_view::{PasswordGenAction, PasswordGenView};
use crate::views::vault_form::{VaultForm, VaultFormAction};
use bogita_core::app::App;
use bogita_core::domain::Entry;
use bogita_core::error::Result;
use bogita_core::ports::KeychainStore;
use bogita_core::service::clipboard::{ArboardBackend, ClipboardService};
use rat_widget::text_input::TextInputState;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Running state of the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunState {
    Running,
    Quit,
}

/// Which view is currently active.
enum ActiveView {
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

/// A deferred async action set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`] in the async event loop.
enum AppAction {
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

/// Kind of status bar message — controls color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageKind {
    Success,
    Error,
    Info,
}

/// A deferred clipboard copy set by [`Tui::handle_key`] and drained by
/// [`Tui::flush_pending`]. Kept separate from `AppAction` so the clipboard
/// path never blocks a vault mutation and vice-versa.
pub struct PendingCopy {
    pub entry_id: Uuid,
    pub vault_id: Uuid,
    pub field_idx: usize,
}

/// Top-level TUI application.
pub struct Tui<K: KeychainStore> {
    pub context: TuiContext,
    pub state: RunState,
    pub app: App<K>,
    pub main_view: MainView,
    active: ActiveView,
    pending: Option<AppAction>,
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

impl<K: KeychainStore> Tui<K> {
    /// Create a new `Tui` with the given startup context.
    ///
    /// Loads all vaults and their entries from the registry so `MainView`
    /// starts with real data rather than empty lists.
    pub async fn new(app: App<K>, context: TuiContext) -> bogita_core::error::Result<Self> {
        // Load vaults
        let vaults = app.registry.list_vaults().await?;

        // Load entries from every vault (skip if locked)
        let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
        if let Some(identity) = &app.identity {
            for vault in &vaults {
                let svc = app.registry.vault_service_for(vault, identity.clone());
                let entries = svc.list_entries(vault.id, None).await?;
                all_entries.extend(entries);
            }
        }

        let main_view = MainView::new(vaults.clone(), all_entries.clone());
        let detail_entry = all_entries.into_iter().next();
        let active = match &context {
            TuiContext::AddEntry { name } => {
                let vault_id = vaults
                    .iter()
                    .find(|v| v.is_default)
                    .map(|v| v.id)
                    .unwrap_or_else(|| vaults.first().map(|v| v.id).unwrap_or(Uuid::nil()));
                let vault_name = vaults
                    .iter()
                    .find(|v| v.id == vault_id)
                    .map(|v| v.name.clone())
                    .unwrap_or_default();
                let mut form = EntryForm::new_add(name.clone());
                form.set_vault(vault_id, vault_name);
                ActiveView::Form(form)
            }
            TuiContext::AddVault { name } => ActiveView::VaultForm(VaultForm::new(name.clone())),
            _ => ActiveView::Main,
        };
        Ok(Self {
            context,
            state: RunState::Running,
            app,
            main_view,
            active,
            pending: None,
            pending_copy: None,
            error_message: None,
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
    fn lock(&mut self) {
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

        // Error overlay: [Esc] or [Enter] dismisses.
        if self.error_message.is_some() {
            if matches!(key, KeyCode::Esc | KeyCode::Enter) {
                self.error_message = None;
            }
            return;
        }

        match &mut self.active {
            ActiveView::Form(f) => {
                // [g] on a token slot (obscured value) opens the password generator.
                if key == KeyCode::Char('g')
                    && modifiers.is_empty()
                    && f.focused_slot_label() == "token"
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

    /// Render the current frame.
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Split into header (1) | body (fill) | status bar (1)
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);
        let header_area = layout[0];
        let body_area = layout[1];
        let status_area = layout[2];

        // Header
        frame.render_widget(
            Paragraph::new(self.header_text()).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            header_area,
        );

        // Body: render Cols 1+2 for unlocked views; for locked, clear the body first.
        match &mut self.active {
            ActiveView::Locked {
                passphrase_input,
                error,
            } => {
                // Clear the full body area so vault names don't leak behind the lock screen.
                frame.render_widget(Clear, body_area);
                render_lock_screen(frame, body_area, passphrase_input, error.as_deref());
            }
            _ => {
                let col3 = self.main_view.render_cols_1_2(frame, body_area);
                match &mut self.active {
                    ActiveView::Main => {
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        if self.main_view.is_leader_mode() {
                            self.main_view.render_leader_overlay(frame, body_area);
                        }
                        if self.main_view.is_vault_picker_open() {
                            self.main_view.render_vault_picker(frame, body_area);
                        }
                    }
                    ActiveView::Form(f) => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as centered overlay on top.
                        let overlay = centered_rect(80, 90, body_area);
                        f.render(frame, overlay);
                    }
                    ActiveView::FormVaultPicker { form, vault_state } => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as centered overlay.
                        let overlay = centered_rect(80, 90, body_area);
                        form.render(frame, overlay);
                        // Render vault picker on top.
                        let vaults = self.main_view.vaults_snapshot();
                        render_form_vault_picker(frame, body_area, &vaults, vault_state);
                    }
                    ActiveView::ConfirmSave { form } => {
                        let name = form.name().to_string();
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as centered overlay, then confirm modal on top.
                        let overlay = centered_rect(80, 90, body_area);
                        form.render(frame, overlay);
                        render_confirm_save_modal(frame, body_area, &name);
                    }
                    ActiveView::ConfirmDiscard { form } => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as centered overlay, then discard confirm modal on top.
                        let overlay = centered_rect(80, 90, body_area);
                        form.render(frame, overlay);
                        render_confirm_discard_modal(frame, body_area);
                    }
                    ActiveView::ConfirmDelete { name, .. } => {
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        render_confirm_delete_modal(frame, body_area, name);
                    }
                    ActiveView::PasswordGen { gen, form } => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as centered overlay, then gen popup on top.
                        let overlay = centered_rect(80, 90, body_area);
                        form.render(frame, overlay);
                        gen.render(frame, body_area);
                    }
                    ActiveView::VaultForm(f) => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render vault form as centered overlay.
                        let overlay = centered_rect(80, 90, body_area);
                        f.render(frame, overlay);
                    }
                    ActiveView::ConfirmDeleteVault { name, .. } => {
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        render_confirm_delete_vault_modal(frame, body_area, name);
                    }
                    // Locked is handled above — unreachable here.
                    ActiveView::Locked { .. } => unreachable!(),
                }
            }
        }

        // Error overlay: rendered on top of everything else.
        if let Some(msg) = &self.error_message.clone() {
            render_error_modal(frame, body_area, msg);
        }

        // Status bar — show active status message or context hint
        let status_text = if let Some((msg, created_at)) = &self.status_message {
            if created_at.elapsed() < Duration::from_secs(3) {
                msg.clone()
            } else {
                self.status_hint()
            }
        } else {
            self.status_hint()
        };
        let status_color = if self.status_message.is_some() {
            match self.status_message_kind {
                MessageKind::Success => Color::Green,
                MessageKind::Error => Color::Red,
                MessageKind::Info => Color::DarkGray,
            }
        } else {
            Color::DarkGray
        };
        frame.render_widget(
            Paragraph::new(status_text).style(Style::default().fg(status_color)),
            status_area,
        );
    }

    /// One-line status bar hint text, context-aware.
    pub fn status_hint(&self) -> String {
        if self.error_message.is_some() {
            return "[Esc / Enter] dismiss".to_string();
        }
        match &self.active {
            ActiveView::ConfirmDelete { .. } => "[d] delete  [c/Esc] cancel".to_string(),
            ActiveView::ConfirmDeleteVault { .. } => "[d] delete  [c/Esc] cancel".to_string(),
            ActiveView::ConfirmSave { .. } => {
                "[s] save  [d] discard  [Esc] back to form".to_string()
            }
            ActiveView::ConfirmDiscard { .. } => "[d] discard  [b/Esc] back to form".to_string(),
            ActiveView::Form(f) => {
                let dirty_suffix = if f.is_dirty() {
                    "  ·  unsaved changes"
                } else {
                    ""
                };
                let validation_warning = if f.has_validation_errors() {
                    format!("  ⚠ {}", f.validation_error_summary())
                } else {
                    String::new()
                };
                let slot = f.focused_slot_label();
                match slot {
                    "name" => {
                        format!("Editing name  ·  [Tab] next field  [Enter] save  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    "key" => {
                        let n = (f.focused_field().saturating_sub(1)) / 3 + 1;
                        let m = f.field_count();
                        format!("Editing key (field {n} of {m})  ·  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    "value" => format!("Editing value  ·  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}"),
                    "token" => {
                        format!("Editing token  ·  [g] generate  [Ctrl-r] toggle reveal  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    "type" => {
                        let badge = f.field_badge((f.focused_field().saturating_sub(1)) / 3);
                        format!(
                            "[j/k] select type  ·  currently: {badge}  ·  [Enter] save  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}"
                        )
                    }
                    _ => format!("[Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}"),
                }
            }
            ActiveView::Main => {
                if self.main_view.is_vault_picker_open() {
                    return "[j/k] select  [a] add vault  [d] delete vault  [Enter] confirm  [Esc] close".to_string();
                }
                if self.main_view.is_leader_mode() {
                    use crate::views::main_view::Column;
                    match self.main_view.focused {
                        Column::Entries => {
                            "[a] add  [e] edit  [d] delete  [Esc] cancel".to_string()
                        }
                        Column::Detail => "[Esc] cancel".to_string(),
                    }
                } else {
                    use crate::views::main_view::Column;
                    if self.main_view.focused == Column::Detail {
                        "[j/k] scroll  [s] reveal  [c] copy  [Tab] columns  [v] vault  [Space] actions  [q] quit".to_string()
                    } else {
                        "[/] search  [v] vault  [Space] actions  [j/k] scroll  [Tab] focus  [q] quit"
                            .to_string()
                    }
                }
            }
            ActiveView::PasswordGen { .. } => {
                "[g] regenerate  [a] accept  [Esc] cancel  [+/-] length  [u/l/d/s/x] charset"
                    .to_string()
            }
            ActiveView::FormVaultPicker { .. } => {
                "[j/k] select  [a] add vault  [Enter] confirm  [Esc] close".to_string()
            }
            ActiveView::VaultForm(f) => {
                if f.is_name_focused() {
                    "Editing vault name  ·  [Tab] toggle default  [Enter] create  [Esc] cancel"
                        .to_string()
                } else {
                    "[Space] toggle default  [Tab] back to name  [Enter] create  [Esc] cancel"
                        .to_string()
                }
            }
            ActiveView::Locked { error, .. } => {
                if error.is_some() {
                    "Wrong passphrase. Try again.  [Esc] quit".to_string()
                } else {
                    "Enter passphrase to unlock  [Esc] quit".to_string()
                }
            }
        }
    }

    /// One-line header text: app name · entry count.
    pub fn header_text(&self) -> String {
        let entry_count = self.main_view.visible_entries().len();
        let noun = if entry_count == 1 { "entry" } else { "entries" };
        format!("bogita  ·  {entry_count} {noun}")
    }

    /// Handle a key press. Returns the new [`RunState`].
    ///
    /// Kept synchronous so tests can drive it without a real terminal.
    /// Async mutations are deferred to `self.pending`; call `flush_pending`
    /// afterwards to drain them.
    pub fn handle_key(&mut self, key: KeyCode) -> RunState {
        // Error overlay: [Esc] or [Enter] dismisses.
        if self.error_message.is_some() {
            if matches!(key, KeyCode::Esc | KeyCode::Enter) {
                self.error_message = None;
            }
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
                Ok(Some(ref msg)) => {
                    self.status_message = Some((msg.to_string(), Instant::now()));
                    self.status_message_kind = MessageKind::Success;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status_message = Some((e.to_string(), Instant::now()));
                    self.status_message_kind = MessageKind::Error;
                }
            }
        }

        // Drain clipboard copy.
        if let Some(pc) = self.pending_copy.take() {
            match self.do_copy(pc).await {
                Ok(()) => {
                    self.status_message = Some((
                        "Copied to clipboard (30s timeout)".to_string(),
                        Instant::now(),
                    ));
                    self.status_message_kind = MessageKind::Success;
                }
                Err(e) => {
                    self.status_message = Some((e.to_string(), Instant::now()));
                    self.status_message_kind = MessageKind::Error;
                }
            }
        }

        Ok(())
    }

    /// Inner flush implementation — returns `Ok(Some(msg))` on success with a
    /// status message, `Ok(None)` on success without a message, or `Err` on failure.
    async fn do_flush(&mut self, action: AppAction) -> Result<Option<String>> {
        let vaults = self.app.registry.list_vaults().await?;
        let identity = match self.app.identity.as_ref() {
            Some(id) => id.clone(),
            None => return Ok(None), // locked — no-op
        };

        match action {
            AppAction::SaveEntry {
                entry,
                mode,
                select_id,
            } => {
                // Find the vault that owns this entry
                let vault = vaults.iter().find(|v| v.id == entry.vault_id);
                if let Some(v) = vault {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
                    match mode {
                        FormMode::Add => svc.add_entry(&entry).await?,
                        FormMode::Edit => svc.update_entry(&entry).await?,
                    }
                }

                // Reload and re-select the saved entry
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for vault in &vaults {
                    let svc = self.app.registry.vault_service_for(vault, identity.clone());
                    let entries = svc.list_entries(vault.id, None).await?;
                    all_entries.extend(entries);
                }
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
                if let Some(v) = vault {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
                    svc.delete_entry(entry_id).await?;
                }

                // Reload after delete — reset to index 0
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for vault in &vaults {
                    let svc = self.app.registry.vault_service_for(vault, identity.clone());
                    let entries = svc.list_entries(vault.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

                Ok(Some("Entry deleted".to_string()))
            }
            AppAction::LoadDetail { entry_id, vault_id } => {
                let vault = vaults.iter().find(|v| v.id == vault_id);
                if let Some(v) = vault {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
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
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for v in &vaults {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
                    let entries = svc.list_entries(v.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);

                Ok(Some("Vault created".to_string()))
            }
            AppAction::DeleteVault { vault_id } => {
                self.app.registry.remove_vault(vault_id).await?;
                // Reload vaults and entries
                let vaults = self.app.registry.list_vaults().await?;
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for v in &vaults {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
                    let entries = svc.list_entries(v.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

                Ok(Some("Vault deleted".to_string()))
            }
            AppAction::ReloadEntries => {
                // Reload entries after unlock
                let vaults = self.app.registry.list_vaults().await?;
                let identity = match self.app.identity.as_ref() {
                    Some(id) => id.clone(),
                    None => return Ok(None),
                };
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for v in &vaults {
                    let svc = self.app.registry.vault_service_for(v, identity.clone());
                    let entries = svc.list_entries(v.id, None).await?;
                    all_entries.extend(entries);
                }
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

        let svc = ClipboardService::new(ArboardBackend);
        svc.copy_with_timeout(SecretString::from(text), 30).await
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Render a centered error modal over the given area.
fn render_error_modal(frame: &mut Frame, area: Rect, message: &str) {
    // Modal: up to 60 wide, at least 5 tall (more if message is long).
    let modal_w = 60u16.min(area.width);
    // Wrap message into modal_w - 4 (2 border + 2 padding) columns.
    let inner_w = (modal_w as usize).saturating_sub(4);
    let wrapped: Vec<&str> = if message.len() <= inner_w {
        vec![message]
    } else {
        // Simple word-wrap at inner_w chars.
        message
            .as_bytes()
            .chunks(inner_w)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect()
    };
    let modal_h = (wrapped.len() as u16 + 4).min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " Error ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let msg_lines: Vec<Line> = wrapped
        .iter()
        .map(|s| Line::from(Span::styled(*s, Style::default().fg(Color::White))))
        .collect();
    frame.render_widget(Paragraph::new(msg_lines), rows[0]);

    let hint = Paragraph::new(Span::styled(
        "[Esc / Enter] dismiss",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(hint, rows[1]);
}

/// Compute a centered rectangle within `area` at the given width and height
/// percentages. The result is centered both horizontally and vertically.
pub fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render a small centered confirmation modal over the full terminal area.
fn render_confirm_delete_modal(frame: &mut Frame, area: Rect, entry_name: &str) {
    // Modal dimensions: 40 wide, 5 tall
    let modal_w = 44u16.min(area.width);
    let modal_h = 5u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    // Clear the area beneath the modal so the box is readable.
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(" Delete entry ");

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let name_line = Line::from(vec![
        Span::raw("  Delete "),
        Span::styled(
            entry_name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ]);
    frame.render_widget(Paragraph::new(name_line), rows[0]);

    let hint =
        Paragraph::new("  [d] delete  [c/Esc] cancel").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, rows[1]);
}

/// Render a small centered confirmation modal for deleting a vault.
fn render_confirm_delete_vault_modal(frame: &mut Frame, area: Rect, vault_name: &str) {
    let modal_w = 44u16.min(area.width);
    let modal_h = 5u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(" Delete vault ");

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let name_line = Line::from(vec![
        Span::raw("  Delete vault "),
        Span::styled(
            vault_name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ]);
    frame.render_widget(Paragraph::new(name_line), rows[0]);

    let hint =
        Paragraph::new("  [d] delete  [c/Esc] cancel").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, rows[1]);
}

/// Render a small centered confirmation modal for saving edits.
fn render_confirm_save_modal(frame: &mut Frame, area: Rect, entry_name: &str) {
    let modal_w = 44u16.min(area.width);
    let modal_h = 5u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Save changes ");

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let name_line = Line::from(vec![
        Span::raw("  Save changes to "),
        Span::styled(
            entry_name,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("?"),
    ]);
    frame.render_widget(Paragraph::new(name_line), rows[0]);

    let hint = Paragraph::new("  [s] save  [d] discard  [Esc] back to form")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, rows[1]);
}

/// Render a small centered confirmation modal for discarding changes.
fn render_confirm_discard_modal(frame: &mut Frame, area: Rect) {
    let modal_w = 44u16.min(area.width);
    let modal_h = 5u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Discard changes ");

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let msg = Paragraph::new("  You have unsaved changes. Discard?")
        .style(Style::default().fg(Color::White));
    frame.render_widget(msg, rows[0]);

    let hint = Paragraph::new("  [d] discard  [b/Esc] back to form")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, rows[1]);
}

/// Render the lock screen: a centered modal with a masked passphrase prompt.
fn render_lock_screen(
    frame: &mut Frame,
    area: Rect,
    passphrase_input: &TextInputState,
    error: Option<&str>,
) {
    let modal_w = 50u16.min(area.width);
    let modal_h = 8u16.min(area.height);
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(x, y, modal_w, modal_h);

    frame.render_widget(Clear, modal_area);

    let border_color = if error.is_some() {
        Color::Red
    } else {
        Color::Yellow
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Locked ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Error text
    if let Some(err) = error {
        frame.render_widget(
            Paragraph::new(Span::styled(err, Style::default().fg(Color::Red))),
            rows[0],
        );
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Enter passphrase:",
                Style::default().fg(Color::White),
            )),
            rows[0],
        );
    }

    // Masked passphrase input
    let masked: String = passphrase_input.text().chars().map(|_| '•').collect();
    let input_widget = Paragraph::new(Span::styled(masked, Style::default().fg(Color::Cyan)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(input_widget, rows[1]);

    // Cursor position indicator
    let cursor_pos = passphrase_input.text().len();
    let cursor_text = if cursor_pos > 0 {
        format!("{} chars", cursor_pos)
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            cursor_text,
            Style::default().fg(Color::DarkGray),
        )),
        rows[2],
    );

    // Hint line
    frame.render_widget(
        Paragraph::new(Span::styled(
            "[Enter] unlock  [Esc] quit",
            Style::default().fg(Color::DarkGray),
        )),
        rows[3],
    );
}

/// Render a vault picker dropdown overlay on top of the entry form.
fn render_form_vault_picker(
    frame: &mut Frame,
    area: Rect,
    vaults: &[bogita_core::domain::Vault],
    state: &mut ListState,
) {
    let width = 26u16.min(area.width);
    let height = (vaults.len() as u16 + 4).min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let dropdown = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dropdown);

    let block = Block::default()
        .title(" Vault ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(dropdown);
    frame.render_widget(block, dropdown);

    let mut items: Vec<ListItem> = vec![ListItem::new("All Vaults")];
    for v in vaults {
        items.push(ListItem::new(v.name.as_str()));
    }

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, inner, state);
}
