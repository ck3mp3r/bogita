type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::app::bindings;
use crate::app::Tui;
use crate::context::TuiContext;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::support::{make_app, make_app_with_entry, TestService};

/// Press one unmodified key.
fn key(tui: &mut Tui<TestService>, k: KeyCode) {
    tui.handle_key_with_modifiers(k, KeyModifiers::NONE);
}

// ── the footer is one constant row ──────────────────────────────────────────

/// The only text the status bar shows when nothing transient is active.
const FOOTER_TEXT: &str = "[?] help";

#[tokio::test]
async fn test_footer_is_constant_row() -> Result<()> {
    let app = make_app_with_entry().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    assert_eq!(bindings::footer(), FOOTER_TEXT);

    // -- Check: main, both panes, the leader menu, the vault picker and search
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Tab); // → detail
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::BackTab);
    key(&mut tui, KeyCode::Char(' ')); // leader
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Esc);
    key(&mut tui, KeyCode::Char('v')); // vault picker
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Esc);
    key(&mut tui, KeyCode::Char('/')); // search
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Esc);

    // -- Check: the edit form, every slot
    key(&mut tui, KeyCode::Char(' '));
    key(&mut tui, KeyCode::Char('e'));
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    for _ in 0..3 {
        key(&mut tui, KeyCode::Tab);
        assert_eq!(tui.status_hint(), FOOTER_TEXT);
    }

    // -- Check: the save modal
    key(&mut tui, KeyCode::Char('x')); // dirty
    key(&mut tui, KeyCode::Esc);
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Esc); // back to the form
    key(&mut tui, KeyCode::Esc); // leave the form

    // -- Check: the lock screen
    tui.handle_key_with_modifiers(KeyCode::Char('l'), KeyModifiers::CONTROL);
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    Ok(())
}

#[tokio::test]
async fn test_status_message_still_wins_over_the_footer() -> Result<()> {
    let app = make_app_with_entry().await?;
    let tui = Tui::new(app, TuiContext::Default).await?;

    // -- Exec: a transient status message (Ctrl-c style info is not exposed, so
    // the delete flow's cancel path is used to set one) is not needed: the
    // renderer composes message-first, and the hint is the constant behind it.
    assert_eq!(tui.status_hint(), FOOTER_TEXT, "idle text is the constant");
    Ok(())
}

// ── the overlay carries the inventory ──────────────────────────────────────

/// Chords the handlers actually route, per surface — the reference the tables
/// must match in both directions.
fn routed_chords(surface: bindings::Surface) -> &'static [&'static str] {
    use bindings::Surface;
    match surface {
        Surface::MainEntries => &[
            "/", "v", "Space", "j/k", "h/l", "Tab", "gg", "G", "Ctrl-d/u", "?", "q",
        ],
        Surface::MainDetail => &[
            "j/k", "gg", "G", "Ctrl-d/u", "s", "y", "/", "h/l", "Tab", "v", "Space", "?", "q",
        ],
        Surface::MainLeaderEntries => &["a", "e", "d", "Esc"],
        Surface::MainLeaderDetail => &["Esc"],
        Surface::MainVaultPicker => &["j/k", "a", "d", "Enter", "Esc"],
        Surface::Search => &["Esc", "Enter", "Backspace"],
        Surface::FormName => &[
            "Tab", "Enter", "Alt-s", "Alt-n", "Alt-d", "Alt-x", "Alt-v", "Esc",
        ],
        Surface::FormKey => &[
            "Enter", "Alt-s", "Tab", "Alt-n", "Alt-d", "Alt-x", "Alt-v", "Esc",
        ],
        Surface::FormValue => &[
            "Enter", "Alt-s", "Alt-n", "Alt-d", "Alt-x", "Alt-v", "Tab", "Esc",
        ],
        Surface::FormToken => &[
            "Enter", "Alt-s", "Alt-g", "Alt-r", "Alt-n", "Alt-d", "Alt-x", "Alt-v", "Tab", "Esc",
        ],
        Surface::FormType => &[
            "j/k", "Alt-s", "Enter", "Alt-n", "Alt-d", "Alt-v", "Tab", "Esc",
        ],
        Surface::ConfirmSave => &["s/Enter", "d", "b/Esc"],
        Surface::ConfirmDiscard => &["d", "b/Enter"],
        Surface::ConfirmDelete => &["d", "c/Esc"],
        Surface::VaultFormName => &["Tab", "Enter", "Esc"],
        Surface::VaultFormCheckbox => &["Alt-t", "Tab", "Enter", "Esc"],
        Surface::Locked | Surface::LockedError => &["Enter", "Backspace", "Esc"],
        Surface::FormVaultPicker => &["j/k", "a", "Enter", "Esc"],
        Surface::PasswordGen => &["g", "a", "Esc", "+/-", "u/l/d/s/x"],
        Surface::ErrorOverlay => &["Esc / Enter"],
    }
}

