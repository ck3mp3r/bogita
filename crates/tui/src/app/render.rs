//! Frame rendering — layout, view dispatch, and status chrome.

use super::bindings;
use super::help;
use super::modals::{
    centered_rect, form_modal_rect, render_confirm_delete_modal, render_confirm_delete_vault_modal,
    render_confirm_discard_modal, render_confirm_save_modal, render_error_modal,
    render_form_vault_picker, render_lock_screen,
};
use super::tui::{ActiveView, MessageKind, Tui};
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
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
                        // Render form as a content-sized overlay on top.
                        let overlay = form_modal_rect(body_area, f.field_count());
                        f.render(frame, overlay);
                    }
                    ActiveView::FormVaultPicker { form, vault_state } => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as a content-sized overlay.
                        let overlay = form_modal_rect(body_area, form.field_count());
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
                        // Render form as a content-sized overlay, then confirm modal on top.
                        let overlay = form_modal_rect(body_area, form.field_count());
                        form.render(frame, overlay);
                        render_confirm_save_modal(frame, body_area, &name);
                    }
                    ActiveView::ConfirmDiscard { form } => {
                        // Render main view detail pane behind the overlay.
                        self.main_view
                            .render_detail(frame, col3, self.detail_entry.as_ref());
                        // Render form as a content-sized overlay, then discard confirm modal on top.
                        let overlay = form_modal_rect(body_area, form.field_count());
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
                        // Render form as a content-sized overlay, then gen popup on top.
                        let overlay = form_modal_rect(body_area, form.field_count());
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

        if self.help.is_some() {
            self.render_help_overlay(frame, body_area);
        }

        // Error overlay: rendered last so an error always wins over the help
        // overlay — the caller must see why the action failed.
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

    /// The status bar's idle text — one bindings-sourced row, the same on every
    /// surface. The full key inventory lives behind `?`; advertising chords here
    /// is what made the bar drift.
    pub fn status_hint(&self) -> String {
        bindings::footer()
    }

    /// The surface that currently owns the keys — the one the `?` overlay
    /// highlights first. The overlay itself renders the whole registry, so this
    /// names the focus, not the extent, of the inventory.
    pub(crate) fn surface(&self) -> bindings::Surface {
        use bindings::Surface;
        if self.error_message.is_some() {
            return Surface::ErrorOverlay;
        }
        match &self.active {
            ActiveView::Main => {
                use crate::views::main_view::Column;
                if self.main_view.is_vault_picker_open() {
                    Surface::MainVaultPicker
                } else if self.main_view.is_leader_mode() {
                    match self.main_view.focused {
                        Column::Entries => Surface::MainLeaderEntries,
                        Column::Detail => Surface::MainLeaderDetail,
                    }
                } else if self.main_view.is_searching() {
                    Surface::Search
                } else if self.main_view.focused == Column::Detail {
                    Surface::MainDetail
                } else {
                    Surface::MainEntries
                }
            }
            ActiveView::ConfirmSave { .. } => Surface::ConfirmSave,
            ActiveView::ConfirmDiscard { .. } => Surface::ConfirmDiscard,
            ActiveView::ConfirmDelete { .. } | ActiveView::ConfirmDeleteVault { .. } => {
                Surface::ConfirmDelete
            }
            ActiveView::Form(f) => {
                bindings::form_surface(&f.focused_slot(), f.focused_field_is_obscured())
            }
            ActiveView::PasswordGen { .. } => Surface::PasswordGen,
            ActiveView::FormVaultPicker { .. } => Surface::FormVaultPicker,
            ActiveView::VaultForm(f) => {
                if f.is_name_focused() {
                    Surface::VaultFormName
                } else {
                    Surface::VaultFormCheckbox
                }
            }
            ActiveView::Locked { error, .. } => {
                if error.is_some() {
                    Surface::LockedError
                } else {
                    Surface::Locked
                }
            }
        }
    }

    /// The `?` keybindings overlay: every registry chord, described, in
    /// sections. `j`/`k` scroll; `?`/Esc/`q` close.
    fn render_help_overlay(&mut self, frame: &mut Frame, area: Rect) {
        let modal = centered_rect(80, 80, area);
        frame.render_widget(Clear, modal);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Line::from(" Keybindings "));
        let inner = block.inner(modal);
        frame.render_widget(block, modal);

        let rows = bindings::help_rows(self.surface());
        let content = help::layout(&rows, inner.width);
        let content_len = content.len();
        // The scroll bounds are state, not locals: the key handler owns the
        // grammar and needs the same two numbers this frame is clamped by.
        self.help_viewport = inner.height;
        self.help_max_scroll = help::max_scroll(content_len, inner.height);
        let scroll = self.help.unwrap_or(0).min(self.help_max_scroll);
        let lines: Vec<Line> = content
            .into_iter()
            .map(|line| match line {
                help::HelpLine::Header { title, focused } => {
                    let style = if focused {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    };
                    Line::from(Span::styled(title, style))
                }
                help::HelpLine::Row(text) => Line::from(text),
                help::HelpLine::Spacer => Line::from(""),
            })
            .collect();

        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);

        // A scrollbar only when there is something below the fold: it is the
        // position cue in a modal that has no other chrome for it.
        //
        // `content_length` counts scroll STATES (max offset + 1), not content
        // lines: the widget places the thumb at position / (content_length - 1)
        // and sizes it by viewport / (content_length - 1), so feeding raw lines
        // left the thumb short of the end by about one viewport. No arrow heads
        // and no inset: the track is the full inner height, so the thumb touches
        // both edges.
        let max_scroll = help::max_scroll(content_len, inner.height);
        if max_scroll > 0 {
            let mut bar_state = ScrollbarState::new(max_scroll as usize + 1)
                .position(scroll as usize)
                .viewport_content_length(inner.height as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None),
                inner,
                &mut bar_state,
            );
        }
    }

    /// One-line header text: app name · entry count.
    pub fn header_text(&self) -> String {
        let entry_count = self.main_view.visible_entries().len();
        let noun = if entry_count == 1 { "entry" } else { "entries" };
        format!("bogita  ·  {entry_count} {noun}")
    }
}
