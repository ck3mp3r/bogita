//! Tui application — event loop, key handling, deferred actions, and rendering.

// region:    --- Modules

mod actions;
mod keys;
mod modals;
mod render;
mod tui;

pub use actions::PendingCopy;
pub use modals::centered_rect;
pub use tui::{MessageKind, RunState, Tui};

#[cfg(all(test, feature = "integration"))]
use modals::wrap_text;

// endregion: --- Modules

// region:    --- Tests
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/deep_links.rs"]
mod app_deep_links_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/main_and_navigation.rs"]
mod app_main_and_navigation_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/overlays_and_lock.rs"]
mod app_overlays_and_lock_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/rendering.rs"]
mod app_rendering_test;
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/app/support.rs"]
mod support;
// endregion: --- Tests
