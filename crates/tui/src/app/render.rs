//! Frame rendering — layout, view dispatch, and status chrome.

use super::modals::{
    centered_rect, render_confirm_delete_modal, render_confirm_delete_vault_modal,
    render_confirm_discard_modal, render_confirm_save_modal, render_error_modal,
    render_form_vault_picker, render_lock_screen,
};
use super::tui::{ActiveView, MessageKind, Tui};
use crate::views::entry_form::SlotKind;
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Paragraph};
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
                match f.focused_slot() {
                    SlotKind::Name => {
                        format!("Editing name  ·  [Tab] next field  [Enter] save  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    SlotKind::Key(_) => {
                        let n = (f.focused_field().saturating_sub(1)) / 3 + 1;
                        let m = f.field_count();
                        format!("Editing key (field {n} of {m})  ·  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    SlotKind::Value(_) if f.focused_field_is_obscured() => {
                        format!("Editing token  ·  [g] generate  [Ctrl-r] toggle reveal  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}")
                    }
                    SlotKind::Value(_) => format!("Editing value  ·  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}"),
                    SlotKind::Type(_) => {
                        let badge = f.field_badge((f.focused_field().saturating_sub(1)) / 3);
                        format!(
                            "[j/k] select type  ·  currently: {badge}  ·  [Enter] save  [Tab] next  [Esc] cancel{dirty_suffix}{validation_warning}"
                        )
                    }
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
}
