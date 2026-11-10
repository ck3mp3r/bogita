//! RFC 9987 conformance tests for the agent daemon (agent/server.rs)
//!
//! Gated on the `integration` feature: these tests bind a real unix socket, spawn
//! the serve loop, and speak the protocol to it.
//!
//! The client channel is the client half of `ssh-agent-lib` 0.6.0 (`client::Client`,
//! `Client::handle(Request) -> Response`); its checkpoint was verified against the
//! pinned crate source before this file was written. It can express every message
//! the daemon decodes, which is all but two cases. Those two need bytes the codec
//! cannot produce, so they use the hand-rolled frame writer in the raw-frames region
//! below.
//!
//! Refusal shapes differ by where the refusal happens, and this suite asserts each
//! shape at its own layer:
//!
//! - 17, 18, 19, 20, 21, 22, 23, 25 decode fine and are refused by the `Session`
//!   defaults, so they answer FAILURE(5) on the wire.
//! - An unknown message number fails in the library's `Request::decode`, before any
//!   session method runs. The connection handler returns and the socket closes with
//!   no wire reply. RFC 9987 § 6 would prefer FAILURE(5); v1 keeps the library codec,
//!   so the suite pins the close-without-reply behaviour.
//! - EXTENSION(27) with an unknown name decodes and answers EXTENSION_FAILURE(28).
//!   The two `bogita-*` names keep their success path.
//! - A length prefix above `MAX_FRAME_LEN` never reaches the codec: the cap in
//!   `server.rs` refuses the prefix and closes the connection without a wire reply, so an
//!   announced length can no longer park the codec. RFC 9987 § 6 allows the close branch.
//!
//! Keys: RSA = rsa-sha2-256/512 only; SHA-1 (`ssh-rsa`) is never supported.
//!
//! Out of scope, decided at spec time:
//! - `ssh-keysign`: needs a `ssh-keysign` host configuration file and a setuid binary,
//!   neither of which exists in the dev shell. Not exercised.
//! - `ssh -Y` (X11 forwarding): needs a live SSH server with X11 enabled. Not
//!   exercised. The real-client channel is `ssh-add` (see real_client.rs).
//! - The signing helper's verify path belongs to the `ssh-keygen` forwarding task, not
//!   to the daemon suite.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::error::SshAgentError;
use crate::agent::keyring::{KeyRing, SharedKeyRing};
use crate::agent::server::{serve, MAX_FRAME_LEN};
use crate::domain::ssh::{agent_servable_field, SshKeyMaterial};
use crate::domain::Entry;
use crate::test_support::test_entry;
use secrecy::SecretString;
use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::{
    AddIdentity, AddIdentityConstrained, Extension, Identity, KeyConstraint, PrivateCredential,
    PublicCredential, RemoveIdentity, Request, Response, SignRequest, SmartcardKey,
};
use ssh_key::public::KeyData;
use ssh_key::{PrivateKey, PublicKey};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DATA: &[u8] = b"ssh-agent-sign-request-data";
const STARTUP_LIMIT: usize = 200;

/// How long a raw-frame test waits for a reply before it calls the silence a refusal.
const RAW_REPLY_WAIT: Duration = Duration::from_millis(500);

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
    Arc::new(Mutex::new(ring))
}

fn temp_socket(dir: &Path) -> PathBuf {
    dir.join("data").join("agent.sock")
}

fn public_key_data(material: &SshKeyMaterial) -> Result<KeyData> {
    Ok(PublicKey::from_openssh(material.public_line())?
        .key_data()
        .clone())
}

fn private_credential(material: &SshKeyMaterial) -> Result<PrivateCredential> {
    let key = PrivateKey::from_openssh(material.private_pem())?;
    Ok(PrivateCredential::Key {
        privkey: key.key_data().clone(),
        comment: material.comment().to_string(),
    })
}

