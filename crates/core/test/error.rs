use crate::error::{ConfigError, Error};

#[test]
fn config_error_not_found_display() {
    let e = ConfigError::NotFound;
    assert_eq!(e.to_string(), "config file not found");
}

#[test]
fn config_error_parse_failed_display() {
    let e = ConfigError::ParseFailed("bad toml".to_string());
    assert_eq!(e.to_string(), "config parse failed: bad toml");
}

#[test]
fn config_error_write_failed_display() {
    let e = ConfigError::WriteFailed("permission denied".to_string());
    assert_eq!(e.to_string(), "config write failed: permission denied");
}

#[test]
fn error_config_variant_wraps_correctly() {
    let e: Error = ConfigError::NotFound.into();
    assert_eq!(e.to_string(), "config error: config file not found");
}
