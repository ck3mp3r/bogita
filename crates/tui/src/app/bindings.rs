//! Key-binding registry — the single source of truth for footer hints and the
//! `?` help overlay.
//!
//! One table per focusable surface. A key that is not in the table does not
//! work on that surface, and every key in the table is advertised. Hand-editing
//! a rendered hint string is a defect: change the table instead.

use crate::views::entry_form::SlotKind;

/// One advertised key, the short label a hint shows, and the sentence the help
/// overlay renders after it.
pub(crate) struct Binding {
    pub(super) key: &'static str,
    pub(super) action: &'static str,
    pub(super) desc: &'static str,
}

/// Shorthand for table rows.
const fn b(key: &'static str, action: &'static str, desc: &'static str) -> Binding {
    Binding { key, action, desc }
}

/// The field-management and slot chords, spelled once so every surface that
/// offers them reads the same. `Alt-x` cuts the focused slot; `Alt-s` saves from
/// any slot, `Alt-v` opens the vault picker from any slot.
const ADD_FIELD: Binding = b("Alt-n", "add field", "Add a field row below");
const DELETE_FIELD: Binding = b("Alt-d", "delete field", "Delete the focused field row");
const CUT_SLOT: Binding = b("Alt-x", "cut slot", "Cut the slot to the input buffer");
const SAVE_FORM: Binding = b("Alt-s", "save", "Save the entry from any slot");
const PICK_VAULT: Binding = b("Alt-v", "vault", "Pick the entry's vault");
/// Copying a field is a yank: `c` is Ctrl-c's letter in every input, so the bare
/// chord belongs to the editor grammar, not to us.
const COPY_KEY: Binding = b("y", "copy", "Copy the focused field value");

/// Tab is pane navigation in the main view: it names the column it reaches.
const TAB_TO_DETAIL: Binding = b("Tab", "focus detail", "Move focus to the detail pane");
const TAB_TO_COLUMNS: Binding = b("Tab", "focus columns", "Move focus to the columns");

/// `/` opens the search box from either main-view column (`main_view/keys.rs`
/// does not gate it by focus), so both footers advertise it.
const SEARCH_KEY: Binding = b("/", "search", "Search the entry list");

/// Every surface that advertises keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Surface {
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
}

// region:    --- Tables

pub(super) const MAIN_ENTRIES: &[Binding] = &[
    SEARCH_KEY,
    b("v", "vault", "Pick the vault to list"),
    b("Space", "actions", "Open the action menu"),
    b("j/k", "scroll", "Move the selection one row"),
    b("h/l", "panes", "Move focus between panes"),
    TAB_TO_DETAIL,
    b("gg", "top", "Jump to the first entry"),
    b("G", "bottom", "Jump to the last entry"),
    b("Ctrl-d/u", "half page", "Scroll half a page"),
    b("?", "help", "Show these keybindings"),
    b("q", "quit", "Quit and lock the vault"),
];

pub(super) const MAIN_DETAIL: &[Binding] = &[
    b("j/k", "scroll", "Move between field rows"),
    b("gg", "top", "Jump to the first field"),
    b("G", "bottom", "Jump to the last field"),
    b("Ctrl-d/u", "half page", "Scroll half a page"),
    b("s", "reveal", "Reveal or hide the value"),
    COPY_KEY,
    SEARCH_KEY,
    b("h/l", "panes", "Move focus between panes"),
    TAB_TO_COLUMNS,
    b("v", "vault", "Pick the vault to list"),
    b("Space", "actions", "Open the action menu"),
    b("?", "help", "Show these keybindings"),
    b("q", "quit", "Quit and lock the vault"),
];

pub(super) const MAIN_LEADER_ENTRIES: &[Binding] = &[
    b("a", "add", "Create a new entry"),
    b("e", "edit", "Edit the selected entry"),
    b("d", "delete", "Delete the selected entry"),
    b("Esc", "cancel", "Close the action menu"),
];

pub(super) const MAIN_LEADER_DETAIL: &[Binding] = &[b("Esc", "cancel", "Close the action menu")];