fn smartcard_key() -> SmartcardKey {
    SmartcardKey {
        id: "token-0".to_string(),
        pin: SecretString::from("1234".to_string()),
    }
}

fn identity_extension(name: &str) -> Extension {
    Extension {
        name: name.to_string(),
        details: Vec::new().into(),
    }
}

fn identities(response: Response) -> Result<Vec<Identity>> {
    let Response::IdentitiesAnswer(identities) = response else {
        return Err(format!("expected an identities answer, got {response:?}").into());
    };
    Ok(identities)
}

fn sign_request(material: &SshKeyMaterial) -> Result<Request> {
    Ok(Request::SignRequest(SignRequest {
        credential: PublicCredential::Key(public_key_data(material)?),
        data: DATA.to_vec(),
        flags: 0,
    }))
}

/// Block until the daemon has bound its socket (or the budget runs out).
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
    handle: JoinHandle<std::result::Result<(), SshAgentError>>,
    socket_path: PathBuf,
}

impl RunningAgent {
    async fn start(ring: SharedKeyRing, socket_path: PathBuf) -> Result<Self> {
        let handle = tokio::spawn(serve(ring, socket_path.clone()));
        wait_for_socket(&socket_path).await?;
        Ok(Self {
            handle,
            socket_path,
        })
    }

    async fn client(&self) -> Result<Client<UnixStream>> {
        Ok(Client::new(UnixStream::connect(&self.socket_path).await?))
    }

    /// True while the serve task still runs.
    fn is_serving(&self) -> bool {
        !self.handle.is_finished()
    }

    async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }
}

// endregion: --- Fixtures

// region:    --- Raw frames

/// Frame writer for the two messages the `ssh-agent-lib` codec cannot express.
///
/// RFC 9987 § 5: `uint32 length` followed by `length` bytes, the first of which is
/// the message number.
async fn send_raw(stream: &mut UnixStream, length: u32, payload: &[u8]) -> Result<()> {
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

/// How one raw-frame read ended.
#[derive(Debug)]
enum FrameRead {
    /// A frame arrived. The raw cases expect the close branch, so the body is not inspected.
    Frame,
    /// The daemon closed the socket without answering.
    Closed,
    Silent,
}

/// One reply, `Closed` when the daemon hung up, `Silent` when nothing arrived.
async fn read_outcome(stream: &mut UnixStream, wait: Duration) -> Result<FrameRead> {
    let mut header = [0u8; 4];
    let first = match tokio::time::timeout(wait, stream.read(&mut header)).await {
        Err(_) => return Ok(FrameRead::Silent),
        Ok(Err(e)) => return Err(e.into()),
        Ok(Ok(0)) => return Ok(FrameRead::Closed),
        Ok(Ok(n)) => n,
    };
    stream.read_exact(&mut header[first..]).await?;
    let length = u32::from_be_bytes(header) as usize;
    stream.read_exact(&mut vec![0u8; length]).await?;
    Ok(FrameRead::Frame)
}

// endregion: --- Raw frames

// region:    --- Refusals (criterion 1)

/// One entry per refused message number, RFC 9987 § 6.1.
fn unsupported_requests(material: &SshKeyMaterial) -> Result<Vec<(u8, Request)>> {
    let credential = private_credential(material)?;
    let identity = AddIdentity {
        credential: credential.clone(),
    };
    Ok(vec![
        (17, Request::AddIdentity(identity)),
        (
            18,
            Request::RemoveIdentity(RemoveIdentity {
                credential: PublicCredential::Key(public_key_data(material)?),
            }),
        ),
        (19, Request::RemoveAllIdentities),
        (20, Request::AddSmartcardKey(smartcard_key())),
        (21, Request::RemoveSmartcardKey(smartcard_key())),
        (22, Request::Lock("passphrase".to_string())),
        (23, Request::Unlock("passphrase".to_string())),
        (
            25,
            Request::AddIdConstrained(AddIdentityConstrained {
                identity: AddIdentity { credential },
                constraints: vec![KeyConstraint::Lifetime(60)],
            }),
        ),
    ])
}

#[tokio::test]
async fn test_agent_conformance_refuses_every_unsupported_request() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("refused@example.com")?;
    let agent =
        RunningAgent::start(ring_with(&[("github", &material)]), temp_socket(dir.path())).await?;
    let mut client = agent.client().await?;
    let before = identities(client.handle(Request::RequestIdentities).await?)?.len();

    // -- Exec
    let mut refused = Vec::new();
    for (number, request) in unsupported_requests(&material)? {
        let response = client.handle(request).await?;

        // -- Check
        assert_eq!(
            response,
            Response::Failure,
            "message {number} must answer FAILURE(5) on the wire"
        );
        refused.push(number);
    }

    // -- Check: every refusal was sent, and none of them touched the ring
    assert_eq!(refused, vec![17, 18, 19, 20, 21, 22, 23, 25]);
    let after = identities(client.handle(Request::RequestIdentities).await?)?.len();
    assert_eq!(before, 1);
    assert_eq!(after, before, "a refusal must not change the key ring");

    agent.stop().await;
    Ok(())
}

