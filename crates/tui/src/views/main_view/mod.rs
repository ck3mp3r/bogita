//! Main 2-column view: entry list | entry detail.
//!
//! Column layout (approximate widths):
//!   [Col 1: ~35%] Entry list — filtered by selected vault
//!   [Col 2: ~65%] Entry detail — fields rendered type-aware; live TOTP for OTP entries
//!
//! Navigation:
//!   Tab / Shift+Tab — move focus right / left between columns
//!   j / k / Down / Up — scroll within the focused column
//!   s — toggle reveal on the selected hidden field (Col 2 focused)

// region:    --- Modules

mod actions;
mod keys;
mod render;
mod state;

pub use actions::{Column, MainViewAction};
pub use state::MainView;

// endregion: --- Modules

// region:    --- Tests
#[cfg(test)]
#[path = "../../../test/views/main_view/entries_navigation.rs"]
mod main_view_entries_navigation_test;
#[cfg(test)]
#[path = "../../../test/views/main_view/reveal_actions.rs"]
mod main_view_reveal_actions_test;
#[cfg(test)]
#[path = "../../../test/views/main_view/search_leader_picker.rs"]
mod main_view_search_leader_picker_test;
#[cfg(test)]
#[path = "../../../test/views/main_view/support.rs"]
mod support;
// endregion: --- Tests