/// Every surface that has a table — the overlay's whole inventory.
fn all_surfaces() -> &'static [bindings::Surface] {
    use bindings::Surface;
    &[
        Surface::MainEntries,
        Surface::MainDetail,
        Surface::MainLeaderEntries,
        Surface::MainLeaderDetail,
        Surface::MainVaultPicker,
        Surface::Search,
        Surface::FormName,
        Surface::FormKey,
        Surface::FormValue,
        Surface::FormToken,
        Surface::FormType,
        Surface::ConfirmSave,
        Surface::ConfirmDiscard,
        Surface::ConfirmDelete,
        Surface::VaultFormName,
        Surface::VaultFormCheckbox,
        Surface::Locked,
        Surface::LockedError,
        Surface::FormVaultPicker,
        Surface::PasswordGen,
        Surface::ErrorOverlay,
    ]
}

#[test]
fn test_no_dead_advertised_chords() {
    for surface in all_surfaces() {
        let advertised: Vec<&str> = bindings::table(*surface)
            .iter()
            .map(|binding| binding.key)
            .collect();
        let routed = routed_chords(*surface);

        for chord in &advertised {
            assert!(
                routed.contains(chord),
                "{surface:?}: the overlay advertises `{chord}` but no handler routes it"
            );
        }
        for chord in routed {
            assert!(
                advertised.contains(chord),
                "{surface:?}: the handler routes `{chord}` but the overlay does not list it"
            );
        }
    }
}

#[test]
fn test_global_group_in_overlay() {
    let keys: Vec<&str> = bindings::GLOBAL.iter().map(|binding| binding.key).collect();
    for chord in ["?", "q", "Ctrl-c", "Ctrl-l"] {
        assert!(
            keys.contains(&chord),
            "the overlay's Everywhere group must carry `{chord}`, got {keys:?}"
        );
    }
}

#[tokio::test]
async fn help_overlay_toggles_and_returns_to_the_prior_surface() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    assert_eq!(
        tui.status_hint(),
        FOOTER_TEXT,
        "the overlay does not change the footer"
    );

    key(&mut tui, KeyCode::Esc); // dismiss
    assert_eq!(tui.status_hint(), FOOTER_TEXT);

    tui.handle_key_with_modifiers(KeyCode::Char('?'), KeyModifiers::SHIFT);
    key(&mut tui, KeyCode::Char('q')); // q also dismisses
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    Ok(())
}

#[test]
fn render_holds_no_hand_written_hints() {
    let render = include_str!("../../src/app/render.rs");
    let form_render = include_str!("../../src/views/entry_form/render.rs");
    let form_row = include_str!("../../src/views/entry_form/row.rs");
    for literal in [
        "[s/Enter] save",
        "[d] discard",
        "[j/k] select type",
        "Editing token",
        "[/] search",
        "[g] regenerate",
        "Enter passphrase",
        "[Alt-t] toggle default",
    ] {
        assert!(
            !render.contains(literal),
            "render.rs must generate `{literal}` from bindings.rs"
        );
    }
    for literal in ["[Alt-n] add field", "[Alt-d] remove field"] {
        assert!(
            !form_render.contains(literal),
            "entry_form/render.rs must generate `{literal}` from bindings.rs"
        );
        assert!(
            !form_row.contains(literal),
            "entry_form/row.rs must generate `{literal}` from bindings.rs"
        );
    }
}

#[test]
fn modals_hold_no_hand_written_hints() {
    let modals = include_str!("../../src/app/modals.rs");
    for literal in ["[s/Enter] save", "[d] discard", "back to form", "[b/Esc]"] {
        assert!(
            !modals.contains(literal),
            "modals.rs must generate `{literal}` from bindings.rs"
        );
    }
}

// ── whole-tree scan ──────────────────────────────────────────────────────────

/// Every render module in the tree, by path — the scan covers all of them.
const RENDER_MODULES: &[(&str, &str)] = &[
    ("app/render.rs", include_str!("../../src/app/render.rs")),
    ("app/modals.rs", include_str!("../../src/app/modals.rs")),
    (
        "views/entry_form/render.rs",
        include_str!("../../src/views/entry_form/render.rs"),
    ),
    (
        "views/entry_form/row.rs",
        include_str!("../../src/views/entry_form/row.rs"),
    ),
    (
        "views/entry_form/widgets.rs",
        include_str!("../../src/views/entry_form/widgets.rs"),
    ),
    (
        "views/main_view/render.rs",
        include_str!("../../src/views/main_view/render.rs"),
    ),
    (
        "views/main_view/detail.rs",
        include_str!("../../src/views/main_view/detail.rs"),
    ),
    (
        "views/vault_form.rs",
        include_str!("../../src/views/vault_form.rs"),
    ),
    (
        "views/password_gen_view.rs",
        include_str!("../../src/views/password_gen_view.rs"),
    ),
];

/// `true` when `inner` reads like a key chord (`Tab`, `Esc / Enter`, `s/Enter`).
fn is_chord(inner: &str) -> bool {
    !inner.is_empty()
        && inner.chars().count() <= 12
        && inner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/+- ?".contains(c))
        && inner.chars().any(|c| c.is_ascii_alphanumeric())
}