pub(super) const MAIN_VAULT_PICKER: &[Binding] = &[
    b("j/k", "select", "Move the vault selection"),
    b("a", "add vault", "Create a new vault"),
    b("d", "delete vault", "Delete the selected vault"),
    b("Enter", "confirm", "Filter by the selection"),
    b("Esc", "close", "Close without changing"),
];

pub(super) const SEARCH: &[Binding] = &[
    b("Esc", "close", "Close and clear the search"),
    b("Enter", "commit", "Keep the filter and close"),
    b("Backspace", "delete", "Delete the last character"),
];

pub(super) const FORM_NAME: &[Binding] = &[
    b("Tab", "next field", "Move to the next field"),
    b("Enter", "save", "Save the entry"),
    SAVE_FORM,
    ADD_FIELD,
    DELETE_FIELD,
    CUT_SLOT,
    PICK_VAULT,
    b("Esc", "cancel", "Cancel the edit"),
];

pub(super) const FORM_KEY: &[Binding] = &[
    b("Enter", "save", "Save the entry"),
    SAVE_FORM,
    b("Tab", "next", "Move to the next field"),
    ADD_FIELD,
    DELETE_FIELD,
    CUT_SLOT,
    PICK_VAULT,
    b("Esc", "cancel", "Cancel the edit"),
];

pub(super) const FORM_TOKEN: &[Binding] = &[
    b("Enter", "save", "Save the entry"),
    SAVE_FORM,
    b("Alt-g", "generate", "Generate a password"),
    b("Alt-r", "toggle reveal", "Reveal or hide the value"),
    ADD_FIELD,
    DELETE_FIELD,
    CUT_SLOT,
    PICK_VAULT,
    b("Tab", "next", "Move to the next field"),
    b("Esc", "cancel", "Cancel the edit"),
];

pub(super) const FORM_VALUE: &[Binding] = &[
    b("Enter", "save", "Save the entry"),
    SAVE_FORM,
    ADD_FIELD,
    DELETE_FIELD,
    CUT_SLOT,
    PICK_VAULT,
    b("Tab", "next", "Move to the next field"),
    b("Esc", "cancel", "Cancel the edit"),
];

/// The type slot: the selector itself, then the rest of the slot chords.
pub(super) const FORM_TYPE_TAIL: &[Binding] = &[
    b("j/k", "select type", "Change the field type"),
    SAVE_FORM,
    b("Enter", "open", "Open the type list"),
    ADD_FIELD,
    DELETE_FIELD,
    PICK_VAULT,
    b("Tab", "next", "Move to the next field"),
    b("Esc", "cancel", "Cancel the edit"),
];

/// The form's vault row: `v` alone types into the focused input, so the chord
/// that opens the picker is `Alt-v`.
pub(super) const FORM_VAULT_CELL: &[Binding] =
    &[b("Alt-v", "change vault", "Change the entry's vault")];

/// The one row the status bar shows when nothing transient is active. The full
/// inventory lives in the `?` overlay; the footer only advertises how to reach it.
pub(crate) const FOOTER: &[Binding] = &[b("?", "help", "Show these keybindings")];

/// Chords that work on every surface. They are inventory, not advertisement:
/// the overlay lists them so no route is undiscoverable.
pub(super) const GLOBAL: &[Binding] = &[
    b("?", "help", "Show these keybindings"),
    b("q", "quit", "Quit and lock the vault"),
    b("Ctrl-c", "quit", "Quit and lock the vault"),
    b("Ctrl-l", "lock", "Lock the vault now"),
];

pub(super) const CONFIRM_SAVE: &[Binding] = &[
    b("s/Enter", "save", "Save the changes"),
    b("d", "discard", "Discard the changes"),
    b("b/Esc", "back to form", "Return to the form"),
];

pub(super) const CONFIRM_DISCARD: &[Binding] = &[
    b("d", "discard", "Discard and close the form"),
    b("b/Enter", "back to form", "Return to the form"),
];

pub(super) const CONFIRM_DELETE: &[Binding] = &[
    b("d", "delete", "Delete the entry"),
    b("c/Esc", "cancel", "Cancel without deleting"),
];

