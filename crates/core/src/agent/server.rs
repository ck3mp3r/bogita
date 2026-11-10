//! RFC 9987 serve loop over the gated unix socket.
//!
//! Message handling is delegated to `ssh-agent-lib`: it owns the `uint32 + byte`
//! framing, decode, and per-connection task. This module supplies the session
//! and the gate around it.
//!
//! Protocol v1 answers two requests, plus two vendor control extensions:
//!
//! | Message | Reply |
//! |---------|-------|
//! | REQUEST_IDENTITIES(11) | IDENTITIES_ANSWER(12) |
//! | SIGN_REQUEST(13) | SIGN_RESPONSE(14) |
//! | EXTENSION(27) `bogita-status@bogita` | EXTENSION_RESPONSE(29) with the status payload |
//! | EXTENSION(27) `bogita-stop@bogita` | SUCCESS(6), then a bounded drain |
//!
//! Every other number is refused: 17, 18, 19, 20, 21, 22, 23, 25, 26 answer
//! FAILURE(5) through the `Session` defaults, and any other EXTENSION(27) name
//! answers EXTENSION_FAILURE(28). No handler writes to the vault, and the daemon
//! never reads stdin.
//!
//! Frames are capped at [`MAX_FRAME_LEN`] before the codec sees them: a length prefix above
//! the cap closes the connection without a reply, so an announced length can never park a
//! session.

use crate::agent::control::{
    status_payload, Control, GRACEFUL_DRAIN, STATUS_EXTENSION, STOP_EXTENSION,
};
use crate::agent::error::SshAgentError;
use crate::agent::keyring::rsa_hash_alg;
use crate::agent::{signals, socket};
use ssh_agent_lib::agent::{listen, Agent, ListeningSocket, Session};
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Extension, Identity, PublicCredential, SignRequest};
use ssh_key::Signature;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, MutexGuard};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{UnixListener, UnixStream};

pub use crate::agent::keyring::SharedKeyRing;

// region:    --- Server

/// Upper bound between two reads of the termination flag.
const SIGNAL_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Longest frame prefix the daemon accepts: 256 KiB.
///
/// RFC 9987 sets no frame limit and the vendored codec has no knob for one: it waits for the
/// announced length, so a huge prefix parks the connection. The largest frame v1 can
/// legitimately receive carries an RSA key blob and its payload — ~540 bytes at RSA-4096 and
/// ~2 KiB at 16384 bits — so 256 KiB leaves real traffic more than an order of magnitude of
/// headroom.
///
/// RSA keys sign with rsa-sha2-256 or rsa-sha2-512 only; SHA-1 (`ssh-rsa`) is never supported.
pub const MAX_FRAME_LEN: u32 = 256 * 1024;

/// Bind the socket, harden its permissions, and serve until stopped.
///
/// The socket directory is created 0700 and the socket file restricted to 0600;
/// the initial `create`/`bind` runs under an owner-only umask so the modes never
/// pass through a wider window. An existing socket that a process answers on
/// makes this call fail with [`SshAgentError::AlreadyServing`]; a stale socket
/// file is replaced.
///
/// Serving ends on a `bogita-stop@bogita` extension or on a failed accept loop.
/// Both paths unlink the socket file immediately, drain in-flight sessions for at
/// most [`GRACEFUL_DRAIN`], and only then resolve. A restart may therefore bind
/// the path while this daemon is still draining; the guard removes the file only
/// while it is still the one this daemon bound.
pub async fn serve(
    ring: SharedKeyRing,
    socket_path: impl AsRef<Path>,
) -> Result<(), SshAgentError> {
    serve_with(ring, socket_path.as_ref(), Arc::new(Control::new())).await
}

/// Serve until a stop arrives through the control extensions, SIGINT, or SIGHUP.
///
/// The daemon child uses this entry point: closing the terminal that started it
/// must reach the same drain-and-unlink path as an explicit stop.
pub async fn serve_with_signals(
    ring: SharedKeyRing,
    socket_path: impl AsRef<Path>,
) -> Result<(), SshAgentError> {
    signals::install()?;
    let control = Arc::new(Control::new());
    let watcher = tokio::spawn(watch_termination(Arc::clone(&control)));

    let served = serve_with(ring, socket_path.as_ref(), control).await;

    watcher.abort();
    served
}