// endregion: --- Refusals (criterion 1)

// region:    --- Constraints (criterion 3)

#[tokio::test]
async fn test_agent_conformance_refuses_constrained_adds() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let listed = SshKeyMaterial::keygen_ed25519("listed@example.com")?;
    let offered = SshKeyMaterial::keygen_ed25519("offered@example.com")?;
    let agent =
        RunningAgent::start(ring_with(&[("github", &listed)]), temp_socket(dir.path())).await?;
    let mut client = agent.client().await?;
    let before = identities(client.handle(Request::RequestIdentities).await?)?.len();

    // -- Exec: the constraint types of RFC 9987 § 8.2 Table 3 the daemon may receive
    for constraint in [KeyConstraint::Lifetime(300), KeyConstraint::Confirm] {
        let label = format!("{constraint:?}");
        let response = client
            .handle(Request::AddIdConstrained(AddIdentityConstrained {
                identity: AddIdentity {
                    credential: private_credential(&offered)?,
                },
                constraints: vec![constraint],
            }))
            .await?;

        // -- Check
        assert_eq!(
            response,
            Response::Failure,
            "ADD_ID_CONSTRAINED with {label} must answer FAILURE(5)"
        );
        let after = identities(client.handle(Request::RequestIdentities).await?)?.len();
        assert_eq!(after, before, "a {label} add must not grow the key ring");
    }

    agent.stop().await;
    Ok(())
}

// endregion: --- Constraints (criterion 3)

// region:    --- Extensions (criterion 2)

#[tokio::test]
async fn test_agent_conformance_refuses_unknown_extension_names() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("extension@example.com")?;
    let agent =
        RunningAgent::start(ring_with(&[("github", &material)]), temp_socket(dir.path())).await?;
    let mut client = agent.client().await?;

    // -- Exec & Check: any name outside the two vendor extensions is refused
    for name in [
        "bogita-keys@bogita",
        "bogita-status",
        "bogita-stop@example.com",
        "query@openssh.com",
        "session-bind@openssh.com",
        "",
    ] {
        let response = client
            .handle(Request::Extension(identity_extension(name)))
            .await?;
        assert_eq!(
            response,
            Response::ExtensionFailure,
            "extension {name:?} must answer EXTENSION_FAILURE(28)"
        );
    }

    // -- Check: the accepted name keeps its success path
    let status = client
        .handle(Request::Extension(identity_extension(
            crate::agent::control::STATUS_EXTENSION,
        )))
        .await?;
    assert!(
        matches!(status, Response::ExtensionResponse(_)),
        "the status extension must stay answered, got {status:?}"
    );

    agent.stop().await;
    Ok(())
}

// endregion: --- Extensions (criterion 2)

// region:    --- Identities shape

