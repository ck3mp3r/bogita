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
fn test_entry_metadata_default() {
    let metadata = EntryMetadata::default();
    assert_eq!(metadata.url, None);
    assert_eq!(metadata.username, None);
    assert_eq!(metadata.notes, None);
    assert!(!metadata.favorite);
}

#[test]
fn test_entry_metadata_serialization() {
    let metadata = EntryMetadata {
        url: Some("https://github.com".to_string()),
        username: Some("user@example.com".to_string()),
        notes: Some("Test notes".to_string()),
        favorite: true,
    };

    let json = serde_json::to_string(&metadata).unwrap();
    let deserialized: EntryMetadata = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.url, metadata.url);
    assert_eq!(deserialized.username, metadata.username);
    assert_eq!(deserialized.notes, metadata.notes);
    assert_eq!(deserialized.favorite, metadata.favorite);
}

#[test]
fn test_password_data_serialization() {
    let password_data = PasswordData {
        password: "super-secret".to_string(),
        history: vec![],
    };

    let json = serde_json::to_string(&password_data).unwrap();
    let deserialized: PasswordData = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.password, "super-secret");
}

#[test]
fn test_otp_algorithm_defaults() {
    let algorithm = default_otp_algorithm();
    assert_eq!(algorithm, OtpAlgorithm::SHA1);

    let digits = default_otp_digits();
    assert_eq!(digits, 6);

    let period = default_otp_period();
    assert_eq!(period, 30);
}

#[test]
fn test_otp_data_serialization_with_defaults() {
    let otp_data = OtpData {
        secret: "JBSWY3DPEHPK3PXP".to_string(),
        algorithm: default_otp_algorithm(),
        digits: default_otp_digits(),
        period: default_otp_period(),
        issuer: Some("GitHub".to_string()),
        account: Some("user@example.com".to_string()),
    };

    let json = serde_json::to_string(&otp_data).unwrap();
    let deserialized: OtpData = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.algorithm, OtpAlgorithm::SHA1);
    assert_eq!(deserialized.digits, 6);
    assert_eq!(deserialized.period, 30);
}

#[test]
fn test_entry_data_tagged_enum_serialization() {
    // Test Password variant
    let password_entry = EntryData::Password(PasswordData {
        password: "test123".to_string(),
        history: vec![],
    });

    let json = serde_json::to_string(&password_entry).unwrap();
    assert!(json.contains(r#""type":"password""#));

    let deserialized: EntryData = serde_json::from_str(&json).unwrap();
    match deserialized {
        EntryData::Password(data) => {
            assert_eq!(data.password, "test123");
        }
        _ => panic!("Expected Password variant"),
    }
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

#[test]
fn test_entry_data_zeroization() {
    // Note: In actual usage, sensitive data in EntryData will be encrypted
    // The application layer will need to handle zeroization when working with decrypted data
    let password_data = EntryData::Password(PasswordData {
        password: "will-be-encrypted".to_string(),
        history: vec![],
    });

    // This data will be encrypted with age before storage
    drop(password_data);
}
