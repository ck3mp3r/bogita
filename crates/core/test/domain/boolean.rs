//! Boolean-field tests for the domain types

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::domain::*;

#[test]
fn test_field_type_boolean_storage_discriminator() -> Result<()> {
    // -- Setup & Fixtures
    let field_type = FieldType::Boolean;

    // -- Exec
    let storage = field_type.to_storage();
    let parsed = FieldType::from_storage(&storage);

    // -- Check
    assert_eq!(storage, "boolean");
    assert_eq!(
        parsed,
        FieldType::Boolean,
        "boolean must not fall to Custom"
    );
    Ok(())
}

#[test]
fn test_field_boolean_roundtrip_keeps_boolean_value() -> Result<()> {
    // -- Setup & Fixtures
    let field = Field {
        id: Uuid::new_v4(),
        key: "agent_servable".to_string(),
        value: FieldValue::Boolean(true),
        field_type: FieldType::Boolean,
        encrypted: false,
        idx: 0,
    };

    // -- Exec
    let json = serde_json::to_string(&field)?;
    let deserialized: Field = serde_json::from_str(&json)?;

    // -- Check
    assert_eq!(deserialized.field_type, FieldType::Boolean);
    assert_eq!(deserialized.value, FieldValue::Boolean(true));
    Ok(())
}

#[test]
fn test_agent_servable_field_type_is_boolean() -> Result<()> {
    // -- Exec
    let field = agent_servable_field(true, 0);

    // -- Check
    assert_eq!(field.field_type, FieldType::Boolean);
    assert_eq!(field.key, AGENT_SERVABLE_KEY);
    assert_eq!(field.value, FieldValue::Boolean(true));
    assert!(!field.encrypted);
    Ok(())
}
