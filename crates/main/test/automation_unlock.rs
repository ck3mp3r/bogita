//! Tests for the automation passphrase sources: the source mutual-exclusion
//! guard, the one-line strip rules, and the passphrase-file mode gate.

use super::{resolve_unlock_secret, strip_trailing_newline, UnlockError, UnlockSecret};
use bogita_core::domain::ExposeSecret;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// region:    --- Fixtures

/// Per-test temporary directory, removed when the guard drops.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> std::io::Result<Self> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("bogita-unlock-{}-{unique}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_file(dir: &Path, name: &str, content: &[u8]) -> std::io::Result<PathBuf> {
    let path = dir.join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Write a passphrase file with the mode the gate accepts.
fn write_private_file(dir: &Path, name: &str, content: &[u8]) -> std::io::Result<PathBuf> {
    let path = write_file(dir, name, content)?;
    #[cfg(unix)]
    set_mode(&path, 0o600)?;
    Ok(path)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

/// The resolved passphrase text, for assertions.
fn value_of(secret: UnlockSecret) -> String {
    match secret {
        UnlockSecret::Interactive => String::new(),
        UnlockSecret::Arg(value) | UnlockSecret::Stdin(value) | UnlockSecret::File(value) => {
            value.expose_secret().to_string()
        }
    }
}

// endregion: --- Fixtures

// region:    --- Source selection

#[test]
fn no_source_flag_keeps_the_interactive_prompt() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(None, false, None, &mut Cursor::new(b""))?;

    // -- Check
    assert!(matches!(secret, UnlockSecret::Interactive));
    Ok(())
}

#[test]
fn argument_source_takes_the_value_verbatim() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(
        Some("correct horse battery".to_string()),
        false,
        None,
        &mut Cursor::new(b"ignored\n"),
    )?;

    // -- Check
    assert!(matches!(secret, UnlockSecret::Arg(_)));
    assert_eq!(value_of(secret), "correct horse battery");
    Ok(())
}

#[test]
fn argument_source_trims_like_the_interactive_prompt() -> Result<()> {
    // -- Exec: the interactive path trims surrounding whitespace before use
    let secret = resolve_unlock_secret(
        Some("  pass  ".to_string()),
        false,
        None,
        &mut Cursor::new(b""),
    )?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[test]
fn stdin_source_takes_one_line() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"pass\n"))?;

    // -- Check
    assert!(matches!(secret, UnlockSecret::Stdin(_)));
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[test]
fn stdin_source_strips_a_carriage_return_before_the_newline() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"pass\r\n"))?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[test]
fn stdin_source_accepts_a_missing_final_newline() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"pass"))?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[test]
fn stdin_source_reads_only_the_first_line() -> Result<()> {
    // -- Exec
    let secret = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"pass\nsecond\n"))?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[test]
fn stdin_source_rejects_invalid_utf8() -> Result<()> {
    // -- Exec
    let result = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"\xff\xfe\n"));

    // -- Check
    let Err(error) = result else {
        return Err("invalid UTF-8 must be refused".into());
    };
    assert!(matches!(error, UnlockError::Source(_)));
    assert_eq!(error.exit_code(), 1);
    Ok(())
}

#[test]
fn an_empty_stdin_line_is_a_legal_empty_passphrase() -> Result<()> {
    // -- Exec: an empty passphrase is a legal vault state, not an error
    let secret = resolve_unlock_secret(None, true, None, &mut Cursor::new(b"\n"))?;

    // -- Check
    assert_eq!(value_of(secret), "");
    Ok(())
}

// endregion: --- Source selection

// region:    --- Mutual exclusion

#[test]
fn two_sources_are_refused() -> Result<()> {
    // -- Setup & Fixtures
    let cases: [(&str, Option<String>, bool, Option<PathBuf>); 3] = [
        ("argv + stdin", Some("x".to_string()), true, None),
        (
            "argv + file",
            Some("x".to_string()),
            false,
            Some(PathBuf::from("/tmp/pass")),
        ),
        ("stdin + file", None, true, Some(PathBuf::from("/tmp/pass"))),
    ];

    for (label, password, stdin, file) in cases {
        // -- Exec
        let result = resolve_unlock_secret(password, stdin, file, &mut Cursor::new(b""));

        // -- Check: the handler guard refuses with the usage exit code
        let Err(error) = result else {
            return Err(format!("{label} must be refused").into());
        };
        assert!(matches!(error, UnlockError::Conflict), "{label}");
        assert_eq!(error.exit_code(), 2, "{label}");
    }
    Ok(())
}

