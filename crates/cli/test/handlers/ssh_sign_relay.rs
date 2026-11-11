//! End-to-end relay tests: the git signing helper against a live agent daemon.
//!
//! Gated on the `integration` feature: every test binds a real unix socket, spawns
//! `bogita_core::agent::server::serve`, and the cross-checks spawn `ssh-keygen`.
//!
//! The daemon is the only holder of key material here: the relay receives public
//! keys and signatures, never a private key, so an unlocked daemon is all a git
//! commit needs.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::handlers::ssh_relay::{self, DaemonRelay};
use crate::handlers::ssh_sign::{
    self, forward, HelperAction, HelperError, SignRequest, LOCKED_MESSAGE,
    MULTIPLE_SERVABLE_MESSAGE, NO_SERVABLE_MESSAGE,
};
use bogita_core::agent::client;
use bogita_core::agent::error::SshAgentError;
use bogita_core::agent::keyring::{KeyRing, SharedKeyRing};
use bogita_core::agent::server::serve;
use bogita_core::domain::ssh::{agent_servable_field, SshKeyMaterial};
use bogita_core::domain::Entry;
use bogita_core::test_support::{test_entry, RSA_OPENSSH_PEM};
use ssh_key::{Algorithm, HashAlg, PublicKey, SshSig};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use uuid::Uuid;

const PAYLOAD: &[u8] = b"tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n";
const STARTUP_LIMIT: usize = 200;

// region:    --- Fixtures

fn ssh_entry(name: &str, material: &SshKeyMaterial) -> Entry {
    let mut fields = material.to_fields();
    fields.push(agent_servable_field(true, fields.len() as i32));
    test_entry(Uuid::new_v4(), name, fields)
}

fn ring_with(materials: &[(&str, &SshKeyMaterial)]) -> SharedKeyRing {
    let mut ring = KeyRing::new();
    ring.load_entries(
        materials
            .iter()
            .map(|(name, material)| ssh_entry(name, material))
            .collect::<Vec<_>>(),
    );
    std::sync::Arc::new(std::sync::Mutex::new(ring))
}

fn temp_socket(dir: &Path) -> PathBuf {
    dir.join("data").join("agent.sock")
}

async fn wait_for_socket(path: &Path) -> Result<()> {
    for _ in 0..STARTUP_LIMIT {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(format!("agent socket {} never appeared", path.display()).into())
}

struct RunningAgent {
    handle: tokio::task::JoinHandle<std::result::Result<(), SshAgentError>>,
    socket_path: PathBuf,
}

impl RunningAgent {
    async fn start(materials: &[(&str, &SshKeyMaterial)], socket_path: PathBuf) -> Result<Self> {
        Self::start_ring(ring_with(materials), socket_path).await
    }

    async fn start_ring(ring: SharedKeyRing, socket_path: PathBuf) -> Result<Self> {
        let handle = tokio::spawn(serve(ring, socket_path.clone()));
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            handle,
            socket_path,
        })
    }

    fn relay(&self) -> DaemonRelay {
        DaemonRelay::new(self.socket_path.clone())
    }

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

fn rsa_material() -> Result<SshKeyMaterial> {
    Ok(SshKeyMaterial::validate(RSA_OPENSSH_PEM)?)
}

fn sig_path(data: &Path) -> PathBuf {
    let mut path = data.as_os_str().to_owned();
    path.push(".sig");
    PathBuf::from(path)
}

/// The `-Y sign` request git builds, with its key line in a file.
fn request(dir: &Path, material: &SshKeyMaterial) -> Result<(SignRequest, PathBuf)> {
    let key_file = dir.join("git_signing_key_tmp");
    let data_file = dir.join("git_signing_buffer_tmp");
    std::fs::write(&key_file, material.public_line())?;
    std::fs::write(&data_file, PAYLOAD)?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file,
        data_file: data_file.clone(),
        use_agent: true,
    };
    Ok((request, data_file))
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

/// The gate answer the helper computes for `socket`.
fn gate_for(socket: &Path) -> std::result::Result<HelperAction, HelperError> {
    let status = client::status(socket).ok().flatten();
    ssh_sign::prepare(status.as_deref(), &["-Y".to_string(), "sign".to_string()])
}

// endregion: --- Fixtures