#[tokio::test]
async fn test_agent_conformance_answers_identities_as_key_comment_pairs() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let first = SshKeyMaterial::keygen_ed25519("first@example.com")?;
    let second = SshKeyMaterial::keygen_ed25519("second@example.com")?;
    let agent = RunningAgent::start(
        ring_with(&[("first", &first), ("second", &second)]),
        temp_socket(dir.path()),
    )
    .await?;
    let mut client = agent.client().await?;

    // -- Exec
    let answered = identities(client.handle(Request::RequestIdentities).await?)?;

    // -- Check: nkeys matches the ring, and every blob is paired with its comment
    assert_eq!(answered.len(), 2, "nkeys must be the number of served keys");
    for (identity, material) in answered.iter().zip([&first, &second]) {
        assert!(
            matches!(identity.credential, PublicCredential::Key(_)),
            "v1 advertises plain keys only, got {:?}",
            identity.credential
        );
        assert_eq!(
            identity.credential.key_data(),
            &public_key_data(material)?,
            "the blob must be the key it was loaded from"
        );
        assert_eq!(
            identity.comment,
            material.comment(),
            "the comment must belong to the key next to it"
        );
    }

    // -- Check: each advertised key really signs, so the blob is usable
    for material in [&first, &second] {
        let signed = client.handle(sign_request(material)?).await?;
        let Response::SignResponse(signature) = signed else {
            return Err(format!("expected a sign response, got {signed:?}").into());
        };
        Verifier::verify(
            &PublicKey::from_openssh(material.public_line())?,
            DATA,
            &signature,
        )?;
    }

    agent.stop().await;
    Ok(())
}

// endregion: --- Identities shape

// region:    --- Framing edges

#[tokio::test]
async fn test_agent_conformance_closes_on_an_unknown_message_number() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("unknown-number@example.com")?;
    let agent =
        RunningAgent::start(ring_with(&[("github", &material)]), temp_socket(dir.path())).await?;
    let mut stream = UnixStream::connect(&agent.socket_path).await?;

    // -- Exec: message number 99 is not in the RFC 9987 table
    send_raw(&mut stream, 1, &[99]).await?;
    let reply = read_outcome(&mut stream, RAW_REPLY_WAIT).await?;

    // -- Check: the decode stage rejects it, so the socket closes without a reply
    assert!(
        matches!(reply, FrameRead::Closed),
        "an unknown message number must close without a reply, got {reply:?}"
    );
    assert!(
        agent.is_serving(),
        "an unknown message number must not stop the daemon"
    );
    let mut client = agent.client().await?;
    assert_eq!(
        identities(client.handle(Request::RequestIdentities).await?)?.len(),
        1,
        "the daemon must keep serving after a refused frame"
    );

    agent.stop().await;
    Ok(())
}

#[tokio::test]
async fn test_agent_conformance_closes_on_an_oversized_length_prefix() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("oversized@example.com")?;
    let agent =
        RunningAgent::start(ring_with(&[("github", &material)]), temp_socket(dir.path())).await?;

    // -- Exec & Check: one byte over the cap, then 2^31 bytes that never arrive
    for length in [MAX_FRAME_LEN + 1, 0x8000_0000] {
        let mut stream = UnixStream::connect(&agent.socket_path).await?;
        send_raw(&mut stream, length, &[11]).await?;
        let reply = read_outcome(&mut stream, RAW_REPLY_WAIT).await?;
        assert!(
            matches!(reply, FrameRead::Closed),
            "a {length}-byte prefix must close the connection, got {reply:?}"
        );
    }

    // -- Check: the daemon keeps serving after both refusals
    assert!(
        agent.is_serving(),
        "an oversized length prefix must not stop the daemon"
    );
    let mut client = agent.client().await?;
    let answered = identities(client.handle(Request::RequestIdentities).await?)?;
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].comment, material.comment());

    agent.stop().await;
    Ok(())
}

// endregion: --- Framing edges
