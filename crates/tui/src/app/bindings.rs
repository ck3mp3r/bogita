//! Key-binding registry — the single source of truth for footer hints and the
//! `?` help overlay.
//!
//! One table per focusable surface. A key that is not in the table does not
//! work on that surface, and every key in the table is advertised. Hand-editing
//! a rendered hint string is a defect: change the table instead.

use crate::views::entry_form::SlotKind;

/// One advertised key and what it does.
pub(super) struct Binding {
    pub(super) key: &'static str,
    pub(super) action: &'static str,
}

/// Shorthand for table rows.
const fn b(key: &'static str, action: &'static str) -> Binding {
    Binding { key, action }
}

/// Every surface that advertises keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Surface {
    MainEntries,
    MainDetail,
    MainLeaderEntries,
    MainLeaderDetail,
    MainVaultPicker,
    Search,
    FormName,
    FormKey,
    FormToken,
    FormValue,
    FormType,
    ConfirmSave,
    ConfirmDiscard,
    ConfirmDelete,
    VaultFormName,
    VaultFormCheckbox,
    Locked,
    LockedError,
    FormVaultPicker,
    PasswordGen,
    ErrorOverlay,
    Help,
}

// region:    --- Tables

pub(super) const MAIN_ENTRIES: &[Binding] = &[
    b("/", "search"),
    b("v", "vault"),
    b("Space", "actions"),
    b("j/k", "scroll"),
    b("Tab", "focus"),
    b("?", "help"),
    b("q", "quit"),
];

pub(super) const MAIN_DETAIL: &[Binding] = &[
    b("j/k", "scroll"),
    b("s", "reveal"),
    b("c", "copy"),
    b("Tab", "columns"),
    b("v", "vault"),
    b("Space", "actions"),
    b("?", "help"),
    b("q", "quit"),
];

pub(super) const MAIN_LEADER_ENTRIES: &[Binding] = &[
    b("a", "add"),
    b("e", "edit"),
    b("d", "delete"),
    b("Esc", "cancel"),
];

pub(super) const MAIN_LEADER_DETAIL: &[Binding] = &[b("Esc", "cancel")];

pub(super) const MAIN_VAULT_PICKER: &[Binding] = &[
    b("j/k", "select"),
    b("a", "add vault"),
    b("d", "delete vault"),
    b("Enter", "confirm"),
    b("Esc", "close"),
];

pub(super) const SEARCH: &[Binding] = &[
    b("Esc", "close"),
    b("Enter", "commit"),
    b("Backspace", "delete"),
];

pub(super) const FORM_NAME: &[Binding] = &[
    b("Tab", "next field"),
    b("Enter", "save"),
    b("Alt-n", "new field"),
    b("Esc", "cancel"),
];

pub(super) const FORM_KEY: &[Binding] = &[
    b("Enter", "save"),
    b("Tab", "next"),
    b("Alt-n", "new field"),
    b("Alt-d", "delete field"),
    b("Esc", "cancel"),
];

pub(super) const FORM_TOKEN: &[Binding] = &[
    b("Enter", "save"),
    b("Alt-g", "generate"),
    b("Alt-r", "toggle reveal"),
    b("Alt-n", "new field"),
    b("Alt-d", "delete field"),
    b("Tab", "next"),
    b("Esc", "cancel"),
];

pub(super) const FORM_VALUE: &[Binding] = &[
    b("Enter", "save"),
    b("Alt-n", "new field"),
    b("Alt-d", "delete field"),
    b("Tab", "next"),
    b("Esc", "cancel"),
];

/// The type slot reads `select type` first, then the badge, then the rest.
pub(super) const FORM_TYPE_HEAD: &[Binding] = &[b("j/k", "select type")];

pub(super) const FORM_TYPE_TAIL: &[Binding] = &[
    b("Alt-s", "save"),
    b("Enter", "open"),
    b("Alt-n", "new field"),
    b("Alt-d", "delete field"),
    b("Tab", "next"),
    b("Esc", "cancel"),
];

