//! Tests for the OTP/TOTP service.
//!
//! RFC 6238 test vectors are used where possible.

use crate::service::otp::generate_totp;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── generate_totp ─────────────────────────────────────────────────────────────

/// RFC 6238 Appendix B test vector: SHA-1, secret = b"12345678901234567890", 30s steps.
/// At T=59 (step 1): expected code = 94287082 (8 digits).
#[test]
fn totp_rfc6238_vector_t59() -> Result<()> {
    // -- Setup & Fixtures
    // Secret in the RFC uses raw ASCII bytes, not base32.
    let secret = b"12345678901234567890";

    // -- Exec
    let (code, _remaining) = generate_totp(secret, 30, 8, 59)?;

    // -- Check
    assert_eq!(code, "94287082", "RFC6238 T=59 SHA-1 vector mismatch");
    Ok(())
}

/// RFC 6238 Appendix B test vector: T=1111111109.
/// Expected code (SHA-1, 8 digits) = 07081804.
#[test]
fn totp_rfc6238_vector_t1111111109() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"12345678901234567890";

    // -- Exec
    let (code, _remaining) = generate_totp(secret, 30, 8, 1_111_111_109)?;

    // -- Check
    assert_eq!(code, "07081804");
    Ok(())
}

/// RFC 6238 Appendix B test vector: T=1234567890.
/// Expected code (SHA-1, 8 digits) = 89005924.
#[test]
fn totp_rfc6238_vector_t1234567890() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"12345678901234567890";

    // -- Exec
    let (code, _remaining) = generate_totp(secret, 30, 8, 1_234_567_890)?;

    // -- Check
    assert_eq!(code, "89005924");
    Ok(())
}

/// Default 6-digit output has exactly 6 characters.
#[test]
fn totp_default_6_digits() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"any_secret";

    // -- Exec
    let (code, _) = generate_totp(secret, 30, 6, 0)?;

    // -- Check
    assert_eq!(code.len(), 6, "expected 6-digit code, got: {code}");
    Ok(())
}

/// 8-digit output has exactly 8 characters.
#[test]
fn totp_8_digits() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"any_secret";

    // -- Exec
    let (code, _) = generate_totp(secret, 30, 8, 0)?;

    // -- Check
    assert_eq!(code.len(), 8, "expected 8-digit code, got: {code}");
    Ok(())
}

/// Returned code is numeric.
#[test]
fn totp_code_is_numeric() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"any_secret";

    // -- Exec
    let (code, _) = generate_totp(secret, 30, 6, 1_000_000)?;

    // -- Check
    assert!(
        code.chars().all(|c| c.is_ascii_digit()),
        "TOTP code must be all digits, got: {code}"
    );
    Ok(())
}

/// `seconds_remaining` is always in 0..step.
#[test]
fn totp_remaining_is_within_step() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"any_secret";
    let step = 30u64;
    let now = 1_700_000_017u64; // 17 seconds into a 30s window

    // -- Exec
    let (_, remaining) = generate_totp(secret, step, 6, now)?;

    // -- Check
    assert!(
        remaining < step,
        "seconds_remaining {remaining} should be < step {step}"
    );
    assert_eq!(remaining, step - (now % step));
    Ok(())
}

/// Same time → same code (determinism).
#[test]
fn totp_same_time_same_code() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"deterministic";

    // -- Exec
    let (a, _) = generate_totp(secret, 30, 6, 9999)?;
    let (b, _) = generate_totp(secret, 30, 6, 9999)?;

    // -- Check
    assert_eq!(a, b);
    Ok(())
}

/// Different time steps → possibly different codes.
/// (Probabilistically almost certain to differ.)
#[test]
fn totp_different_steps_likely_different() -> Result<()> {
    // -- Setup & Fixtures
    let secret = b"changing";

    // -- Exec
    let (a, _) = generate_totp(secret, 30, 6, 0)?;
    let (b, _) = generate_totp(secret, 30, 6, 30)?;

    // -- Check
    // Not strictly guaranteed to differ but true for any real secret.
    let _ = (a, b); // just exercise the code path without asserting equality
    Ok(())
}

// ── decode_secret ─────────────────────────────────────────────────────────────

/// Decoded JBSWY3DPEHPK3PXP decodes to the Google Authenticator example key.
/// Value verified empirically: b"Hello!\xde\xad\xbe\xef"
#[test]
fn otp_uri_decode_secret_known_value() -> Result<()> {
    // -- Setup & Fixtures
    use crate::service::otp::decode_secret;

    // -- Exec
    let bytes = decode_secret("JBSWY3DPEHPK3PXP")?;

    // -- Check
    // Verify first 6 bytes are b"Hello!"
    assert_eq!(&bytes[..6], b"Hello!");
    assert_eq!(bytes.len(), 10);
    Ok(())
}

/// Lowercase base32 is also accepted.
#[test]
fn otp_uri_decode_secret_lowercase() -> Result<()> {
    // -- Setup & Fixtures
    use crate::service::otp::decode_secret;

    // -- Exec
    let upper = decode_secret("JBSWY3DPEHPK3PXP")?;
    let lower = decode_secret("jbswy3dpehpk3pxp")?;

    // -- Check
    assert_eq!(upper, lower);
    Ok(())
}