async fn serve_with(
    ring: SharedKeyRing,
    socket_path: &Path,
    control: Arc<Control>,
) -> Result<(), SshAgentError> {
    let private_umask = socket::PrivateUmask::set();
    socket::prepare(socket_path)?;
    let listener =
        UnixListener::bind(socket_path).map_err(|e| SshAgentError::Socket(e.to_string()))?;
    socket::harden_socket(socket_path)?;
    drop(private_umask);
    let guard = socket::SocketGuard::new(socket_path)?;

    let agent = GatedAgent::new(ring, Arc::clone(&control));
    let served = tokio::select! {
        served = listen(CappedListener::new(listener), agent) => Some(served),
        () = control.stopped() => None,
    };

    // Unlink before the drain so a restart can bind the path while this daemon
    // still finishes in-flight sessions. The guard repeats the removable check
    // on drop for the paths that never reach this line (accept error).
    guard.remove();
    control.drain(GRACEFUL_DRAIN).await;

    if let Some(served) = served {
        served.map_err(|e| SshAgentError::Serve(e.to_string()))?;
    }
    Ok(())
}

/// Map a termination signal onto the stop path.
async fn watch_termination(control: Arc<Control>) {
    while !control.is_stopping() {
        if signals::termination_requested() {
            control.request_stop();
            return;
        }
        tokio::time::sleep(SIGNAL_POLL_INTERVAL).await;
    }
}

// endregion: --- Server

// region:    --- Frame cap

/// Width of the RFC 9987 § 5 `uint32` frame length prefix.
const FRAME_PREFIX_LEN: usize = 4;

/// Listener that wraps every accepted stream in a [`CappedStream`].
///
/// The vendored codec has no frame-length knob, so the cap sits in front of it. A prefix
/// above [`MAX_FRAME_LEN`] makes the stream error, the session loop ends as it does for any
/// decode error, and the connection closes without a wire reply — the close branch of
/// RFC 9987 § 6.
#[derive(Debug)]
struct CappedListener(UnixListener);

impl CappedListener {
    fn new(listener: UnixListener) -> Self {
        Self(listener)
    }
}

#[ssh_agent_lib::async_trait]
impl ListeningSocket for CappedListener {
    type Stream = CappedStream;

    async fn accept(&mut self) -> io::Result<CappedStream> {
        let (stream, _addr) = self.0.accept().await?;
        Ok(CappedStream::new(stream))
    }
}

/// Socket that refuses a frame whose length prefix exceeds [`MAX_FRAME_LEN`].
///
/// The first four bytes are held back until all of them have arrived, so the cap needs only
/// the prefix and never the body. A client that sends fewer than four bytes keeps waiting,
/// exactly as it does inside the codec. Every later read passes straight through.
#[derive(Debug)]
struct CappedStream {
    inner: UnixStream,
    prefix: Vec<u8>,
    capped: bool,
}

impl CappedStream {
    fn new(inner: UnixStream) -> Self {
        Self {
            inner,
            prefix: Vec::with_capacity(FRAME_PREFIX_LEN),
            capped: false,
        }
    }

    /// The wrapped stream, for the accept-time peer gate.
    fn inner(&self) -> &UnixStream {
        &self.inner
    }

    /// Buffer the four prefix bytes, then refuse a length above the cap.
    fn admit_prefix(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.prefix.len() < FRAME_PREFIX_LEN {
            let wanted = FRAME_PREFIX_LEN - self.prefix.len();
            let mut scratch = [0u8; FRAME_PREFIX_LEN];
            let mut read = ReadBuf::new(&mut scratch[..wanted]);
            match Pin::new(&mut self.inner).poll_read(cx, &mut read) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => {}
            }
            if read.filled().is_empty() {
                return Poll::Ready(Ok(()));
            }
            self.prefix.extend_from_slice(read.filled());
        }

        self.capped = true;
        let mut prefix = [0u8; FRAME_PREFIX_LEN];
        prefix.copy_from_slice(&self.prefix);
        let length = u32::from_be_bytes(prefix);
        if length > MAX_FRAME_LEN {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("agent frame prefix {length} exceeds the {MAX_FRAME_LEN}-byte cap"),
            )));
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for CappedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        if !this.capped {
            match this.admit_prefix(cx) {
                Poll::Ready(Ok(())) => {}
                other => return other,
            }
            if this.prefix.is_empty() {
                // EOF before a full prefix; the codec reads the same EOF and closes.
                return Poll::Ready(Ok(()));
            }
        }

        if this.prefix.is_empty() {
            return Pin::new(&mut this.inner).poll_read(cx, buf);
        }

        let n = buf.remaining().min(this.prefix.len());
        buf.put_slice(&this.prefix[..n]);
        this.prefix.drain(..n);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for CappedStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

// endregion: --- Frame cap

// region:    --- Agent

/// Session factory that applies the peer-uid gate to every connection.
pub struct GatedAgent {
    ring: SharedKeyRing,
    own_uid: u32,
    control: Arc<Control>,
}

