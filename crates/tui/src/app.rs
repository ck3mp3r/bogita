//! TUI application — event loop and rendering.

use bogita_core::app::App;
use bogita_core::domain::Entry;
use bogita_core::error::Result;
use bogita_core::service::clipboard::{ArboardBackend, ClipboardService};
use crate::context::TuiContext;
use crate::views::entry_form::{EntryForm, FormAction, FormMode};
use crate::views::main_view::{MainView, MainViewAction};
use crate::views::password_gen_view::{PasswordGenAction, PasswordGenView};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
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
    /// The form is kept so [Esc] can return to it, and [y] can re-confirm to get the entry.
    ConfirmSave {
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
}

impl Tui {
    /// Create a new `Tui` with the given startup context.
    ///
    /// Loads all vaults and their entries from the registry so `MainView`
    /// starts with real data rather than empty lists.
    pub async fn new(app: App, context: TuiContext) -> bogita_core::error::Result<Self> {        // Load vaults
        let vaults = app.registry.list_vaults().await?;

        // Load entries from every vault
        let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();        for vault in &vaults {
            let svc = app.registry.vault_service_for(vault, app.identity.clone());
            let entries = svc.list_entries(vault.id, None).await?;
            all_entries.extend(entries);
        }

        let main_view = MainView::new(vaults, all_entries.clone());
        let detail_entry = all_entries.into_iter().next();
        let active = match &context {
            TuiContext::AddEntry { name, .. } => ActiveView::Form(EntryForm::new_add(name.clone())),
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
        })
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
                        self.handle_key(key.code);
                        self.flush_pending().await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Render the current frame.
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Split into header (1 row) | body (fill) | status bar (1 row)
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
            ActiveView::Form(f) => f.render(frame, col3),
            ActiveView::ConfirmSave { form } => {
                let name = form.name().to_string();
                form.render(frame, col3);
                render_confirm_save_modal(frame, body_area, &name);
            }
            ActiveView::ConfirmDelete { name, .. } => {
                self.main_view
                    .render_detail(frame, col3, self.detail_entry.as_ref());
                render_confirm_delete_modal(frame, body_area, name);
            }
            ActiveView::PasswordGen { gen, form } => {
                // Form still visible in col3 underneath the popup.
                form.render(frame, col3);
                gen.render(frame, body_area);
            }
        }

        // Error overlay: rendered on top of everything else.
        if let Some(msg) = &self.error_message.clone() {
            render_error_modal(frame, body_area, msg);
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
            ActiveView::ConfirmDelete { .. } => "[y] confirm delete  [n / Esc] cancel".to_string(),
            ActiveView::ConfirmSave { .. } => {
                "[y] save  [n] discard  [Esc] back to form".to_string()
            }
            ActiveView::Form(f) => {
                let slot = f.focused_slot_label();
                match slot {
                    "name" => {
                        "Editing name  ·  [Tab] next field  [Enter] save  [Esc] cancel".to_string()
                    }
                    "key" => {
                        let n = (f.focused_field().saturating_sub(1)) / 3 + 1;
                        let m = f.field_count();
                        format!("Editing key (field {n} of {m})  ·  [Tab] next  [Esc] cancel")
                    }
                    "value" => "Editing value  ·  [Tab] next  [Esc] cancel".to_string(),
                    "token" => {
                        "Editing token  ·  [g] generate  [Tab] next  [Esc] cancel".to_string()
                    }
                    "type" => {
                        let badge = f.field_badge((f.focused_field().saturating_sub(1)) / 3);
                        format!(
                            "[j/k] select type  ·  currently: {badge}  ·  [Enter] save  [Tab] next  [Esc] cancel"
                        )
                    }
                    _ => "[Tab] next  [Esc] cancel".to_string(),
                }
            }
            ActiveView::Main => {
                if self.main_view.is_leader_mode() {
                    "[a] add  [e] edit  [d] delete  [Esc] cancel".to_string()
                } else {
                    use crate::views::main_view::Column;                    if self.main_view.focused == Column::Detail {
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
            ActiveView::Form(f) => {
                // [g] on a token slot (obscured value) opens the password generator.
                if key == KeyCode::Char('g') && f.focused_slot_label() == "token" {
                    // Swap form out, wrap in PasswordGen view.
                    let placeholder = EntryForm::new_add(None);
                    let form = std::mem::replace(f, placeholder);
                    self.active = ActiveView::PasswordGen {
                        gen: PasswordGenView::new(),
                        form,
                    };
                    return self.state.clone();
                }
                match f.handle_key(key) {
                    FormAction::Cancel => {
                        if f.mode() == FormMode::Edit {
                            // Edit mode: show confirm modal before discarding.
                            let placeholder = EntryForm::new_add(None);
                            let form = std::mem::replace(f, placeholder);
                            self.active = ActiveView::ConfirmSave { form };
                        } else {
                            self.active = ActiveView::Main;
                        }
                    }
                    FormAction::Confirm(_) => {
                        let mode = f.mode();
                        if mode == FormMode::Edit {
                            // Edit mode: require confirmation before saving.
                            let placeholder = EntryForm::new_add(None);
                            let form = std::mem::replace(f, placeholder);
                            self.active = ActiveView::ConfirmSave { form };
                        } else {
                            // Add mode: build entry and save immediately.
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
            ActiveView::ConfirmSave { .. } => {
                // Destructure by replacing active to get ownership.
                let ActiveView::ConfirmSave { mut form } =
                    std::mem::replace(&mut self.active, ActiveView::Main)
                else {
                    unreachable!()
                };
                match key {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
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
                    KeyCode::Char('n') | KeyCode::Char('N') => {
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
            ActiveView::ConfirmDelete {
                entry_id, vault_id, ..
            } => match key {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.pending = Some(AppAction::DeleteEntry {
                        entry_id: *entry_id,
                        vault_id: *vault_id,
                    });
                    self.active = ActiveView::Main;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.active = ActiveView::Main;
                }
                _ => {}
            },
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
            if let Err(e) = self.do_flush(action).await {
                self.error_message = Some(e.to_string());
            }
        }

        // Drain clipboard copy.
        if let Some(pc) = self.pending_copy.take() {
            if let Err(e) = self.do_copy(pc).await {
                self.error_message = Some(e.to_string());
            }
        }

        Ok(())
    }

    /// Inner flush implementation — returns `Err` on backend failure.
    async fn do_flush(&mut self, action: AppAction) -> Result<()> {
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
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();                for vault in &vaults {
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(vault, self.app.identity.clone());
                    let entries = svc.list_entries(vault.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view
                    .reload_entries_select(all_entries, Some(select_id));
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
                let mut all_entries: Vec<bogita_core::domain::Entry> = Vec::new();                for vault in &vaults {
                    let svc = self
                        .app
                        .registry
                        .vault_service_for(vault, self.app.identity.clone());
                    let entries = svc.list_entries(vault.id, None).await?;
                    all_entries.extend(entries);
                }
                self.main_view.reload_entries(all_entries);
                self.detail_entry = None;
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
            }
        }

        Ok(())
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

    let hint = Paragraph::new("  [y] confirm  [n / Esc] cancel")
        .style(Style::default().fg(Color::DarkGray));
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

    let hint = Paragraph::new("  [y] save  [n] discard  [Esc] back to form")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, rows[1]);
}
