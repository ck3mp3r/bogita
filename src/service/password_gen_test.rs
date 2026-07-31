//! Tests for the password generator service.

use crate::service::password_gen::{CharsetOptions, PasswordGen};

// ── construction ──────────────────────────────────────────────────────────────

#[test]
fn default_options_include_all_charsets() {
    let opts = CharsetOptions::default();
    assert!(opts.uppercase);
    assert!(opts.lowercase);
    assert!(opts.digits);
    assert!(opts.symbols);
    assert!(!opts.avoid_ambiguous);
}

// ── generation ────────────────────────────────────────────────────────────────

#[test]
fn generated_password_has_requested_length() {
    let gen = PasswordGen::new(20, CharsetOptions::default());
    let pw = gen.generate();
    assert_eq!(expose(&pw).len(), 20);
}

#[test]
fn generated_password_respects_length_1() {
    let gen = PasswordGen::new(1, CharsetOptions::default());
    let pw = gen.generate();
    assert_eq!(expose(&pw).len(), 1);
}

#[test]
fn generated_password_length_64() {
    let gen = PasswordGen::new(64, CharsetOptions::default());
    let pw = gen.generate();
    assert_eq!(expose(&pw).len(), 64);
}

#[test]
fn uppercase_only_uses_only_uppercase() {
    let opts = CharsetOptions {
        uppercase: true,
        lowercase: false,
        digits: false,
        symbols: false,
        avoid_ambiguous: false,
    };
    let gen = PasswordGen::new(50, opts);
    let pw = gen.generate();
    assert!(
        expose(&pw).chars().all(|c| c.is_ascii_uppercase()),
        "password should only contain uppercase: {}",
        expose(&pw)
    );
}

#[test]
fn lowercase_only_uses_only_lowercase() {
    let opts = CharsetOptions {
        uppercase: false,
        lowercase: true,
        digits: false,
        symbols: false,
        avoid_ambiguous: false,
    };
    let gen = PasswordGen::new(50, opts);
    let pw = gen.generate();
    assert!(
        expose(&pw).chars().all(|c| c.is_ascii_lowercase()),
        "password should only contain lowercase: {}",
        expose(&pw)
    );
}

#[test]
fn digits_only_uses_only_digits() {
    let opts = CharsetOptions {
        uppercase: false,
        lowercase: false,
        digits: true,
        symbols: false,
        avoid_ambiguous: false,
    };
    let gen = PasswordGen::new(50, opts);
    let pw = gen.generate();
    assert!(
        expose(&pw).chars().all(|c| c.is_ascii_digit()),
        "password should only contain digits: {}",
        expose(&pw)
    );
}

#[test]
fn avoid_ambiguous_excludes_ambiguous_chars() {
    let opts = CharsetOptions {
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: false,
        avoid_ambiguous: true,
    };
    let gen = PasswordGen::new(200, opts);
    let pw = gen.generate();
    let ambiguous = ['0', 'O', 'l', '1', 'I'];
    for c in ambiguous {
        assert!(
            !expose(&pw).contains(c),
            "password should not contain ambiguous char '{c}': {}",
            expose(&pw)
        );
    }
}

#[test]
fn empty_charset_falls_back_to_lowercase() {
    // All options false → should not panic, falls back gracefully.
    let opts = CharsetOptions {
        uppercase: false,
        lowercase: false,
        digits: false,
        symbols: false,
        avoid_ambiguous: false,
    };
    let gen = PasswordGen::new(10, opts);
    let pw = gen.generate();
    assert_eq!(expose(&pw).len(), 10);
}

// ── entropy ───────────────────────────────────────────────────────────────────

#[test]
fn entropy_increases_with_charset_size() {
    let opts_small = CharsetOptions {
        uppercase: false,
        lowercase: false,
        digits: true,
        symbols: false,
        avoid_ambiguous: false,
    };
    let opts_large = CharsetOptions::default();

    let gen_small = PasswordGen::new(16, opts_small);
    let gen_large = PasswordGen::new(16, opts_large);

    assert!(
        gen_large.entropy_bits() > gen_small.entropy_bits(),
        "larger charset should yield higher entropy"
    );
}

#[test]
fn entropy_increases_with_length() {
    let opts = CharsetOptions::default();
    let gen_short = PasswordGen::new(8, opts.clone());
    let gen_long = PasswordGen::new(16, opts);
    assert!(
        gen_long.entropy_bits() > gen_short.entropy_bits(),
        "longer password should yield higher entropy"
    );
}

#[test]
fn entropy_digits_only_16_chars_is_around_53_bits() {
    // log2(10) * 16 ≈ 53.15
    let opts = CharsetOptions {
        uppercase: false,
        lowercase: false,
        digits: true,
        symbols: false,
        avoid_ambiguous: false,
    };
    let gen = PasswordGen::new(16, opts);
    let e = gen.entropy_bits();
    assert!(
        (50.0..=56.0).contains(&e),
        "expected ~53 bits for 16-char digits-only, got {e:.2}"
    );
}

// ── expose_secret helper ──────────────────────────────────────────────────────

// Thin wrapper to make tests read cleanly.
fn expose(s: &secrecy::SecretString) -> &str {
    use secrecy::ExposeSecret;
    s.expose_secret()
}
