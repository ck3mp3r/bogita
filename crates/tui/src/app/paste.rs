//! Bracketed-paste routing — one atomic payload, one policy per surface.

use super::tui::{ActiveView, Tui};
use crate::views::paste_to_single_line;
use bogita_core::ports::{IdentityStore, KeychainStore, VaultStorageFactory};

impl<K: KeychainStore, F: VaultStorageFactory, I: IdentityStore> Tui<K, F, I> {
    /// Route one bracketed-paste payload to the focused surface.
    ///
    /// The payload arrives atomically, so no key event can fire mid-paste. The
    /// form decides per slot whether line structure survives; every other
    /// surface is single-line and gets the flattened text.
    pub(crate) fn handle_paste(&mut self, payload: &str) {
        match &mut self.active {
            ActiveView::Form(form) => form.handle_paste(payload),
            ActiveView::Main => {
                let text = paste_to_single_line(payload);
                self.main_view.handle_paste(&text);
            }
            ActiveView::VaultForm(form) => {
                let text = paste_to_single_line(payload);
                form.handle_paste(&text);
            }
            ActiveView::Locked {
                passphrase_input, ..
            } => {
                let text = paste_to_single_line(payload);
                passphrase_input.set_text(format!("{}{}", passphrase_input.text(), text));
            }
            _ => {}
        }
    }
}
