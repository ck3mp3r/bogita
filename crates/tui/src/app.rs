//! TUI application — event loop and rendering.

use crate::context::TuiContext;
use crate::views::entry_form::{EntryForm, FormAction, FormMode};
use crate::views::main_view::{MainView, MainViewAction};
use crate::views::password_gen_view::{PasswordGenAction, PasswordGenView};
use crate::views::vault_form::{VaultForm, VaultFormAction};
use bogita_core::app::App;
use bogita_core::domain::Entry;
use bogita_core::error::Result;
use bogita_core::service::clipboard::{ArboardBackend, ClipboardService};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::time::Duration;
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
    /// Delete vault confirmation modal overlay.
    ConfirmDeleteVault {
        vault_id: Uuid,
        /// Vault name shown in the prompt.
        name: String,
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
}

/// Kind of toast notification — controls color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
}

/// Transient notification message shown briefly after successful actions.
#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created_at: std::time::Instant,
    pub duration: std::time::Duration,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Success,
            created_at: std::time::Instant::now(),
            duration: std::time::Duration::from_secs(3),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Warning,
            created_at: std::time::Instant::now(),
            duration: std::time::Duration::from_secs(5),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.duration
    }
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
pub struct Tui {
    pub context: TuiContext,
    pub state: RunState,
    pub app: App,
    pub main_view: MainView,
    active: ActiveView,
    pending: Option<AppAction>,
    pub pending_copy: Option<PendingCopy>,
    pub error_message: Option<String>,
    /// Currently selected entry, fetched and decrypted on demand when selection changes.
    pub detail_entry: Option<Entry>,
    /// Transient toast notification shown after successful actions.
    pub toast: Option<Toast>,
}