// region:    --- Relay sign

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_ed25519_armor_validates_with_ssh_keygen() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let agent = RunningAgent::start(&[("alice", &material)], temp_socket(dir.path())).await?;
    let (request, data_file) = request(dir.path(), &material)?;

    // -- Exec
    ssh_relay::sign(&mut agent.relay(), &request)?;

    // -- Check: the armor sits at <data>.sig, binds the namespace, and verifies
    let signature = SshSig::from_pem(std::fs::read(sig_path(&data_file))?)?;
    assert_eq!(signature.namespace(), "git");
    assert_eq!(signature.hash_alg(), HashAlg::Sha512);
    assert_eq!(signature.signature().algorithm(), Algorithm::Ed25519);
    PublicKey::from_openssh(material.public_line())?.verify("git", PAYLOAD, &signature)?;

    // -- Check: OpenSSH itself accepts the armor git would hand it
    let status = run_ssh_keygen(
        &[
            "-Y",
            "check-novalidate",
            "-n",
            "git",
            "-s",
            sig_path(&data_file)
                .to_str()
                .ok_or("sig path is not utf-8")?,
        ],
        PAYLOAD,
    )?;
    assert_eq!(status, 0, "ssh-keygen must accept the relayed armor");

    agent.stop().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_rsa_armor_names_rsa_sha2_512_and_validates() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = rsa_material()?;
    assert!(
        material.public_line().starts_with("ssh-rsa "),
        "the fixture must be an RSA key"
    );
    let agent = RunningAgent::start(&[("rsa", &material)], temp_socket(dir.path())).await?;
    let (request, data_file) = request(dir.path(), &material)?;

    // -- Exec
    ssh_relay::sign(&mut agent.relay(), &request)?;

    // -- Check: the 0x0000_0004 flag reaches the daemon and names SHA-512
    let signature = SshSig::from_pem(std::fs::read(sig_path(&data_file))?)?;
    assert_eq!(
        signature.signature().algorithm(),
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512)
        },
        "the relay asks for rsa-sha2-512"
    );

    // -- Check: OpenSSH verifies the RSA armor against the matched public key
    let status = run_ssh_keygen(
        &[
            "-Y",
            "check-novalidate",
            "-n",
            "git",
            "-s",
            sig_path(&data_file)
                .to_str()
                .ok_or("sig path is not utf-8")?,
        ],
        PAYLOAD,
    )?;
    assert_eq!(status, 0, "ssh-keygen must accept the relayed RSA armor");

    agent.stop().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_refuses_a_key_the_daemon_does_not_serve() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let served = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let requested = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let agent = RunningAgent::start(&[("bob", &served)], temp_socket(dir.path())).await?;
    let (request, data_file) = request(dir.path(), &requested)?;

    // -- Exec
    let result = ssh_relay::sign(&mut agent.relay(), &request);

    // -- Check
    assert!(
        matches!(result, Err(HelperError::NoMatchingKey)),
        "a key the ring does not hold is a no-match"
    );
    assert!(!sig_path(&data_file).exists(), "no armor without a match");

    agent.stop().await;
    Ok(())
}

// endregion: --- Relay sign

// region:    --- Gate and default key

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_gate_refuses_a_locked_daemon_without_prompting() -> Result<()> {
    // -- Setup & Fixtures: a daemon whose ring holds no key, as after a lock
    let dir = tempfile::tempdir()?;
    let agent = RunningAgent::start_ring(
        std::sync::Arc::new(std::sync::Mutex::new(KeyRing::new())),
        temp_socket(dir.path()),
    )
    .await?;
    assert_eq!(
        client::status(&agent.socket_path)?.as_deref(),
        Some("0 keys, locked"),
        "the daemon must report the locked ring"
    );

    // -- Exec
    let result = gate_for(&agent.socket_path);

    // -- Check: the gate refuses, so the helper never reaches the relay
    let error = match result {
        Err(error) => error,
        Ok(action) => return Err(format!("expected a locked gate, got {action:?}").into()),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);

    agent.stop().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_gate_refuses_a_daemon_that_is_not_running() -> Result<()> {
    // -- Setup & Fixtures: a socket path nobody bound
    let dir = tempfile::tempdir()?;
    let socket_path = temp_socket(dir.path());

    // -- Exec
    let result = gate_for(&socket_path);

    // -- Check: nothing answering is the locked contract, never a prompt
    let error = match result {
        Err(error) => error,
        Ok(action) => return Err(format!("expected a locked gate, got {action:?}").into()),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_relay_default_key_answers_from_the_daemon() -> Result<()> {
    // -- Setup & Fixtures: three daemons, one per cardinality
    let empty_dir = tempfile::tempdir()?;
    let empty = RunningAgent::start(&[], temp_socket(empty_dir.path())).await?;

    let one_dir = tempfile::tempdir()?;
    let only = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let one = RunningAgent::start(&[("alice", &only)], temp_socket(one_dir.path())).await?;

    let two_dir = tempfile::tempdir()?;
    let first = SshKeyMaterial::keygen_ed25519("first@example.com")?;
    let second = SshKeyMaterial::keygen_ed25519("second@example.com")?;
    let two = RunningAgent::start(
        &[("first", &first), ("second", &second)],
        temp_socket(two_dir.path()),
    )
    .await?;

    // -- Exec & Check: no served key
    let none = ssh_relay::default_key(&mut empty.relay());
    let error = match none {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::NoAgentServableKeys));
    assert_eq!(error.to_string(), NO_SERVABLE_MESSAGE);

    // -- Exec & Check: exactly one served key prints its line
    assert_eq!(
        ssh_relay::default_key(&mut one.relay())?,
        only.public_line()
    );

    // -- Exec & Check: several served keys refuse to pick
    let many = ssh_relay::default_key(&mut two.relay());
    let error = match many {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::MultipleAgentServableKeys));
    assert_eq!(error.to_string(), MULTIPLE_SERVABLE_MESSAGE);

    empty.stop().await;
    one.stop().await;
    two.stop().await;
    Ok(())
}

// endregion: --- Gate and default key

// region:    --- Forward

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_ssh_sign_forward_runs_ssh_keygen_and_returns_its_exit_status() -> Result<()> {
    // -- Setup & Fixtures: a relayed signature to look a principal up in
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let agent = RunningAgent::start(&[("alice", &material)], temp_socket(dir.path())).await?;
    let (request, data_file) = request(dir.path(), &material)?;
    ssh_relay::sign(&mut agent.relay(), &request)?;
    let signature = sig_path(&data_file)
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
        signature,
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

    agent.stop().await;
    Ok(())
}

// endregion: --- Forward