pub(super) const CONFIRM_SAVE: &[Binding] = &[
    b("s/Enter", "save"),
    b("d", "discard"),
    b("b/Esc", "back to form"),
];

pub(super) const CONFIRM_DISCARD: &[Binding] = &[b("d", "discard"), b("b/Enter", "back to form")];

pub(super) const CONFIRM_DELETE: &[Binding] = &[b("d", "delete"), b("c/Esc", "cancel")];

pub(super) const VAULT_FORM_NAME: &[Binding] = &[
    b("Tab", "toggle default"),
    b("Enter", "create"),
    b("Esc", "cancel"),
];

pub(super) const VAULT_FORM_CHECKBOX: &[Binding] = &[
    b("Alt-t", "toggle default"),
    b("Tab", "back to name"),
    b("Enter", "create"),
    b("Esc", "cancel"),
];

pub(super) const LOCKED: &[Binding] = &[
    b("Enter", "unlock"),
    b("Backspace", "delete"),
    b("Esc", "quit"),
];

pub(super) const FORM_VAULT_PICKER: &[Binding] = &[
    b("j/k", "select"),
    b("a", "add vault"),
    b("Enter", "confirm"),
    b("Esc", "close"),
];

pub(super) const PASSWORD_GEN: &[Binding] = &[
    b("g", "regenerate"),
    b("a", "accept"),
    b("Esc", "cancel"),
    b("+/-", "length"),
    b("u/l/d/s/x", "charset"),
];

pub(super) const ERROR_OVERLAY: &[Binding] = &[b("Esc / Enter", "dismiss")];

pub(super) const HELP: &[Binding] = &[b("j/k", "scroll"), b("?/Esc/q", "close help")];

// endregion: --- Tables

// region:    --- Lookup

/// The table for one surface.
pub(super) fn table(surface: Surface) -> &'static [Binding] {
    match surface {
        Surface::MainEntries => MAIN_ENTRIES,
        Surface::MainDetail => MAIN_DETAIL,
        Surface::MainLeaderEntries => MAIN_LEADER_ENTRIES,
        Surface::MainLeaderDetail => MAIN_LEADER_DETAIL,
        Surface::MainVaultPicker => MAIN_VAULT_PICKER,
        Surface::Search => SEARCH,
        Surface::FormName => FORM_NAME,
        Surface::FormKey => FORM_KEY,
        Surface::FormToken => FORM_TOKEN,
        Surface::FormValue => FORM_VALUE,
        Surface::FormType => FORM_TYPE_TAIL,
        Surface::ConfirmSave => CONFIRM_SAVE,
        Surface::ConfirmDiscard => CONFIRM_DISCARD,
        Surface::ConfirmDelete => CONFIRM_DELETE,
        Surface::VaultFormName => VAULT_FORM_NAME,
        Surface::VaultFormCheckbox => VAULT_FORM_CHECKBOX,
        Surface::Locked => LOCKED,
        Surface::LockedError => LOCKED,
        Surface::FormVaultPicker => FORM_VAULT_PICKER,
        Surface::PasswordGen => PASSWORD_GEN,
        Surface::ErrorOverlay => ERROR_OVERLAY,
        Surface::Help => HELP,
    }
}

/// The entry-form surface that matches a focused slot.
pub(super) fn form_surface(slot: &SlotKind, obscured: bool) -> Surface {
    match slot {
        SlotKind::Name => Surface::FormName,
        SlotKind::Key(_) => Surface::FormKey,
        SlotKind::Value(_) if obscured => Surface::FormToken,
        SlotKind::Value(_) => Surface::FormValue,
        SlotKind::Type(_) => Surface::FormType,
    }
}

/// Join a table into the footer format: `[key] action` pairs, two spaces apart.
pub(super) fn hint(table: &[Binding]) -> String {
    table
        .iter()
        .map(|binding| format!("[{}] {}", binding.key, binding.action))
        .collect::<Vec<_>>()
        .join("  ")
}

/// A titled hint: `title  ·  [key] action ...`.
pub(super) fn titled(title: &str, table: &[Binding]) -> String {
    format!("{title}  ·  {}", hint(table))
}

