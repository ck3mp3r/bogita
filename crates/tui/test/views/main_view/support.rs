use crate::views::main_view::MainView;
use bogita_core::domain::{Entry, EntryType};
use bogita_core::test_support::{test_entry, test_otp_entry, test_password_entry};
use uuid::Uuid;

pub(super) use bogita_core::test_support::test_vault as make_vault;

pub(super) fn make_entry(name: &str, vault_id: Uuid, entry_type: EntryType) -> Entry {
    match entry_type {
        EntryType::Otp => test_otp_entry(vault_id, name),
        _ => test_entry(vault_id, name, vec![]),
    }
}

pub(super) fn make_password_entry(name: &str, vault_id: Uuid) -> Entry {
    test_password_entry(vault_id, name)
}

pub(super) fn make_otp_entry(name: &str, vault_id: Uuid) -> Entry {
    test_otp_entry(vault_id, name)
}

pub(super) fn make_view_with_vaults() -> MainView {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("GitLab", v1.id, EntryType::Token);
    let e3 = make_entry("Twitter", v2.id, EntryType::Token);
    MainView::new(vec![v1, v2], vec![e1, e2, e3])
}
