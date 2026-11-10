//! Tests for the concealed passphrase prompt seam. The seam feeds the
//! rpassword read path from injected buffers, so prompt handling is testable
//! without a terminal.

use super::secretize;
use bogita_core::domain::{ExposeSecret, SecretString};
use std::io::{BufRead, Cursor, Write};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// region:    --- Test support

/// Prompt over injected streams, so the prompt path is testable without a tty.
fn prompt_passphrase_from<R: BufRead, W: Write>(
    prompt: &str,
    input: &mut R,
    output: &mut W,
) -> std::io::Result<SecretString> {
    rpassword::prompt_password_from_bufread(input, output, prompt).map(secretize)
}

// endregion: --- Test support

#[test]
fn prompt_text_is_written_verbatim() -> Result<()> {
    let mut input = Cursor::new(b"correct horse\n".to_vec());
    let mut output = Vec::new();
    prompt_passphrase_from("Enter passphrase: ", &mut input, &mut output)?;
    assert_eq!(String::from_utf8(output)?, "Enter passphrase: ");
    Ok(())
}

#[test]
fn prompt_does_not_echo_the_typed_secret() -> Result<()> {
    let mut input = Cursor::new(b"correct horse\n".to_vec());
    let mut output = Vec::new();
    prompt_passphrase_from("Set a passphrase: ", &mut input, &mut output)?;
    let rendered = String::from_utf8(output)?;
    assert!(!rendered.contains("correct horse"));
    Ok(())
}

#[test]
fn trims_surrounding_whitespace() -> Result<()> {
    let mut input = Cursor::new(b"  correct horse  \n".to_vec());
    let mut output = Vec::new();
    let secret = prompt_passphrase_from("Enter passphrase: ", &mut input, &mut output)?;
    assert_eq!(secret.expose_secret(), "correct horse");
    Ok(())
}

#[test]
fn preserves_inner_whitespace() -> Result<()> {
    let mut input = Cursor::new(b"\tcorrect horse battery\t\n".to_vec());
    let mut output = Vec::new();
    let secret = prompt_passphrase_from("Enter passphrase: ", &mut input, &mut output)?;
    assert_eq!(secret.expose_secret(), "correct horse battery");
    Ok(())
}

#[test]
fn empty_line_yields_empty_secret() -> Result<()> {
    let mut input = Cursor::new(b"\n".to_vec());
    let mut output = Vec::new();
    let secret = prompt_passphrase_from("Enter passphrase: ", &mut input, &mut output)?;
    assert_eq!(secret.expose_secret(), "");
    Ok(())
}

#[test]
fn whitespace_only_line_yields_empty_secret() -> Result<()> {
    let mut input = Cursor::new(b"   \n".to_vec());
    let mut output = Vec::new();
    let secret = prompt_passphrase_from("Enter passphrase: ", &mut input, &mut output)?;
    assert_eq!(secret.expose_secret(), "");
    Ok(())
}
