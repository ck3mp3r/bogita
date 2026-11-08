//! OTP/TOTP service.
//!
//! Generates Time-based One-time Passwords conforming to RFC 6238.
//!
//! ## Example
//! ```ignore
//! use std::time::{SystemTime, UNIX_EPOCH};
//! let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
//! let secret = decode_secret("JBSWY3DPEHPK3PXP").unwrap();
//! let (code, remaining) = generate_totp(&secret, 30, 6, now).unwrap();
//! println!("{code}  (valid for {remaining}s)");
//! ```

use crate::error::{Result, ValidationError};
use data_encoding::BASE32_NOPAD;
use totp_lite::{totp_custom, Sha1};

// ── public API ────────────────────────────────────────────────────────────────

/// Decode a base32-encoded OTP secret (case-insensitive, padding optional).
///
/// Returns the raw secret bytes suitable for passing to [`generate_totp`].
pub fn decode_secret(b32: &str) -> Result<Vec<u8>> {
    let upper = b32.to_ascii_uppercase();
    // Strip optional padding.
    let stripped = upper.trim_end_matches('=');
    BASE32_NOPAD
        .decode(stripped.as_bytes())
        .map_err(|e| ValidationError::InvalidOtpSecret(e.to_string()).into())
}

/// Generate a TOTP code.
///
/// # Arguments
/// * `secret`  — raw secret bytes (decode with [`decode_secret`] from base32)
/// * `step`    — time step in seconds (typically 30)
/// * `digits`  — code length (6 or 8)
/// * `now_secs` — current Unix timestamp in seconds
///
/// # Returns
/// `(code, seconds_remaining)` — the formatted code (zero-padded) and the
/// number of seconds until the current code expires.
pub fn generate_totp(
    secret: &[u8],
    step: u64,
    digits: u32,
    now_secs: u64,
) -> Result<(String, u64)> {
    let code = totp_custom::<Sha1>(step, digits, secret, now_secs);
    let remaining = step - (now_secs % step);
    Ok((code, remaining))
}

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/service/otp.rs"]
mod otp_test;
// endregion: --- Tests
