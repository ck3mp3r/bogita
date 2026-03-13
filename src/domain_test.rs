//! Tests for domain types

use crate::domain::*;
use secrecy::ExposeSecret;
use std::str::FromStr;

#[test]
fn test_entry_type_serialization() {
    // Test that EntryType serializes to expected snake_case strings
    let password = EntryType::Password;
    let json = serde_json::to_string(&password).unwrap();
    assert_eq!(json, r#""password""#);

    let otp = EntryType::Otp;
    let json = serde_json::to_string(&otp).unwrap();
    assert_eq!(json, r#""otp""#);

    let ssh_key = EntryType::SshKey;
    let json = serde_json::to_string(&ssh_key).unwrap();
    assert_eq!(json, r#""ssh_key""#);

    let note = EntryType::Note;
    let json = serde_json::to_string(&note).unwrap();
    assert_eq!(json, r#""note""#);
}

#[test]
fn test_field_value_text_serialization() {
    let value = FieldValue::Text("hello world".to_string());
    let json = serde_json::to_string(&value).unwrap();
    let deserialized: FieldValue = serde_json::from_str(&json).unwrap();

    match deserialized {
        FieldValue::Text(s) => assert_eq!(s, "hello world"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_field_value_hidden_serialization() {
    let value = FieldValue::Hidden("secret-password".to_string());
    let json = serde_json::to_string(&value).unwrap();
    let deserialized: FieldValue = serde_json::from_str(&json).unwrap();

    match deserialized {
        FieldValue::Hidden(s) => assert_eq!(s, "secret-password"),
        _ => panic!("Expected Hidden variant"),
    }
}

#[test]
fn test_field_value_boolean_serialization() {
    let value = FieldValue::Boolean(true);
    let json = serde_json::to_string(&value).unwrap();
    let deserialized: FieldValue = serde_json::from_str(&json).unwrap();

    match deserialized {
        FieldValue::Boolean(b) => assert!(b),
        _ => panic!("Expected Boolean variant"),
    }
}

#[test]
fn test_field_value_number_serialization() {
    let value = FieldValue::Number(42);
    let json = serde_json::to_string(&value).unwrap();
    let deserialized: FieldValue = serde_json::from_str(&json).unwrap();

    match deserialized {
        FieldValue::Number(n) => assert_eq!(n, 42),
        _ => panic!("Expected Number variant"),
    }
}

#[test]
fn test_field_type_serialization() {
    let field_type = FieldType::Username;
    let json = serde_json::to_string(&field_type).unwrap();
    assert_eq!(json, r#""username""#);

    let field_type = FieldType::TotpSecret;
    let json = serde_json::to_string(&field_type).unwrap();
    assert_eq!(json, r#""totp_secret""#);
}

#[test]
fn test_field_type_custom_serialization() {
    let field_type = FieldType::Custom("my_custom_field".to_string());
    let json = serde_json::to_string(&field_type).unwrap();
    let deserialized: FieldType = serde_json::from_str(&json).unwrap();

    match deserialized {
        FieldType::Custom(name) => assert_eq!(name, "my_custom_field"),
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn test_field_serialization() {
    let field = Field {
        id: Uuid::new_v4(),
        key: "username".to_string(),
        value: FieldValue::Text("john@example.com".to_string()),
        field_type: FieldType::Username,
        encrypted: false,
        idx: 0,
    };

    let json = serde_json::to_string(&field).unwrap();
    let deserialized: Field = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.key, "username");
    assert_eq!(deserialized.field_type, FieldType::Username);
    assert!(!deserialized.encrypted);
    assert_eq!(deserialized.idx, 0);

    match deserialized.value {
        FieldValue::Text(s) => assert_eq!(s, "john@example.com"),
        _ => panic!("Expected Text value"),
    }
}

#[test]
fn test_entry_with_password_fields() {
    let entry_id = Uuid::new_v4();
    let vault_id = Uuid::new_v4();

    let entry = Entry {
        id: entry_id,
        vault_id,
        name: "GitHub".to_string(),
        entry_type: EntryType::Password,
        created_at: 1234567890,
        modified_at: 1234567890,
        fields: vec![
            Field {
                id: Uuid::new_v4(),
                key: "username".to_string(),
                value: FieldValue::Text("myuser".to_string()),
                field_type: FieldType::Username,
                encrypted: false,
                idx: 0,
            },
            Field {
                id: Uuid::new_v4(),
                key: "password".to_string(),
                value: FieldValue::Hidden("secret123".to_string()),
                field_type: FieldType::Password,
                encrypted: true,
                idx: 1,
            },
            Field {
                id: Uuid::new_v4(),
                key: "url".to_string(),
                value: FieldValue::Url("https://github.com".to_string()),
                field_type: FieldType::Url,
                encrypted: false,
                idx: 2,
            },
        ],
    };

    // Verify entry structure
    assert_eq!(entry.name, "GitHub");
    assert_eq!(entry.entry_type, EntryType::Password);
    assert_eq!(entry.fields.len(), 3);

    // Verify fields
    assert_eq!(entry.fields[0].key, "username");
    assert!(!entry.fields[0].encrypted);

    assert_eq!(entry.fields[1].key, "password");
    assert!(entry.fields[1].encrypted);

    assert_eq!(entry.fields[2].key, "url");
    assert!(!entry.fields[2].encrypted);
}

#[test]
fn test_entry_with_otp_fields() {
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "GitHub OTP".to_string(),
        entry_type: EntryType::Otp,
        created_at: 1234567890,
        modified_at: 1234567890,
        fields: vec![
            Field {
                id: Uuid::new_v4(),
                key: "secret".to_string(),
                value: FieldValue::Hidden("BASE32SECRET".to_string()),
                field_type: FieldType::TotpSecret,
                encrypted: true,
                idx: 0,
            },
            Field {
                id: Uuid::new_v4(),
                key: "issuer".to_string(),
                value: FieldValue::Text("GitHub".to_string()),
                field_type: FieldType::TotpIssuer,
                encrypted: false,
                idx: 1,
            },
            Field {
                id: Uuid::new_v4(),
                key: "digits".to_string(),
                value: FieldValue::Number(6),
                field_type: FieldType::TotpDigits,
                encrypted: false,
                idx: 2,
            },
        ],
    };

    assert_eq!(entry.entry_type, EntryType::Otp);
    assert_eq!(entry.fields.len(), 3);

    // Verify TOTP secret is encrypted
    assert!(entry.fields[0].encrypted);
    assert_eq!(entry.fields[0].field_type, FieldType::TotpSecret);

    // Verify issuer is searchable
    assert!(!entry.fields[1].encrypted);
    match &entry.fields[1].value {
        FieldValue::Text(s) => assert_eq!(s, "GitHub"),
        _ => panic!("Expected Text value"),
    }
}

#[test]
fn test_entry_serialization() {
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Test Entry".to_string(),
        entry_type: EntryType::Note,
        created_at: 1234567890,
        modified_at: 1234567890,
        fields: vec![Field {
            id: Uuid::new_v4(),
            key: "content".to_string(),
            value: FieldValue::Text("My secure note".to_string()),
            field_type: FieldType::Notes,
            encrypted: true,
            idx: 0,
        }],
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: Entry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.name, "Test Entry");
    assert_eq!(deserialized.entry_type, EntryType::Note);
    assert_eq!(deserialized.fields.len(), 1);
}

#[test]
fn test_operation_serialization() {
    let insert = Operation::Insert;
    let json = serde_json::to_string(&insert).unwrap();
    assert_eq!(json, r#""insert""#);

    let update = Operation::Update;
    let json = serde_json::to_string(&update).unwrap();
    assert_eq!(json, r#""update""#);

    let delete = Operation::Delete;
    let json = serde_json::to_string(&delete).unwrap();
    assert_eq!(json, r#""delete""#);
}

#[test]
fn test_change_with_entry() {
    let entry = Entry {
        id: Uuid::new_v4(),
        vault_id: Uuid::new_v4(),
        name: "Changed Entry".to_string(),
        entry_type: EntryType::Password,
        created_at: 1234567890,
        modified_at: 1234567899,
        fields: vec![],
    };

    let change = Change {
        entry_id: entry.id,
        vault_id: entry.vault_id,
        operation: Operation::Update,
        timestamp: chrono::Utc::now(),
        entry: entry.clone(),
    };

    assert_eq!(change.entry.name, "Changed Entry");
    assert_eq!(change.operation, Operation::Update);
}

#[test]
fn test_vault_backend_tagged_enum() {
    let git_backend = VaultBackend::Git(GitConfig {
        path: "/tmp/vault".to_string(),
        remote: "git@github.com:user/vault.git".to_string(),
    });

    let json = serde_json::to_string(&git_backend).unwrap();
    assert!(json.contains(r#""type":"git""#));

    let deserialized: VaultBackend = serde_json::from_str(&json).unwrap();
    match deserialized {
        VaultBackend::Git(config) => {
            assert_eq!(config.path, "/tmp/vault");
            assert_eq!(config.remote, "git@github.com:user/vault.git");
        }
        _ => panic!("Expected Git variant"),
    }
}

#[test]
fn test_age_recipient_parsing() {
    // Valid age recipient (public key)
    let recipient_str = "age1zvkyg2lqzraa2lnjvqej32nkuu0ues2s82hzrye869xeexvn73equnujwj";
    let recipient = AgeRecipient::from_str(recipient_str).unwrap();

    // Should be able to convert back to string
    let serialized = recipient.to_string();
    assert_eq!(serialized, recipient_str);
}

#[test]
fn test_age_identity_generation() {
    let identity = AgeIdentity::generate();

    // Should be able to derive recipient
    let _recipient = identity.to_recipient();

    // Should be able to get secret string
    let secret = identity.to_secret_string();
    let secret_str = secret.expose_secret();

    // Secret should start with AGE-SECRET-KEY-1
    assert!(secret_str.starts_with("AGE-SECRET-KEY-1"));
}
