//! OTP/TOTP service.
//!
//! Generates Time-based One-time Passwords conforming to RFC 6238.
//! Also parses `otpauth://` URIs as specified by the Google Authenticator
//! Key URI Format.
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

// ── public types ──────────────────────────────────────────────────────────────

/// A parsed `otpauth://` URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpUri {
    /// The raw base32-encoded secret (uppercase, no padding).
    pub secret_b32: String,
    /// Human-readable label (issuer:account or just account).
    pub label: String,
    /// Issuer from the `issuer` query param (optional).
    pub issuer: Option<String>,
    /// Number of digits in the OTP code (default 6).
    pub digits: u32,
    /// Step period in seconds (default 30).
    pub period: u64,
}

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

/// Parse an `otpauth://` URI into an [`OtpUri`].
///
/// Supports the Google Authenticator Key URI Format:
/// `otpauth://TYPE/LABEL?PARAMETERS`
pub fn parse_otp_uri(uri: &str) -> Result<OtpUri> {
    // Require otpauth:// prefix.
    let rest = uri.strip_prefix("otpauth://").ok_or_else(|| {
        ValidationError::InvalidOtpUri("URI scheme must be 'otpauth'".to_string())
    })?;

    // Split TYPE / LABEL ? QUERY
    let (type_and_label, query) = rest.split_once('?').unwrap_or((rest, ""));

    // TYPE is the first path segment (totp / hotp).
    let (_, label_encoded) = type_and_label
        .split_once('/')
        .ok_or_else(|| ValidationError::InvalidOtpUri("missing label in OTP URI".to_string()))?;

    let label = percent_decode(label_encoded);

    // Parse query parameters.
    let mut secret_b32: Option<String> = None;
    let mut issuer: Option<String> = None;
    let mut digits: u32 = 6;
    let mut period: u64 = 30;

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "secret" => secret_b32 = Some(value.to_ascii_uppercase()),
            "issuer" => issuer = Some(percent_decode(value)),
            "digits" => {
                if let Ok(n) = value.parse::<u32>() {
                    digits = n;
                }
            }
            "period" => {
                if let Ok(n) = value.parse::<u64>() {
                    period = n;
                }
            }
            _ => {} // ignore unknown params
        }
    }

    let secret_b32 = secret_b32.ok_or_else(|| {
        ValidationError::InvalidOtpUri("OTP URI missing 'secret' parameter".to_string())
    })?;

    Ok(OtpUri {
        secret_b32,
        label,
        issuer,
        digits,
        period,
    })
}

/// Compute the current TOTP code from either a plain base32 secret or an
/// `otpauth://` URI.  Returns `None` if the secret cannot be decoded.
pub fn compute_totp(raw: &str) -> Option<String> {
    let (secret_bytes, step, digits) = if raw.starts_with("otpauth://") {
        let uri = parse_otp_uri(raw).ok()?;
        let bytes = decode_secret(&uri.secret_b32).ok()?;
        (bytes, uri.period, uri.digits)
    } else {
        let bytes = decode_secret(raw).ok()?;
        (bytes, 30u64, 6u32)
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    generate_totp(&secret_bytes, step, digits, now)
        .map(|(code, _)| code)
        .ok()
}

// ── internal helpers ──────────────────────────────────────────────────────────

/// Minimal percent-decoder for OTP URI labels and parameter values.
/// Handles %XX sequences; leaves everything else as-is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2])) {
                out.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