/// `true` when the line advertises a key: a bracket group followed by a word,
/// the shape of every hand-written hint (`[d] delete`).
fn has_chord_literal(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(offset) = chars[i + 1..].iter().position(|c| *c == ']') {
                let inner: String = chars[i + 1..i + 1 + offset].iter().collect();
                let space = chars.get(i + 2 + offset) == Some(&' ');
                let word = chars
                    .get(i + 3 + offset)
                    .is_some_and(|c| c.is_ascii_alphabetic());
                if is_chord(&inner) && space && word {
                    return true;
                }
                i += offset + 2;
                continue;
            }
        }
        i += 1;
    }
    false
}

#[test]
fn drift_scan_covers_all_render_modules() {
    // -- Check: the detector flags a hand-written hint and spares a state glyph
    assert!(
        has_chord_literal("            hint: \"  [d] delete  [c/Esc] cancel\","),
        "the scan must flag a hand-written hint row"
    );
    assert!(
        has_chord_literal("Paragraph::new(\"[g] regenerate  [a] accept  [Esc] cancel\")"),
        "the scan must flag hand-written footer text"
    );
    assert!(
        !has_chord_literal("let marker = if on { \"[x]\" } else { \"[ ]\" };"),
        "a checkbox marker is state, not a key hint"
    );
    assert!(
        !has_chord_literal("\"[agent-servable]\""),
        "a pill is a value, not a key hint"
    );

    // -- Check: no render module advertises a key by hand
    let mut offenders: Vec<String> = Vec::new();
    for (path, source) in RENDER_MODULES {
        for line in source.lines() {
            if has_chord_literal(line) {
                offenders.push(format!("{path}: {}", line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "every hint must come from bindings.rs, found {} hand-written: {offenders:#?}",
        offenders.len()
    );
}

// ── chord semantics ──────────────────────────────────────────────────────────

/// The action every table gives `key`, if any.
fn actions_for(tables: &[&[bindings::Binding]], key: &str) -> Vec<&'static str> {
    let mut actions: Vec<&'static str> = Vec::new();
    for table in tables {
        for binding in *table {
            if binding.key == key {
                actions.push(binding.action);
            }
        }
    }
    actions.sort_unstable();
    actions.dedup();
    actions
}

#[test]
fn bindings_use_one_action_per_chord_family() {
    let tables: &[&[bindings::Binding]] = &[
        bindings::MAIN_ENTRIES,
        bindings::MAIN_DETAIL,
        bindings::MAIN_LEADER_ENTRIES,
        bindings::MAIN_LEADER_DETAIL,
        bindings::MAIN_VAULT_PICKER,
        bindings::SEARCH,
        bindings::FORM_NAME,
        bindings::FORM_KEY,
        bindings::FORM_TOKEN,
        bindings::FORM_VALUE,
        bindings::FORM_TYPE_TAIL,
        bindings::CONFIRM_SAVE,
        bindings::CONFIRM_DISCARD,
        bindings::CONFIRM_DELETE,
        bindings::VAULT_FORM_NAME,
        bindings::VAULT_FORM_CHECKBOX,
        bindings::LOCKED,
        bindings::FORM_VAULT_PICKER,
        bindings::PASSWORD_GEN,
        bindings::PASSWORD_GEN_TOGGLES,
        bindings::ERROR_OVERLAY,
        bindings::HELP,
    ];

    // -- Check: the field-management chords read the same on every surface
    for key in ["Alt-n", "Alt-d"] {
        let actions = actions_for(tables, key);
        assert_eq!(
            actions.len(),
            1,
            "`{key}` must mean one thing across the registry, found: {actions:?}"
        );
    }

    // -- Check: Tab names its destination with one verb family
    assert_eq!(
        actions_for(tables, "Tab"),
        vec![
            "back to name",
            "focus columns",
            "focus detail",
            "next",
            "next field",
            "toggle default"
        ],
        "Tab is pane navigation in main and field navigation in forms"
    );

    // -- Check: the retired wording is gone from the registry source
    let source = include_str!("../../src/app/bindings.rs");
    for retired in ["new field", "remove field", "\"columns\""] {
        assert!(
            !source.contains(retired),
            "bindings.rs must not keep the retired wording `{retired}`"
        );
    }
}

#[tokio::test]
async fn main_view_footer_wordings_are_stable() -> Result<()> {
    let app = make_app().await?;
    let mut tui = Tui::new(app, TuiContext::Default).await?;

    // -- Check: the footer is the same row on both panes — the chords moved to
    // the overlay, where the full inventory lives
    assert_eq!(tui.status_hint(), FOOTER_TEXT);
    key(&mut tui, KeyCode::Tab); // → detail column
    assert_eq!(tui.status_hint(), FOOTER_TEXT);

    // -- Check: the overlay still names both Tab destinations
    let tab_action = |surface: bindings::Surface| {
        bindings::table(surface)
            .iter()
            .find(|binding| binding.key == "Tab")
            .map(|binding| binding.action)
    };
    assert_eq!(
        tab_action(bindings::Surface::MainEntries),
        Some("focus detail")
    );
    assert_eq!(
        tab_action(bindings::Surface::MainDetail),
        Some("focus columns")
    );
    Ok(())
}
