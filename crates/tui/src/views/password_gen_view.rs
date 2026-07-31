//! Password generator popup view.
//!
//! Triggered by `[g]` when focused on a value slot in the entry form.
//! Displays the generated password, entropy estimate, and charset controls.
//!
//! ## Keybindings
//!   `[g]`         regenerate
//!   `[a]`         accept — return generated password to caller
//!   `[Esc]`       cancel
//!   `[+] / [-]`   increase / decrease length (1–128)
//!   `[u]`         toggle uppercase
//!   `[l]`         toggle lowercase
//!   `[d]`         toggle digits
//!   `[s]`         toggle symbols
//!   `[x]`         toggle avoid-ambiguous

use bogita_core::service::password_gen::{CharsetOptions, PasswordGen};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use secrecy::ExposeSecret;

// ── PasswordGenAction ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordGenAction {
    None,
    /// Accept the generated password (plain string — caller puts it in the form slot).
    Accept(String),
    Cancel,
}

// ── PasswordGenView ───────────────────────────────────────────────────────────

pub struct PasswordGenView {
    length: usize,
    opts: CharsetOptions,
    /// The currently displayed generated password (plain text, held only in memory).
    current: String,
}

impl Default for PasswordGenView {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordGenView {
    /// Create a new view with default options, generating an initial password.
    pub fn new() -> Self {
        let length = 20;
        let opts = CharsetOptions::default();
        let current = generate(&opts, length);
        Self {
            length,
            opts,
            current,
        }
    }

    // ── accessors ─────────────────────────────────────────────────────────────

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn charset_options(&self) -> &CharsetOptions {
        &self.opts
    }

    pub fn current_password(&self) -> &str {
        &self.current
    }

    pub fn entropy_bits(&self) -> f64 {
        PasswordGen::new(self.length, self.opts.clone()).entropy_bits()
    }

    // ── key handling ──────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, key: KeyCode) -> PasswordGenAction {
        match key {
            KeyCode::Esc => PasswordGenAction::Cancel,
            KeyCode::Char('a') => PasswordGenAction::Accept(self.current.clone()),
            KeyCode::Char('g') => {
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('+') => {
                self.length = (self.length + 1).min(128);
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('-') => {
                self.length = self.length.saturating_sub(1).max(1);
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('u') => {
                self.opts.uppercase = !self.opts.uppercase;
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('l') => {
                self.opts.lowercase = !self.opts.lowercase;
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('d') => {
                self.opts.digits = !self.opts.digits;
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('s') => {
                self.opts.symbols = !self.opts.symbols;
                self.regenerate();
                PasswordGenAction::None
            }
            KeyCode::Char('x') => {
                self.opts.avoid_ambiguous = !self.opts.avoid_ambiguous;
                self.regenerate();
                PasswordGenAction::None
            }
            _ => PasswordGenAction::None,
        }
    }

    // ── private ───────────────────────────────────────────────────────────────

    fn regenerate(&mut self) {
        self.current = generate(&self.opts, self.length);
    }

    // ── rendering ─────────────────────────────────────────────────────────────

    /// Render the popup centered over `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let w = 60u16.min(area.width);
        let h = 12u16.min(area.height);
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let popup = Rect::new(x, y, w, h);

        frame.render_widget(Clear, popup);

        let block = Block::default()
            .title(" Generate Password ")
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Green));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // generated password
                Constraint::Length(1), // entropy
                Constraint::Length(1), // spacer
                Constraint::Length(1), // length
                Constraint::Length(1), // charset toggles row 1
                Constraint::Length(1), // charset toggles row 2
                Constraint::Length(1), // spacer
                Constraint::Min(1),    // hint bar
            ])
            .split(inner);

        // Password display
        let masked: String = self.current.chars().map(|_| '•').collect();
        let _ = masked; // we show it plainly — it's temporary in memory
        frame.render_widget(
            Paragraph::new(self.current.as_str()).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            rows[0],
        );

        // Entropy
        let entropy = self.entropy_bits();
        let entropy_color = entropy_color(entropy);
        frame.render_widget(
            Paragraph::new(format!("Entropy: {entropy:.1} bits"))
                .style(Style::default().fg(entropy_color)),
            rows[1],
        );

        // Length
        frame.render_widget(
            Paragraph::new(format!("Length: {}  [+/-]", self.length))
                .style(Style::default().fg(Color::Cyan)),
            rows[3],
        );

        // Charset toggles row 1
        let o = &self.opts;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                toggle_span("[u] upper", o.uppercase),
                Span::raw("  "),
                toggle_span("[l] lower", o.lowercase),
                Span::raw("  "),
                toggle_span("[d] digits", o.digits),
            ])),
            rows[4],
        );

        // Charset toggles row 2
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                toggle_span("[s] symbols", o.symbols),
                Span::raw("  "),
                toggle_span("[x] no-ambiguous", o.avoid_ambiguous),
            ])),
            rows[5],
        );

        // Hint bar
        frame.render_widget(
            Paragraph::new("[g] regenerate  [a] accept  [Esc] cancel")
                .style(Style::default().fg(Color::DarkGray)),
            rows[7],
        );
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn generate(opts: &CharsetOptions, length: usize) -> String {
    let pw = PasswordGen::new(length, opts.clone()).generate();
    pw.expose_secret().to_string()
}

fn entropy_color(bits: f64) -> Color {
    if bits >= 80.0 {
        Color::Green
    } else if bits >= 50.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn toggle_span(label: &'static str, enabled: bool) -> Span<'static> {
    if enabled {
        Span::styled(label, Style::default().fg(Color::Green))
    } else {
        Span::styled(label, Style::default().fg(Color::DarkGray))
    }
}