pub(super) const VAULT_FORM_NAME: &[Binding] = &[
    b("Tab", "toggle default", "Move to the default toggle"),
    b("Enter", "create", "Create the vault"),
    b("Esc", "cancel", "Cancel the new vault"),
];

pub(super) const VAULT_FORM_CHECKBOX: &[Binding] = &[
    b("Alt-t", "toggle default", "Toggle the default flag"),
    b("Tab", "back to name", "Move back to the name"),
    b("Enter", "create", "Create the vault"),
    b("Esc", "cancel", "Cancel the new vault"),
];

pub(super) const LOCKED: &[Binding] = &[
    b("Enter", "unlock", "Unlock with the passphrase"),
    b("Backspace", "delete", "Delete the last character"),
    b("Esc", "quit", "Quit the application"),
];

pub(super) const FORM_VAULT_PICKER: &[Binding] = &[
    b("j/k", "select", "Move the vault selection"),
    b("a", "add vault", "Create a new vault"),
    b("Enter", "confirm", "Choose the vault"),
    b("Esc", "close", "Keep the current vault"),
];

pub(super) const PASSWORD_GEN: &[Binding] = &[
    b("g", "regenerate", "Generate another password"),
    b("a", "accept", "Use this password"),
    b("Esc", "cancel", "Discard and go back"),
    b("+/-", "length", "Change the length"),
    b("u/l/d/s/x", "charset", "Toggle a character set"),
];

/// The generator's per-toggle rows — one chord per charset switch, so the view
/// can render the key and the state without writing the chord itself.
pub(super) const PASSWORD_GEN_TOGGLES: &[Binding] = &[
    b("u", "upper", "Allow uppercase letters"),
    b("l", "lower", "Allow lowercase letters"),
    b("d", "digits", "Allow digits"),
    b("s", "symbols", "Allow symbols"),
    b("x", "no-ambiguous", "Avoid ambiguous characters"),
];

pub(super) const ERROR_OVERLAY: &[Binding] = &[b("Esc / Enter", "dismiss", "Dismiss the error")];

pub(super) const HELP: &[Binding] = &[
    b("j/k", "scroll", "Scroll these keybindings"),
    b("?/Esc/q", "close help", "Close the keybindings"),
];

// endregion: --- Tables

// region:    --- Sections

/// One rendered section of the help overlay: its heading and the table it
/// paints. The overlay iterates this list, so a table missing here is a table
/// no one can discover.
pub(crate) struct Section {
    pub(super) label: &'static str,
    pub(super) table: &'static [Binding],
}

const fn s(label: &'static str, table: &'static [Binding]) -> Section {
    Section { label, table }
}

/// Every section the overlay renders, in registry order. `Everywhere` leads;
/// the focused surface is hoisted above it at render time.
pub(crate) const SECTIONS: &[Section] = &[
    s("Everywhere", GLOBAL),
    s("Main · entries", MAIN_ENTRIES),
    s("Main · detail", MAIN_DETAIL),
    s("Main · action menu (entries)", MAIN_LEADER_ENTRIES),
    s("Main · action menu (detail)", MAIN_LEADER_DETAIL),
    s("Main · vault picker", MAIN_VAULT_PICKER),
    s("Search", SEARCH),
    s("Form · name", FORM_NAME),
    s("Form · key", FORM_KEY),
    s("Form · token", FORM_TOKEN),
    s("Form · value", FORM_VALUE),
    s("Form · type", FORM_TYPE_TAIL),
    s("Form · vault cell", FORM_VAULT_CELL),
    s("Save changes modal", CONFIRM_SAVE),
    s("Discard changes modal", CONFIRM_DISCARD),
    s("Delete modal", CONFIRM_DELETE),
    s("Vault form · name", VAULT_FORM_NAME),
    s("Vault form · default toggle", VAULT_FORM_CHECKBOX),
    s("Lock screen", LOCKED),
    s("Vault picker (form)", FORM_VAULT_PICKER),
    s("Password generator", PASSWORD_GEN),
    s("Generator toggles", PASSWORD_GEN_TOGGLES),
    s("Error overlay", ERROR_OVERLAY),
    s("Keybindings overlay", HELP),
];