impl GatedAgent {
    fn new(ring: SharedKeyRing, control: Arc<Control>) -> Self {
        Self {
            ring,
            own_uid: socket::current_uid(),
            control,
        }
    }

    /// True when the peer runs as the same user as the daemon.
    fn admits(&self, stream: &UnixStream) -> bool {
        match socket::peer_uid(stream) {
            Ok(peer_uid) => socket::peer_matches(peer_uid, self.own_uid),
            Err(e) => {
                eprintln!("warning: agent rejected a connection without peer credentials: {e}");
                false
            }
        }
    }
}

impl Agent<CappedListener> for GatedAgent {
    fn new_session(&mut self, stream: &CappedStream) -> impl Session {
        BogitaSession::tracked(
            self.ring.clone(),
            self.admits(stream.inner()),
            Arc::clone(&self.control),
        )
    }
}

// endregion: --- Agent

// region:    --- Session

/// One agent connection: the shared ring, the peer-gate verdict, and the
/// in-flight ticket the drain counts.
///
/// A session that does not pass the peer gate answers FAILURE to every request
/// and never touches the ring.
pub struct BogitaSession {
    ring: SharedKeyRing,
    permits: bool,
    active: SessionTicket,
}

impl BogitaSession {
    /// A session that is not attached to a serve loop.
    ///
    /// It answers the two control extensions, but a stop request signals only
    /// its own control handle, so no daemon shuts down.
    pub fn new(ring: SharedKeyRing, permits: bool) -> Self {
        Self::tracked(ring, permits, Arc::new(Control::new()))
    }

    fn tracked(ring: SharedKeyRing, permits: bool, control: Arc<Control>) -> Self {
        Self {
            ring,
            permits,
            active: SessionTicket::new(control),
        }
    }

    fn ring(&self) -> MutexGuard<'_, crate::agent::keyring::KeyRing> {
        self.ring.lock().expect("key ring mutex poisoned")
    }
}

#[ssh_agent_lib::async_trait]
impl Session for BogitaSession {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        if !self.permits {
            return Err(AgentError::Failure);
        }

        let mut ring = self.ring();
        if ring.is_locked() {
            return Err(failure(SshAgentError::Locked));
        }

        let identities: Vec<Identity> = ring
            .entries()
            .iter()
            .map(|entry| Identity {
                credential: PublicCredential::Key(entry.public_key().clone()),
                comment: entry.comment().to_string(),
            })
            .collect();

        // A served request counts against the inactivity cap; a refusal does not.
        ring.note_request();
        Ok(identities)
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        if !self.permits {
            return Err(AgentError::Failure);
        }

        let mut ring = self.ring();
        let signature = ring
            .sign(
                request.credential.key_data(),
                &request.data,
                rsa_hash_alg(request.flags),
            )
            .map_err(failure)?;
        ring.note_request();
        Ok(signature)
    }

    async fn extension(&mut self, extension: Extension) -> Result<Option<Extension>, AgentError> {
        if !self.permits {
            return Err(AgentError::ExtensionFailure);
        }

        match extension.name.as_str() {
            STOP_EXTENSION => {
                self.active.0.request_stop();
                Ok(None)
            }
            STATUS_EXTENSION => {
                let ring = self.ring();
                let payload = status_payload(ring.len(), ring.is_locked());
                Ok(Some(Extension {
                    name: extension.name,
                    details: payload.into_bytes().into(),
                }))
            }
            _ => Err(AgentError::ExtensionFailure),
        }
    }
}

// endregion: --- Session

// region:    --- Support

/// Map a daemon error onto the generic agent failure (FAILURE(5) on the wire).
fn failure(error: SshAgentError) -> AgentError {
    AgentError::other(error)
}

/// Counts a session as in flight and drops the count when the session ends.
struct SessionTicket(Arc<Control>);

impl SessionTicket {
    fn new(control: Arc<Control>) -> Self {
        control.session_started();
        Self(control)
    }
}

impl Drop for SessionTicket {
    fn drop(&mut self) {
        self.0.session_finished();
    }
}

// endregion: --- Support

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/agent/session.rs"]
mod session_test;

#[cfg(test)]
#[path = "../../test/agent/server_frame.rs"]
mod server_frame_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/server.rs"]
mod server_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/server_control.rs"]
mod server_control_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/server_lock.rs"]
mod server_lock_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/conformance.rs"]
mod conformance_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/security.rs"]
mod security_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/real_client.rs"]
mod real_client_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/agent/server_rsa.rs"]
mod server_rsa_test;
// endregion: --- Tests