// endregion: --- Mutual exclusion

// region:    --- Strip rules

#[test]
fn strip_removes_one_trailing_newline() {
    assert_eq!(strip_trailing_newline("pass\n".to_string()), "pass");
}

#[test]
fn strip_removes_one_trailing_carriage_return_before_the_newline() {
    assert_eq!(strip_trailing_newline("pass\r\n".to_string()), "pass");
}

#[test]
fn strip_keeps_a_line_without_a_newline_unchanged() {
    assert_eq!(strip_trailing_newline("pass".to_string()), "pass");
}

#[test]
fn strip_keeps_a_bare_carriage_return() {
    assert_eq!(strip_trailing_newline("pass\r".to_string()), "pass\r");
}

#[test]
fn strip_removes_only_one_newline() {
    assert_eq!(strip_trailing_newline("pass\n\n".to_string()), "pass\n");
}

// endregion: --- Strip rules

// region:    --- Passphrase file

#[test]
fn file_source_strips_the_trailing_newline() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_private_file(scratch.path(), "pass.txt", b"correct horse\n")?;

    // -- Exec
    let secret = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""))?;

    // -- Check
    assert!(matches!(secret, UnlockSecret::File(_)));
    assert_eq!(value_of(secret), "correct horse");
    Ok(())
}

#[test]
fn file_source_accepts_a_missing_final_newline() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_private_file(scratch.path(), "pass.txt", b"correct horse")?;

    // -- Exec
    let secret = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""))?;

    // -- Check
    assert_eq!(value_of(secret), "correct horse");
    Ok(())
}

#[test]
fn file_source_keeps_internal_newlines() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_private_file(scratch.path(), "pass.txt", b"line one\nline two\n")?;

    // -- Exec
    let secret = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""))?;

    // -- Check
    assert_eq!(value_of(secret), "line one\nline two");
    Ok(())
}

#[test]
fn file_source_rejects_invalid_utf8() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_private_file(scratch.path(), "pass.txt", b"\xff\xfe\n")?;

    // -- Exec
    let result = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""));

    // -- Check
    assert!(matches!(result, Err(UnlockError::Source(_))));
    Ok(())
}

#[test]
fn file_source_reports_a_missing_file() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = scratch.path().join("absent.txt");

    // -- Exec
    let result = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""));

    // -- Check
    let Err(UnlockError::Source(message)) = result else {
        return Err("a missing passphrase file must be refused".into());
    };
    assert!(message.contains("absent.txt"), "message: {message}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_source_accepts_an_owner_only_mode() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_file(scratch.path(), "pass.txt", b"pass\n")?;
    set_mode(&path, 0o600)?;

    // -- Exec
    let secret = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""))?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_source_accepts_a_tighter_than_owner_only_mode() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_file(scratch.path(), "pass.txt", b"pass\n")?;
    set_mode(&path, 0o400)?;

    // -- Exec
    let secret = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""))?;

    // -- Check
    assert_eq!(value_of(secret), "pass");
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_source_refuses_a_group_readable_mode() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_file(scratch.path(), "pass.txt", b"pass\n")?;
    set_mode(&path, 0o640)?;

    // -- Exec
    let result = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""));

    // -- Check: the refusal names the mode and the fix
    let Err(UnlockError::Source(message)) = result else {
        return Err("a group-readable passphrase file must be refused".into());
    };
    assert!(message.contains("mode 0640"), "message: {message}");
    assert!(message.contains("chmod 600"), "message: {message}");
    assert_eq!(UnlockError::Source(message).exit_code(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_source_refuses_an_other_readable_mode() -> Result<()> {
    // -- Setup & Fixtures
    let scratch = Scratch::new()?;
    let path = write_file(scratch.path(), "pass.txt", b"pass\n")?;
    set_mode(&path, 0o604)?;

    // -- Exec
    let result = resolve_unlock_secret(None, false, Some(path), &mut Cursor::new(b""));

    // -- Check
    let Err(UnlockError::Source(message)) = result else {
        return Err("an other-readable passphrase file must be refused".into());
    };
    assert!(message.contains("mode 0604"), "message: {message}");
    Ok(())
}

// endregion: --- Passphrase file
