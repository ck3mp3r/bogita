use crate::exit_with_error;
use crate::secretize;
use bogita_core::domain::SecretString;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

/// The usage line the source guard prints when two sources arrive at once.
const UNLOCK_USAGE: &str =
    "usage: bogita vault unlock [--password <value> | --password-stdin | --password-file <path>]";

/// Which source supplies the unlock passphrase.
///
/// The passphrase moves through as a single [`SecretString`]: no source keeps a
/// plaintext copy, and no variant prints anything.
pub(crate) enum UnlockSecret {
    /// No flag: the terminal prompt runs as it always has.
    Interactive,
    /// `--password`: taken from argv.
    Arg(SecretString),
    /// `--password-stdin`: one line from stdin.
    Stdin(SecretString),
    /// `--password-file`: the file content.
    File(SecretString),
}

impl UnlockSecret {
    /// The resolved passphrase, or `None` when the terminal prompt must run.
    pub(crate) fn into_passphrase(self) -> Option<SecretString> {
        match self {
            Self::Interactive => None,
            Self::Arg(secret) | Self::Stdin(secret) | Self::File(secret) => Some(secret),
        }
    }
}

/// A refusal from the passphrase source resolver.
#[derive(Debug)]
pub(crate) enum UnlockError {
    /// Two sources were given. Clap rejects this at parse time; the handler
    /// guard covers a caller that builds the arguments another way.
    Conflict,
    /// A source could not be read.
    Source(String),
}

impl UnlockError {
    /// The process exit code this refusal reports.
    fn exit_code(&self) -> i32 {
        match self {
            Self::Conflict => 2,
            Self::Source(_) => 1,
        }
    }

    /// Report the refusal on stderr and stop.
    pub(crate) fn report(self) -> ! {
        match self {
            Self::Conflict => {
                eprintln!("error: give only one passphrase source");
                eprintln!("{UNLOCK_USAGE}");
                std::process::exit(self.exit_code());
            }
            Self::Source(message) => exit_with_error(&message),
        }
    }
}

impl std::fmt::Display for UnlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => write!(f, "give only one passphrase source"),
            Self::Source(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for UnlockError {}

/// Resolve the unlock passphrase source, refusing more than one.
///
/// The stdin handle is a parameter so the read path is testable without a
/// terminal. Nothing is written to stdout on any path.
pub(crate) fn resolve_unlock_secret<R: BufRead>(
    password: Option<String>,
    password_stdin: bool,
    password_file: Option<PathBuf>,
    stdin: &mut R,
) -> std::result::Result<UnlockSecret, UnlockError> {
    let sources = usize::from(password.is_some())
        + usize::from(password_stdin)
        + usize::from(password_file.is_some());
    if sources > 1 {
        return Err(UnlockError::Conflict);
    }

    if let Some(raw) = password {
        return Ok(UnlockSecret::Arg(secretize(raw)));
    }
    if password_stdin {
        return read_stdin_secret(stdin).map(UnlockSecret::Stdin);
    }
    if let Some(path) = password_file {
        return read_file_secret(&path).map(UnlockSecret::File);
    }
    Ok(UnlockSecret::Interactive)
}

/// Read one line from stdin as the passphrase.
fn read_stdin_secret<R: BufRead>(stdin: &mut R) -> std::result::Result<SecretString, UnlockError> {
    let mut raw = Vec::new();
    stdin
        .read_until(b'\n', &mut raw)
        .map_err(|e| UnlockError::Source(format!("cannot read the passphrase from stdin: {e}")))?;
    let line = into_utf8(raw, "the passphrase from stdin")?;
    Ok(secretize(strip_trailing_newline(line)))
}

/// Read the passphrase from a file that no other user can read.
fn read_file_secret(path: &Path) -> std::result::Result<SecretString, UnlockError> {
    check_passphrase_file_mode(path)?;
    let raw = std::fs::read(path).map_err(|e| {
        UnlockError::Source(format!(
            "cannot read the passphrase file {}: {e}",
            path.display()
        ))
    })?;
    let content = into_utf8(raw, &format!("the passphrase file {}", path.display()))?;
    Ok(secretize(strip_trailing_newline(content)))
}

/// Move raw passphrase bytes into text, zeroizing them when they are not UTF-8.
fn into_utf8(raw: Vec<u8>, source: &str) -> std::result::Result<String, UnlockError> {
    match String::from_utf8(raw) {
        Ok(text) => Ok(text),
        Err(error) => {
            let mut bytes = error.into_bytes();
            bytes.zeroize();
            Err(UnlockError::Source(format!("{source} is not valid UTF-8")))
        }
    }
}

/// Refuse a passphrase file any other user can read.
///
/// The house rule for secrets on disk is `0o600`; a group- or other-readable
/// file could leak the passphrase to another local user.
#[cfg(unix)]
fn check_passphrase_file_mode(path: &Path) -> std::result::Result<(), UnlockError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)
        .map_err(|e| {
            UnlockError::Source(format!(
                "cannot read the passphrase file {}: {e}",
                path.display()
            ))
        })?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(UnlockError::Source(format!(
            "passphrase file must not be group/other readable (mode 0{:03o}) — chmod 600 {}",
            mode & 0o777,
            path.display()
        )));
    }
    Ok(())
}

/// File modes are not portable: every non-Unix target skips the check.
#[cfg(not(unix))]
fn check_passphrase_file_mode(_path: &Path) -> std::result::Result<(), UnlockError> {
    Ok(())
}

/// Drop exactly one trailing newline, and the carriage return before it.
fn strip_trailing_newline(mut line: String) -> String {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    line
}

#[cfg(test)]
#[path = "../test/automation_unlock.rs"]
mod automation_unlock_test;
