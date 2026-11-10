//! Tests for the agent frame cap (agent/server.rs)
//!
//! Pure stream plumbing over a `UnixStream` pair: no daemon, no filesystem, no keyring.
//! The cap needs only the four-byte RFC 9987 § 5 length prefix, so the cases here drive
//! `CappedStream` directly instead of a bound socket.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::agent::server::{CappedStream, MAX_FRAME_LEN};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Budget for a read that must complete.
const WAIT: Duration = Duration::from_secs(1);

/// Budget for a read that must not complete.
const SHORT_WAIT: Duration = Duration::from_millis(100);

// region:    --- Fixtures

/// A capped stream and its peer: the peer writes frames, the capped side reads them.
fn pair() -> Result<(CappedStream, UnixStream)> {
    let (served, client) = UnixStream::pair()?;
    Ok((CappedStream::new(served), client))
}

// endregion: --- Fixtures

// region:    --- Cap

#[tokio::test]
async fn test_agent_server_frame_cap_admits_a_prefix_at_the_cap() -> Result<()> {
    // -- Setup & Fixtures
    let (mut served, mut client) = pair()?;

    // -- Exec
    client.write_all(&MAX_FRAME_LEN.to_be_bytes()).await?;
    let mut prefix = [0u8; 4];
    tokio::time::timeout(WAIT, served.read_exact(&mut prefix)).await??;

    // -- Check
    assert_eq!(u32::from_be_bytes(prefix), MAX_FRAME_LEN);
    Ok(())
}

#[tokio::test]
async fn test_agent_server_frame_cap_refuses_a_prefix_over_the_cap() -> Result<()> {
    // -- Setup & Fixtures
    let (mut served, mut client) = pair()?;

    // -- Exec
    client.write_all(&(MAX_FRAME_LEN + 1).to_be_bytes()).await?;
    let mut prefix = [0u8; 4];
    let read = tokio::time::timeout(WAIT, served.read_exact(&mut prefix)).await?;

    // -- Check
    let error = read.err().ok_or("an over-cap prefix must not be read")?;
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    Ok(())
}

// endregion: --- Cap

// region:    --- Pass-through

#[tokio::test]
async fn test_agent_server_frame_cap_passes_a_small_frame_through() -> Result<()> {
    // -- Setup & Fixtures
    let (mut served, mut client) = pair()?;
    let frame = [0u8, 0, 0, 1, 11];

    // -- Exec
    client.write_all(&frame).await?;
    let mut read = [0u8; 5];
    tokio::time::timeout(WAIT, served.read_exact(&mut read)).await??;

    // -- Check
    assert_eq!(read, frame, "the cap must not alter the bytes it admits");
    Ok(())
}

#[tokio::test]
async fn test_agent_server_frame_cap_holds_a_partial_prefix_until_it_completes() -> Result<()> {
    // -- Setup & Fixtures
    let (mut served, mut client) = pair()?;
    let prefix = 1u32.to_be_bytes();

    // -- Exec & Check: half a prefix cannot be judged, so nothing is delivered
    client.write_all(&prefix[..2]).await?;
    let mut head = [0u8; 2];
    let early = tokio::time::timeout(SHORT_WAIT, served.read(&mut head)).await;
    assert!(
        early.is_err(),
        "a partial prefix must not be delivered, got {early:?}"
    );

    // -- Exec & Check: the second half completes the prefix and the frame follows
    client.write_all(&prefix[2..]).await?;
    client.write_all(&[11]).await?;
    let mut frame = [0u8; 5];
    tokio::time::timeout(WAIT, served.read_exact(&mut frame)).await??;
    assert_eq!(frame, [0u8, 0, 0, 1, 11]);
    Ok(())
}

// endregion: --- Pass-through