impl Tui {
    /// Create a new `Tui` with the given startup context.
    ///
    /// Loads all vaults and their entries from the registry so `MainView`
    /// starts with real data rather than empty lists.
    pub async fn new(app: App, context: TuiContext) -> bogita_core::error::Result<Self> {
        // Load vaults
        let vaults = app.registry.list_vaults().await?;

        // Load entries from every vault
        let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
        for vault in &vaults {
            let svc = app.registry.vault_service_for(vault, app.identity.clone());
            let entries = svc.list_entries(vault.id, None).await?;
            all_entries.extend(entries);
        }

        let main_view = MainView::new(vaults, all_entries.clone());
        let detail_entry = all_entries.into_iter().next();
        let active = match &context {
            TuiContext::AddEntry { name, .. } => ActiveView::Form(EntryForm::new_add(name.clone())),
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
            toast: None,
        })
    }

    /// Show a transient toast notification.
    pub(crate) fn show_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        let toast = match kind {
            ToastKind::Success => Toast::success(message),
            ToastKind::Warning => Toast::warning(message),
            ToastKind::Info => Toast::success(message), // Info uses same duration as Success
        };
        self.toast = Some(toast);
    }

    /// Run the TUI event loop.
    pub async fn run(mut self) -> Result<()> {
        let terminal = ratatui::init();
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
                        self.handle_key_with_modifiers(key.code, key.modifiers);
                        self.flush_pending().await?;
                    }
                }
            }
            // Auto-dismiss expired toasts
            if self.toast.as_ref().is_some_and(|t| t.is_expired()) {
                self.toast = None;
            }
        }
        Ok(())
    }

    /// Handle a key press with full modifier info.
    /// Used by the event loop to pass Ctrl-r to the form.
    pub(crate) fn handle_key_with_modifiers(&mut self, key: KeyCode, modifiers: KeyModifiers) {
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

        // Body: always render Cols 1+2; Col 3 depends on active view.
        let col3 = self.main_view.render_cols_1_2(frame, body_area);
        match &mut self.active {
            ActiveView::Main => {
                self.main_view
                    .render_detail(frame, col3, self.detail_entry.as_ref());
            }
            ActiveView::Form(f) => {
                // Render main view detail pane behind the overlay.
                self.main_view
                    .render_detail(frame, col3, self.detail_entry.as_ref());
                // Render form as centered overlay on top.
                let overlay = centered_rect(80, 90, body_area);
                f.render(frame, overlay);
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
        }

        // Error overlay: rendered on top of everything else.
        if let Some(msg) = &self.error_message.clone() {
            render_error_modal(frame, body_area, msg);
        }

        // Toast notification (rendered on top of everything in the body)
        if let Some(ref toast) = self.toast {
            render_toast(frame, body_area, toast);
        }

        // Status bar
        frame.render_widget(
            Paragraph::new(self.status_hint()).style(Style::default().fg(Color::DarkGray)),
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
                if self.main_view.is_leader_mode() {
                    use crate::views::main_view::Column;
                    match self.main_view.focused {
                        Column::Vaults => {
                            "[a] add vault  [d] delete vault  [Esc] cancel".to_string()
                        }
                        Column::Entries => {
                            "[a] add  [e] edit  [d] delete  [Esc] cancel".to_string()
                        }
                        Column::Detail => "[Esc] cancel".to_string(),
                    }
                } else {
                    use crate::views::main_view::Column;
                    if self.main_view.focused == Column::Detail {
                        "[j/k] scroll  [s] reveal  [c] copy  [Tab] columns  [Space] actions  [q] quit".to_string()
                    } else {
                        "[/] search  [Space] actions  [j/k] scroll  [Tab] focus  [q] quit"
                            .to_string()
                    }
                }
            }
            ActiveView::PasswordGen { .. } => {
                "[g] regenerate  [a] accept  [Esc] cancel  [+/-] length  [u/l/d/s/x] charset"
                    .to_string()
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
                        let mut form = EntryForm::new_add(None);
                        form.set_vault_id(vault_id);
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
                            self.active = ActiveView::Form(EntryForm::new_edit(&e));
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
                    vault.recipients = vec![self.app.identity.to_recipient()];
                    self.pending = Some(AppAction::SaveVault { vault });
                    self.active = ActiveView::Main;
                }
                VaultFormAction::Cancel => {
                    self.active = ActiveView::Main;
                }
                VaultFormAction::None => {}
            },
            // These are handled in handle_key_with_modifiers, never reached here.
            ActiveView::Form(_) | ActiveView::PasswordGen { .. } => {}
        }
        self.state.clone()
    }

    /// Drain `self.pending`: persist mutations and reload entries into `main_view`.
    ///
    /// Called by the event loop after every keypress. No-op when pending is `None`.
    /// On backend failure the error is stored in `self.error_message` and displayed
    /// as an overlay modal rather than propagating up and crashing the TUI.
    pub async fn flush_pending(&mut self) -> Result<()> {
        // Drain vault mutation (save / delete).
        if let Some(action) = self.pending.take() {
            match self.do_flush(action).await {
                Ok(Some(ref msg)) => {
                    self.show_toast(msg, ToastKind::Success);
                }
                Ok(None) => {}
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }

        // Drain clipboard copy.
        if let Some(pc) = self.pending_copy.take() {
            match self.do_copy(pc).await {
                Ok(()) => {
                    self.show_toast("Copied to clipboard (30s timeout)", ToastKind::Success);
                }
                Err(e) => {
                    self.error_message = Some(e.to_string());
                }
            }
        }

        Ok(())
    }

    /// Inner flush implementation — returns `Ok(Some(msg))` on success with a
    /// toast message, `Ok(None)` on success without a toast, or `Err` on failure.
    async fn do_flush(&mut self, action: AppAction) -> Result<Option<String>> {
        let vaults = self.app.registry.list_vaults().await?;

        match action {
            AppAction::SaveEntry {
                entry,
                mode,
                select_id,
            } => {
                // Find the vault that owns this entry
                let vault = vaults.iter().find(|v| v.id == entry.vault_id);
                if let Some(v) = vault {
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(v, self.app.identity.clone());
                    match mode {
                        FormMode::Add => svc.add_entry(&entry).await?,
                        FormMode::Edit => svc.update_entry(&entry).await?,
                    }
                }

                // Reload and re-select the saved entry
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for vault in &vaults {
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(vault, self.app.identity.clone());
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
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(v, self.app.identity.clone());
                    svc.delete_entry(entry_id).await?;
                }

                // Reload after delete — reset to index 0
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();
                for vault in &vaults {
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(vault, self.app.identity.clone());
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
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(v, self.app.identity.clone());
                    self.detail_entry = svc.get_entry(entry_id).await?;
                }

                Ok(None) // No toast for loading detail
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
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(v, self.app.identity.clone());
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
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(v, self.app.identity.clone());
                    let entries = svc.list_entries(v.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view.reload_vaults(vaults);
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;

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

/// Render a transient toast notification centered at the bottom of the body area.
fn render_toast(frame: &mut Frame, area: Rect, toast: &Toast) {
    let color = match toast.kind {
        ToastKind::Success => Color::Green,
        ToastKind::Info => Color::Cyan,
        ToastKind::Warning => Color::Yellow,
    };

    let msg_len = toast.message.len() as u16;
    let toast_w = (msg_len + 6).min(area.width * 60 / 100);
    let toast_h = 3u16;
    let x = area.x + area.width.saturating_sub(toast_w) / 2;
    let y = area.y + area.height.saturating_sub(toast_h).saturating_sub(2); // 2 rows above bottom
    let toast_area = Rect::new(x, y, toast_w, toast_h);

    frame.render_widget(Clear, toast_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(format!(" {} ", toast.message));

    frame.render_widget(block, toast_area);
}
