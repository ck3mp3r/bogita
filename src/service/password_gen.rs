//! Password generator service.
//!
//! Generates cryptographically-random passwords from a configurable charset
//! and estimates entropy in bits.

use rand::RngExt;
use secrecy::SecretString;

// ── CharsetOptions ────────────────────────────────────────────────────────────

/// Configuration for which character classes are included.
#[derive(Debug, Clone)]
pub struct CharsetOptions {
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
    /// Exclude visually ambiguous characters: `0 O l 1 I`.
    pub avoid_ambiguous: bool,
}

impl Default for CharsetOptions {
    fn default() -> Self {
        Self {
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
            avoid_ambiguous: false,
        }
    }
}

// ── PasswordGen ───────────────────────────────────────────────────────────────

/// Password generator — build with desired `length` and [`CharsetOptions`],
/// then call [`generate`](Self::generate) or inspect [`entropy_bits`](Self::entropy_bits).
pub struct PasswordGen {
    length: usize,
    charset: Vec<char>,
}

impl PasswordGen {
    /// Create a new generator with the given length and charset options.
    pub fn new(length: usize, opts: CharsetOptions) -> Self {
        let charset = build_charset(&opts);
        Self { length, charset }
    }

    /// Generate a random password of the configured length.
    ///
    /// Falls back to lowercase ASCII if the effective charset is empty.
    pub fn generate(&self) -> SecretString {
        let charset = if self.charset.is_empty() {
            &FALLBACK_CHARSET[..]
        } else {
            &self.charset[..]
        };

        let mut rng = rand::rng();
        let pw: String = (0..self.length)
            .map(|_| charset[rng.random_range(0..charset.len())])
            .collect();

        SecretString::from(pw)
    }

    /// Estimated password entropy in bits: `length × log₂(charset_size)`.
    pub fn entropy_bits(&self) -> f64 {
        let size = if self.charset.is_empty() {
            FALLBACK_CHARSET.len()
        } else {
            self.charset.len()
        };
        self.length as f64 * (size as f64).log2()
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Ambiguous characters excluded when `avoid_ambiguous` is set.
const AMBIGUOUS: &[char] = &['0', 'O', 'l', '1', 'I'];

/// Fallback charset (lowercase a-z) used when all options are disabled.
const FALLBACK_CHARSET: [char; 26] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

fn build_charset(opts: &CharsetOptions) -> Vec<char> {
    let mut chars: Vec<char> = Vec::new();

    if opts.uppercase {
        chars.extend('A'..='Z');
    }
    if opts.lowercase {
        chars.extend('a'..='z');
    }
    if opts.digits {
        chars.extend('0'..='9');
    }
    if opts.symbols {
        chars.extend("!@#$%^&*()-_=+[]{}|;:,.<>?".chars());
    }

    if opts.avoid_ambiguous {
        chars.retain(|c| !AMBIGUOUS.contains(c));
    }

    chars
}
