//! Tui application — event loop, key handling, deferred actions, and rendering.

// region:    --- Modules

mod actions;
mod bindings;
mod keys;
mod modals;
mod paste;
mod render;
pub mod signals;
mod tui;

pub use actions::PendingCopy;
pub use modals::centered_rect;
pub use tui::{MessageKind, RunState, Tui};

#[cfg(all(test, feature = "integration"))]
use modals::wrap_text;

// endregion: --- Modules

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/confirm_enter.rs"]
mod app_confirm_enter_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/deep_links.rs"]
mod app_deep_links_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/entry_form_roundtrip.rs"]
mod app_entry_form_roundtrip_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/exit_lock.rs"]
mod app_exit_lock_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/form_esc_save.rs"]
mod app_form_esc_save_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/form_password_gen.rs"]
mod app_form_password_gen_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/form_save_chord.rs"]
mod app_form_save_chord_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/hints_drift.rs"]
mod app_hints_drift_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/lock_screen_input.rs"]
mod app_lock_screen_input_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/main_and_navigation.rs"]
mod app_main_and_navigation_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/overlays_and_lock.rs"]
mod app_overlays_and_lock_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/paste.rs"]
mod app_paste_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/rendering.rs"]
mod app_rendering_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/signals.rs"]
mod app_signals_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/validation.rs"]
mod app_validation_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/support.rs"]
mod support;
// endregion: --- Tests