/// A section heading; the focused section is marked so the eye lands on it.
pub(super) fn section_title(section: &Section, focused: bool) -> String {
    if focused {
        format!("▸ {} (focused)", section.label)
    } else {
        section.label.to_string()
    }
}

/// The section that owns a surface's keys. Surfaces that share a table (the
/// lock screen's two states) share the section.
pub(super) fn section_label(surface: Surface) -> &'static str {
    match surface {
        Surface::MainEntries => "Main · entries",
        Surface::MainDetail => "Main · detail",
        Surface::MainLeaderEntries => "Main · action menu (entries)",
        Surface::MainLeaderDetail => "Main · action menu (detail)",
        Surface::MainVaultPicker => "Main · vault picker",
        Surface::Search => "Search",
        Surface::FormName => "Form · name",
        Surface::FormKey => "Form · key",
        Surface::FormToken => "Form · token",
        Surface::FormValue => "Form · value",
        Surface::FormType => "Form · type",
        Surface::ConfirmSave => "Save changes modal",
        Surface::ConfirmDiscard => "Discard changes modal",
        Surface::ConfirmDelete => "Delete modal",
        Surface::VaultFormName => "Vault form · name",
        Surface::VaultFormCheckbox => "Vault form · default toggle",
        Surface::Locked | Surface::LockedError => "Lock screen",
        Surface::FormVaultPicker => "Vault picker (form)",
        Surface::PasswordGen => "Password generator",
        Surface::ErrorOverlay => "Error overlay",
    }
}

/// One row of the help overlay: a binding plus the section it belongs to.
pub(crate) struct HelpRow {
    pub(super) section: String,
    pub(super) key: &'static str,
    pub(super) action: &'static str,
    pub(super) desc: &'static str,
    pub(super) focused: bool,
}

/// Every registry row for the overlay: the focused surface's section first,
/// then `Everywhere`, then the rest.
///
pub(crate) fn help_rows(focused: Surface) -> Vec<HelpRow> {
    let focused_label = section_label(focused);
    let mut ordered: Vec<&Section> = Vec::with_capacity(SECTIONS.len());
    ordered.extend(
        SECTIONS
            .iter()
            .filter(|section| section.label == focused_label),
    );
    ordered.extend(
        SECTIONS
            .iter()
            .filter(|section| section.label != focused_label),
    );

    ordered
        .into_iter()
        .flat_map(|section| {
            let is_focused = section.label == focused_label;
            let heading = section_title(section, is_focused);
            section.table.iter().map(move |binding| HelpRow {
                section: heading.clone(),
                key: binding.key,
                action: binding.action,
                desc: binding.desc,
                focused: is_focused,
            })
        })
        .collect()
}

// endregion: --- Sections

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

/// One row rendered as `[key] action` — the only place the format lives.
pub(super) fn label(binding: &Binding) -> String {
    format!("[{}] {}", binding.key, binding.action)
}

/// The footer: one constant row, the way into the full inventory.
pub(crate) fn footer() -> String {
    hint(FOOTER)
}

/// Join a table into the `[key] action` format, two spaces apart.
pub(crate) fn hint(table: &[Binding]) -> String {
    table.iter().map(label).collect::<Vec<_>>().join("  ")
}

/// The leader menu for a column: one `(chord, action)` row per binding.
///
/// The menu is the registry painted as a list — the footer and the overlay show
/// the same rows because they read the same table.
pub(crate) fn leader_rows(detail_focused: bool) -> Vec<(String, &'static str)> {
    let table = if detail_focused {
        MAIN_LEADER_DETAIL
    } else {
        MAIN_LEADER_ENTRIES
    };
    table
        .iter()
        .map(|binding| (format!("[{}]", binding.key), binding.action))
        .collect()
}

/// The entry form's vault row: the chord that opens the vault picker.
pub(crate) fn form_vault_hint() -> String {
    hint(FORM_VAULT_CELL)
}

/// One charset toggle rendered as `[key] action`, by table position.
pub(crate) fn password_gen_toggle(index: usize) -> String {
    PASSWORD_GEN_TOGGLES
        .get(index)
        .map(label)
        .unwrap_or_default()
}

// endregion: --- Lookup
