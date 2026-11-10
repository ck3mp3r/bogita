//! Round-trip tests that cross the ssh-keygen process boundary.
//!
//! Gated behind the `integration` feature because they spawn `ssh-keygen`
//! from PATH.

use crate::handlers::ssh_sign::{forward, sign_data, SignRequest};
use bogita_core::domain::{agent_servable_field, Entry, EntryType, SshKeyMaterial};
use bogita_core::test_support::test_entry;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use uuid::Uuid;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// ── helpers ────────────────────────────────────────────────────────────────

fn ssh_entry(material: &SshKeyMaterial) -> Entry {
    let mut fields = material.to_fields();
    let idx = fields.len() as i32;
    fields.push(agent_servable_field(true, idx));
    Entry {
        entry_type: EntryType::SshKey,
        ..test_entry(Uuid::nil(), "alice@laptop", fields)
    }
}

fn sig_path(data: &Path) -> PathBuf {
    let mut path = data.as_os_str().to_owned();
    path.push(".sig");
    PathBuf::from(path)
}

/// Run `ssh-keygen` with `data` on stdin and return its exit code.
fn run_ssh_keygen(args: &[&str], data: &[u8]) -> Result<i32> {
    let mut child = Command::new("ssh-keygen")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("ssh-keygen stdin unavailable")?
        .write_all(data)?;
    Ok(child.wait()?.code().unwrap_or(-1))
}

/// Sign a payload through the helper and return the signature path.
fn signed_payload(dir: &Path, material: &SshKeyMaterial, data: &[u8]) -> Result<PathBuf> {
    let key_file = dir.join("git_signing_key_tmp");
    let data_file = dir.join("git_signing_buffer_tmp");
    std::fs::write(&key_file, material.public_line())?;
    std::fs::write(&data_file, data)?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file,
        data_file: data_file.clone(),
        use_agent: true,
    };
    sign_data(&[ssh_entry(material)], &request)?;
    Ok(sig_path(&data_file))
}

// ── round trips ─────────────────────────────────────────────────────────────

#[test]
fn ssh_keygen_check_novalidate_accepts_the_helper_signature() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let payload = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n";
    let signature = signed_payload(dir.path(), &material, payload)?;

    // -- Exec
    let status = run_ssh_keygen(
        &[
            "-Y",
            "check-novalidate",
            "-n",
            "git",
            "-s",
            signature.to_str().ok_or("sig path is not utf-8")?,
        ],
        payload,
    )?;

    // -- Check
    assert_eq!(status, 0);
    Ok(())
}

#[test]
fn ssh_keygen_verify_accepts_an_allowed_signers_line() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let payload = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n";
    let signature = signed_payload(dir.path(), &material, payload)?;
    let principal = "alice@example.com";
    let allowed_signers = dir.path().join("allowed_signers");
    std::fs::write(
        &allowed_signers,
        format!("{principal} {}\n", material.public_line()),
    )?;

    // -- Exec
    let status = run_ssh_keygen(
        &[
            "-Y",
            "verify",
            "-f",
            allowed_signers.to_str().ok_or("path is not utf-8")?,
            "-I",
            principal,
            "-n",
            "git",
            "-s",
            signature.to_str().ok_or("sig path is not utf-8")?,
        ],
        payload,
    )?;

    // -- Check
    assert_eq!(status, 0);
    Ok(())
}

#[test]
fn ssh_keygen_verify_rejects_a_different_principal() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let payload = b"payload\n";
    let signature = signed_payload(dir.path(), &material, payload)?;
    let allowed_signers = dir.path().join("allowed_signers");
    std::fs::write(
        &allowed_signers,
        format!("alice@example.com {}\n", material.public_line()),
    )?;

    // -- Exec
    let status = run_ssh_keygen(
        &[
            "-Y",
            "verify",
            "-f",
            allowed_signers.to_str().ok_or("path is not utf-8")?,
            "-I",
            "mallory@example.com",
            "-n",
            "git",
            "-s",
            signature.to_str().ok_or("sig path is not utf-8")?,
        ],
        payload,
    )?;

    // -- Check
    assert_ne!(status, 0, "an unknown principal must not verify");
    Ok(())
}

// ── forwarding ─────────────────────────────────────────────────────────────

#[test]
fn forward_runs_ssh_keygen_and_returns_its_exit_status() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let payload = b"payload\n";
    let signature = signed_payload(dir.path(), &material, payload)?;
    let sig = signature
        .to_str()
        .ok_or("sig path is not utf-8")?
        .to_string();
    let allowed_signers = dir.path().join("allowed_signers");
    std::fs::write(
        &allowed_signers,
        format!("alice@example.com {}\n", material.public_line()),
    )?;
    let allowed = allowed_signers
        .to_str()
        .ok_or("path is not utf-8")?
        .to_string();

    // -- Exec
    let ok = forward(&[
        "-Y".to_string(),
        "find-principals".to_string(),
        "-f".to_string(),
        allowed,
        "-s".to_string(),
        sig,
    ])?;
    let missing = forward(&[
        "-Y".to_string(),
        "check-novalidate".to_string(),
        "-n".to_string(),
        "git".to_string(),
        "-s".to_string(),
        dir.path().join("absent.sig").display().to_string(),
    ])?;

    // -- Check
    assert_eq!(ok, 0);
    assert_ne!(missing, 0, "ssh-keygen's failure status is forwarded");
    Ok(())
}
