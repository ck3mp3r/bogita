use crate::views::main_view::MainView;
use bogita_core::domain::{Entry, EntryType, Field, FieldType, FieldValue, Vault};
use chrono::Utc;
use uuid::Uuid;

pub(super) fn make_vault(name: &str) -> Vault {
    Vault {
        id: Uuid::new_v4(),
        name: name.to_string(),
        is_default: false,
        created_at: Utc::now().timestamp(),
        sync_target: None,
        recipients: vec![],
        lock_timeout: None,
        auto_sync: false,
    }
}

pub(super) fn make_entry(name: &str, vault_id: Uuid, entry_type: EntryType) -> Entry {
    Entry {
        id: Uuid::new_v4(),
        vault_id,
        name: name.to_string(),
        entry_type,
        created_at: Utc::now().timestamp(),
        modified_at: Utc::now().timestamp(),
        fields: vec![],
    }
}

pub(super) fn make_password_entry(name: &str, vault_id: Uuid) -> Entry {
    let mut e = make_entry(name, vault_id, EntryType::Token);
    e.fields = vec![
        Field {
            id: Uuid::new_v4(),
            key: "username".into(),
            value: FieldValue::Text("alice".into()),
            field_type: FieldType::Username,
            encrypted: false,
            idx: 0,
        },
        Field {
            id: Uuid::new_v4(),
            key: "password".into(),
            value: FieldValue::Hidden("s3cr3t".into()),
            field_type: FieldType::Token,
            encrypted: true,
            idx: 1,
        },
    ];
    e
}

pub(super) fn make_otp_entry(name: &str, vault_id: Uuid) -> Entry {
    let mut e = make_entry(name, vault_id, EntryType::Otp);
    e.fields = vec![Field {
        id: Uuid::new_v4(),
        key: "secret".into(),
        value: FieldValue::Hidden("JBSWY3DPEHPK3PXP".into()),
        field_type: FieldType::TotpSecret,
        encrypted: true,
        idx: 0,
    }];
    e
}

pub(super) fn make_view_with_vaults() -> MainView {
    let v1 = make_vault("Work");
    let v2 = make_vault("Personal");
    let e1 = make_entry("GitHub", v1.id, EntryType::Token);
    let e2 = make_entry("GitLab", v1.id, EntryType::Token);
    let e3 = make_entry("Twitter", v2.id, EntryType::Token);
    MainView::new(vec![v1, v2], vec![e1, e2, e3])
}
