//! TUI startup context — tells the TUI which view to open on launch.

/// Startup context passed to [`super::app::Tui`] to pre-focus the correct view.
#[derive(Debug, Clone)]
pub enum TuiContext {
    /// Default: open the main vault+entry list view.
    Default,
    /// Open the "add entry" form, optionally pre-filling the name.
    AddEntry { name: Option<String> },
    /// Open the "edit entry" form for the named entry.
    EditEntry { name: String, vault: Option<String> },
    /// Open the "delete entry" confirmation for the named entry.
    DeleteEntry { name: String, vault: Option<String> },
    /// Open the "add vault" form.
    AddVault { name: String },
}
