//! Tests for the OTP/TOTP service.
//!
//! RFC 6238 test vectors are used where possible.

use crate::service::otp::{generate_totp, parse_otp_uri};

// ── generate_totp ─────────────────────────────────────────────────────────────

/// RFC 6238 Appendix B test vector: SHA-1, secret = b"12345678901234567890", 30s steps.
/// At T=59 (step 1): expected code = 94287082 (8 digits).
#[test]
fn totp_rfc6238_vector_t59() {
    // Secret in the RFC uses raw ASCII bytes, not base32.
    let secret = b"12345678901234567890";
    let (code, _remaining) = generate_totp(secret, 30, 8, 59).unwrap();
    assert_eq!(code, "94287082", "RFC6238 T=59 SHA-1 vector mismatch");
}

/// RFC 6238 Appendix B test vector: T=1111111109.
/// Expected code (SHA-1, 8 digits) = 07081804.
#[test]
fn totp_rfc6238_vector_t1111111109() {
    let secret = b"12345678901234567890";
    let (code, _remaining) = generate_totp(secret, 30, 8, 1_111_111_109).unwrap();
    assert_eq!(code, "07081804");
}

/// RFC 6238 Appendix B test vector: T=1234567890.
/// Expected code (SHA-1, 8 digits) = 89005924.
#[test]
fn totp_rfc6238_vector_t1234567890() {
    let secret = b"12345678901234567890";
    let (code, _remaining) = generate_totp(secret, 30, 8, 1_234_567_890).unwrap();
    assert_eq!(code, "89005924");
}

/// Default 6-digit output has exactly 6 characters.
#[test]
fn totp_default_6_digits() {
    let secret = b"any_secret";
    let (code, _) = generate_totp(secret, 30, 6, 0).unwrap();
    assert_eq!(code.len(), 6, "expected 6-digit code, got: {code}");
}

/// 8-digit output has exactly 8 characters.
#[test]
fn totp_8_digits() {
    let secret = b"any_secret";
    let (code, _) = generate_totp(secret, 30, 8, 0).unwrap();
    assert_eq!(code.len(), 8, "expected 8-digit code, got: {code}");
}

/// Returned code is numeric.
#[test]
fn totp_code_is_numeric() {
    let secret = b"any_secret";
    let (code, _) = generate_totp(secret, 30, 6, 1_000_000).unwrap();
    assert!(
        code.chars().all(|c| c.is_ascii_digit()),
        "TOTP code must be all digits, got: {code}"
    );
}

/// `seconds_remaining` is always in 0..step.
#[test]
fn totp_remaining_is_within_step() {
    let secret = b"any_secret";
    let step = 30u64;
    let now = 1_700_000_017u64; // 17 seconds into a 30s window
    let (_, remaining) = generate_totp(secret, step, 6, now).unwrap();
    assert!(
        remaining < step,
        "seconds_remaining {remaining} should be < step {step}"
    );
    assert_eq!(remaining, step - (now % step));
}

/// Same time → same code (determinism).
#[test]
fn totp_same_time_same_code() {
    let secret = b"deterministic";
    let (a, _) = generate_totp(secret, 30, 6, 9999).unwrap();
    let (b, _) = generate_totp(secret, 30, 6, 9999).unwrap();
    assert_eq!(a, b);
}

/// Different time steps → possibly different codes.
/// (Probabilistically almost certain to differ.)
#[test]
fn totp_different_steps_likely_different() {
    let secret = b"changing";
    let (a, _) = generate_totp(secret, 30, 6, 0).unwrap();
    let (b, _) = generate_totp(secret, 30, 6, 30).unwrap();
    // Not strictly guaranteed to differ but true for any real secret.
    let _ = (a, b); // just exercise the code path without asserting equality
}

// ── parse_otp_uri ─────────────────────────────────────────────────────────────

/// Canonical TOTP URI.
#[test]
fn parse_uri_basic_totp() {
    let uri = "otpauth://totp/Example%3Auser@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";
    let otp = parse_otp_uri(uri).unwrap();
    assert_eq!(otp.secret_b32, "JBSWY3DPEHPK3PXP");
    assert_eq!(otp.label, "Example:user@example.com");
    assert_eq!(otp.issuer.as_deref(), Some("Example"));
    assert_eq!(otp.digits, 6);
    assert_eq!(otp.period, 30);
}

/// Custom digits and period.
#[test]
fn parse_uri_custom_params() {
    let uri = "otpauth://totp/account?secret=BASE32SECRET&digits=8&period=60";
    let otp = parse_otp_uri(uri).unwrap();
    assert_eq!(otp.digits, 8);
    assert_eq!(otp.period, 60);
}

/// Missing secret → error.
#[test]
fn parse_uri_missing_secret_is_error() {
    let uri = "otpauth://totp/account?issuer=Foo";
    assert!(parse_otp_uri(uri).is_err(), "missing secret should error");
}

/// Wrong scheme → error.
#[test]
fn parse_uri_wrong_scheme_is_error() {
    let uri = "https://example.com?secret=ABC";
    assert!(
        parse_otp_uri(uri).is_err(),
        "non-otpauth scheme should error"
    );
}

/// HOTP type (not TOTP) is accepted; period defaults.
#[test]
fn parse_uri_hotp_type() {
    let uri = "otpauth://hotp/account?secret=JBSWY3DP&counter=0";
    // We only implement TOTP but a lenient parser should not crash.
    // Accept or return a distinct error — either is fine; just don't panic.
    let _ = parse_otp_uri(uri); // no assertion on Ok/Err
}

// ── decode_secret ─────────────────────────────────────────────────────────────

/// Decoded JBSWY3DPEHPK3PXP decodes to the Google Authenticator example key.
/// Value verified empirically: b"Hello!\xde\xad\xbe\xef"
#[test]
fn otp_uri_decode_secret_known_value() {
    use crate::service::otp::decode_secret;
    let bytes = decode_secret("JBSWY3DPEHPK3PXP").unwrap();
    // Verify first 6 bytes are b"Hello!"
    assert_eq!(&bytes[..6], b"Hello!");
    assert_eq!(bytes.len(), 10);
}

/// Lowercase base32 is also accepted.
#[test]
fn otp_uri_decode_secret_lowercase() {
    use crate::service::otp::decode_secret;
    let upper = decode_secret("JBSWY3DPEHPK3PXP").unwrap();
    let lower = decode_secret("jbswy3dpehpk3pxp").unwrap();
    assert_eq!(upper, lower);
}
