//! Vault creation form view.

use bogita_core::domain::Vault;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use uuid::Uuid;

/// Action returned by [`VaultForm::handle_key`].
pub enum VaultFormAction {
    /// User confirmed the form — create the vault.
    Confirm(Vault),
    /// User cancelled.
    Cancel,
    /// No action.
    None,
}

/// A simple form for creating a new vault.
pub struct VaultForm {
    name: String,
    name_focused: bool,
    is_default: bool,
}

impl VaultForm {
    pub fn new(name: String) -> Self {
        Self {
            name,
            name_focused: true,
            is_default: false,
        }
    }

    pub fn is_name_focused(&self) -> bool {
        self.name_focused
    }

    pub fn handle_key(&mut self, key: KeyCode) -> VaultFormAction {
        if self.name_focused {
            match key {
                KeyCode::Enter => self.confirm(),
                KeyCode::Esc => VaultFormAction::Cancel,
                KeyCode::Tab => {
                    self.name_focused = false;
                    VaultFormAction::None
                }
                KeyCode::Backspace => {
                    self.name.pop();
                    VaultFormAction::None
                }
                KeyCode::Char(c) => {
                    self.name.push(c);
                    VaultFormAction::None
                }
                _ => VaultFormAction::None,
            }
        } else {
            match key {
                KeyCode::Enter => self.confirm(),
                KeyCode::Esc => VaultFormAction::Cancel,
                KeyCode::Tab => {
                    self.name_focused = true;
                    VaultFormAction::None
                }
                KeyCode::Char(' ') => {
                    self.is_default = !self.is_default;
                    VaultFormAction::None
                }
                _ => VaultFormAction::None,
            }
        }
    }

    fn confirm(&self) -> VaultFormAction {
        if self.name.is_empty() {
            return VaultFormAction::None;
        }
        let vault = Vault {
            id: Uuid::new_v4(),
            name: self.name.clone(),
            is_default: self.is_default,
            created_at: chrono::Utc::now().timestamp(),
            sync_target: None,
            recipients: vec![], // filled by Tui from identity
            lock_timeout: None,
            auto_sync: false,
        };
        VaultFormAction::Confirm(vault)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // Clear area first
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" New Vault ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Name input
        let name_style = if self.name_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let name_line =
            Line::from(vec!["Name: ".into(), self.name.as_str().into()]).style(name_style);
        frame.render_widget(
            Paragraph::new(name_line).alignment(Alignment::Left),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // Default toggle
        let default_style = if !self.name_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let marker = if self.is_default { "[x]" } else { "[ ]" };
        let default_line =
            Line::from(vec![format!("{} Default vault", marker).into()]).style(default_style);
        frame.render_widget(
            Paragraph::new(default_line).alignment(Alignment::Left),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
    }
}