/// The plain footer hint for one surface (tables without a dynamic title).
pub(super) fn surface_hint(surface: Surface) -> String {
    hint(table(surface))
}

/// The footer hint for an entry-form slot: dynamic title, then the table.
pub(super) fn form_hint(
    slot: &SlotKind,
    focused_field: usize,
    field_count: usize,
    badge: &str,
    obscured: bool,
) -> String {
    match slot {
        SlotKind::Name => titled("Editing name", FORM_NAME),
        SlotKind::Key(_) => {
            let n = (focused_field.saturating_sub(1)) / 3 + 1;
            titled(
                &format!("Editing key (field {n} of {field_count})"),
                FORM_KEY,
            )
        }
        SlotKind::Value(_) if obscured => titled("Editing token", FORM_TOKEN),
        SlotKind::Value(_) => titled("Editing value", FORM_VALUE),
        SlotKind::Type(_) => format!(
            "{}  ·  currently: {badge}  ·  {}",
            hint(FORM_TYPE_HEAD),
            hint(FORM_TYPE_TAIL)
        ),
    }
}

/// The lock-screen hint: prompt plus the table.
pub(super) fn locked_hint(has_error: bool) -> String {
    if has_error {
        titled("Wrong passphrase. Try again.", LOCKED)
    } else {
        titled("Enter passphrase", LOCKED)
    }
}

/// The vault-form hint: the name cell and the default checkbox cell differ.
pub(super) fn vault_form_hint(name_focused: bool) -> String {
    if name_focused {
        titled("Editing vault name", VAULT_FORM_NAME)
    } else {
        hint(VAULT_FORM_CHECKBOX)
    }
}

/// The heading shown for a surface in the help overlay.
pub(super) fn title(surface: Surface) -> &'static str {
    match surface {
        Surface::MainEntries => "Main · entries",
        Surface::MainDetail => "Main · detail",
        Surface::MainLeaderEntries => "Main · leader (entries)",
        Surface::MainLeaderDetail => "Main · leader (detail)",
        Surface::MainVaultPicker => "Main · vault picker",
        Surface::Search => "Search",
        Surface::FormName => "Form · name",
        Surface::FormKey => "Form · key",
        Surface::FormToken => "Form · token",
        Surface::FormValue => "Form · value",
        Surface::FormType => "Form · type",
        Surface::ConfirmSave => "Save changes modal",
        Surface::ConfirmDiscard => "Discard changes modal",
        Surface::ConfirmDelete => "Delete modals",
        Surface::VaultFormName | Surface::VaultFormCheckbox => "Vault form",
        Surface::Locked | Surface::LockedError => "Lock screen",
        Surface::FormVaultPicker => "Vault picker (form)",
        Surface::PasswordGen => "Password generator",
        Surface::ErrorOverlay => "Error overlay",
        Surface::Help => "Help",
    }
}

/// Every section of the registry, in menu order — the help overlay content.
pub(super) fn registry() -> Vec<(&'static str, &'static [Binding])> {
    vec![
        ("Main · entries", MAIN_ENTRIES),
        ("Main · detail", MAIN_DETAIL),
        ("Main · leader (entries)", MAIN_LEADER_ENTRIES),
        ("Main · leader (detail)", MAIN_LEADER_DETAIL),
        ("Main · vault picker", MAIN_VAULT_PICKER),
        ("Search", SEARCH),
        ("Form · name", FORM_NAME),
        ("Form · key", FORM_KEY),
        ("Form · value", FORM_VALUE),
        ("Form · token", FORM_TOKEN),
        ("Form · type", FORM_TYPE_TAIL),
        ("Save changes modal", CONFIRM_SAVE),
        ("Discard changes modal", CONFIRM_DISCARD),
        ("Delete modals", CONFIRM_DELETE),
        ("Vault form", VAULT_FORM_NAME),
        ("Vault form · default cell", VAULT_FORM_CHECKBOX),
        ("Lock screen", LOCKED),
        ("Vault picker (form)", FORM_VAULT_PICKER),
        ("Password generator", PASSWORD_GEN),
    ]
}

// endregion: --- Lookup
