//! Frame rendering — layout, view dispatch, and status chrome.

use super::bindings;
use super::modals::{
    centered_rect, render_confirm_delete_modal, render_confirm_delete_vault_modal,
    render_confirm_discard_modal, render_confirm_save_modal, render_error_modal,
    render_form_vault_picker, render_lock_screen,
};
use super::tui::{ActiveView, MessageKind, Tui};
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::time::Duration;

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> Tui<K, F, I> {
    /// Render the current frame.
    pub(super) fn render(&mut self, frame: &mut Frame) {
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

        if self.help.is_some() {
            self.render_help_overlay(frame, body_area);
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

    /// One-line status bar hint text, generated from the binding tables.
    pub fn status_hint(&self) -> String {
        use bindings::Surface;
        if self.error_message.is_some() {
            return bindings::surface_hint(Surface::ErrorOverlay);
        }
        if self.help.is_some() {
            return bindings::titled(bindings::title(Surface::Help), bindings::HELP);
        }
        match &self.active {
            ActiveView::ConfirmDelete { .. } | ActiveView::ConfirmDeleteVault { .. } => {
                bindings::surface_hint(Surface::ConfirmDelete)
            }
            ActiveView::ConfirmSave { .. } => bindings::surface_hint(Surface::ConfirmSave),
            ActiveView::ConfirmDiscard { .. } => bindings::surface_hint(Surface::ConfirmDiscard),
            ActiveView::Form(f) => {
                let hint = bindings::form_hint(
                    &f.focused_slot(),
                    f.focused_field(),
                    f.field_count(),
                    f.field_badge((f.focused_field().saturating_sub(1)) / 3),
                    f.focused_field_is_obscured(),
                );
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
                format!("{hint}{dirty_suffix}{validation_warning}")
            }
            ActiveView::Main => {
                if self.main_view.is_vault_picker_open() {
                    return bindings::surface_hint(Surface::MainVaultPicker);
                }
                if self.main_view.is_leader_mode() {
                    use crate::views::main_view::Column;
                    return match self.main_view.focused {
                        Column::Entries => bindings::surface_hint(Surface::MainLeaderEntries),
                        Column::Detail => bindings::surface_hint(Surface::MainLeaderDetail),
                    };
                }
                if self.main_view.is_searching() {
                    return bindings::surface_hint(Surface::Search);
                }
                use crate::views::main_view::Column;
                if self.main_view.focused == Column::Detail {
                    bindings::surface_hint(Surface::MainDetail)
                } else {
                    bindings::surface_hint(Surface::MainEntries)
                }
            }
            ActiveView::PasswordGen { .. } => bindings::surface_hint(Surface::PasswordGen),
            ActiveView::FormVaultPicker { .. } => bindings::surface_hint(Surface::FormVaultPicker),
            ActiveView::VaultForm(f) => bindings::vault_form_hint(f.is_name_focused()),
            ActiveView::Locked { error, .. } => bindings::locked_hint(error.is_some()),
        }
    }

    /// The surface whose keys the help overlay should list.
    ///
    /// `None` means the whole registry — the Main view has no single table.
    fn help_surface(&self) -> Option<bindings::Surface> {
        use bindings::Surface;
        match &self.active {
            ActiveView::Main => None,
            ActiveView::ConfirmSave { .. } => Some(Surface::ConfirmSave),
            ActiveView::ConfirmDiscard { .. } => Some(Surface::ConfirmDiscard),
            ActiveView::ConfirmDelete { .. } | ActiveView::ConfirmDeleteVault { .. } => {
                Some(Surface::ConfirmDelete)
            }
            ActiveView::Form(f) => Some(bindings::form_surface(
                &f.focused_slot(),
                f.focused_field_is_obscured(),
            )),
            ActiveView::PasswordGen { .. } => Some(Surface::PasswordGen),
            ActiveView::FormVaultPicker { .. } => Some(Surface::FormVaultPicker),
            ActiveView::VaultForm(f) => Some(if f.is_name_focused() {
                Surface::VaultFormName
            } else {
                Surface::VaultFormCheckbox
            }),
            ActiveView::Locked { error, .. } => Some(if error.is_some() {
                Surface::LockedError
            } else {
                Surface::Locked
            }),
        }
    }

    /// The `?` keybindings overlay: every key of the focused surface, or the
    /// whole registry on Main. `j`/`k` scroll; `?`/Esc/`q` close.
    fn render_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let modal = centered_rect(80, 80, area);
        frame.render_widget(Clear, modal);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Line::from(" Keybindings "));
        let inner = block.inner(modal);
        frame.render_widget(block, modal);

        let sections = match self.help_surface() {
            Some(surface) => vec![(bindings::title(surface), bindings::table(surface))],
            None => bindings::registry(),
        };
        let mut lines: Vec<Line> = Vec::new();
        for (title, table) in sections {
            lines.push(Line::from(Span::styled(
                title.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!("  {}", bindings::hint(table))));
        }

        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.help.unwrap_or(0), 0)),
            inner,
        );
    }

    /// One-line header text: app name · entry count.
    pub fn header_text(&self) -> String {
        let entry_count = self.main_view.visible_entries().len();
        let noun = if entry_count == 1 { "entry" } else { "entries" };
        format!("bogita  ·  {entry_count} {noun}")
    }
}
