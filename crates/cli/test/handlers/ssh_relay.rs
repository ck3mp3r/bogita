//! Relay tests: the git helper's sign and default-key shapes over [`AgentRelay`].
//!
//! Pure: the daemon is a mock, so no socket is opened and no database or keychain
//! is touched. The armor here is checked for shape and binding; the integration
//! suite checks a real daemon's Ed25519 and RSA replies.

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

use crate::handlers::ssh_relay::{default_key, sign, AgentRelay, RelayError, RSA_SHA2_512_FLAG};
use crate::handlers::ssh_sign::{
    HelperError, SignRequest, LOCKED_MESSAGE, MULTIPLE_SERVABLE_MESSAGE, NO_MATCH_MESSAGE,
    NO_SERVABLE_MESSAGE,
};
use bogita_core::domain::SshKeyMaterial;
use bogita_core::test_support::RSA_OPENSSH_PEM;
use ssh_key::{Algorithm, HashAlg, PublicKey, Signature, SshSig};
use std::path::{Path, PathBuf};

// region:    --- Fixtures

/// A daemon stand-in: it serves the keys it was given and answers one signature.
struct MockRelay {
    served: Vec<PublicKey>,
    unavailable: bool,
    refusal: bool,
    flags: Option<u32>,
    signatures: usize,
}

impl MockRelay {
    fn serving(materials: &[&SshKeyMaterial]) -> Result<Self> {
        let served = materials
            .iter()
            .map(|material| Ok(PublicKey::from_openssh(material.public_line())?))
            .collect::<Result<Vec<PublicKey>>>()?;
        Ok(Self {
            served,
            unavailable: false,
            refusal: false,
            flags: None,
            signatures: 0,
        })
    }
}

impl AgentRelay for MockRelay {
    fn identities(&mut self) -> std::result::Result<Vec<PublicKey>, RelayError> {
        if self.unavailable {
            return Err(RelayError::Unavailable);
        }
        Ok(self.served.clone())
    }

    fn sign(
        &mut self,
        _key: &PublicKey,
        _data: &[u8],
        flags: u32,
    ) -> std::result::Result<Signature, RelayError> {
        self.flags = Some(flags);
        self.signatures += 1;
        if self.refusal {
            return Err(RelayError::Refused);
        }
        Signature::new(Algorithm::Ed25519, vec![0u8; 64]).map_err(|_| RelayError::Unavailable)
    }
}

fn rsa_material() -> Result<SshKeyMaterial> {
    Ok(SshKeyMaterial::validate(RSA_OPENSSH_PEM)?)
}

fn write(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

fn sig_path(data: &Path) -> PathBuf {
    let mut path = data.as_os_str().to_owned();
    path.push(".sig");
    PathBuf::from(path)
}

/// The `-Y sign` request git sends, with the key line in a file of its own.
fn request(dir: &Path, material: &SshKeyMaterial) -> Result<(SignRequest, PathBuf)> {
    let key_file = dir.join("git_signing_key_tmp");
    let data_file = dir.join("git_signing_buffer_tmp");
    write(&key_file, material.public_line())?;
    write(&data_file, "commit payload\n")?;
    let request = SignRequest {
        namespace: "git".to_string(),
        key_file,
        data_file: data_file.clone(),
        use_agent: true,
    };
    Ok((request, data_file))
}

// endregion: --- Fixtures

// region:    --- Sign

#[test]
fn test_ssh_relay_sign_writes_the_armor_at_the_data_file() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut relay = MockRelay::serving(&[&material])?;
    let (request, data_file) = request(dir.path(), &material)?;
    let expected = sig_path(&data_file);
    assert!(!expected.exists());

    // -- Exec
    sign(&mut relay, &request)?;

    // -- Check: git finds the armor at <data-file>.sig and nowhere else
    assert!(expected.exists(), "the armor must sit at <data>.sig");
    assert!(!PathBuf::from(format!("{}.sig.sig", data_file.display())).exists());
    let armor = std::fs::read_to_string(&expected)?;
    let signature = SshSig::from_pem(armor.as_bytes())?;
    assert_eq!(
        signature.namespace(),
        "git",
        "the -n namespace binds the armor"
    );
    assert_eq!(signature.hash_alg(), HashAlg::Sha512);
    assert_eq!(
        signature.public_key(),
        PublicKey::from_openssh(material.public_line())?.key_data(),
        "the armor names the matched served key"
    );
    assert_eq!(
        relay.flags,
        Some(0),
        "an Ed25519 SIGN_REQUEST carries no RSA hash flag"
    );
    Ok(())
}

#[test]
fn test_ssh_relay_sign_asks_for_rsa_sha2_512() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = rsa_material()?;
    let mut relay = MockRelay::serving(&[&material])?;
    let (request, _) = request(dir.path(), &material)?;

    // -- Exec
    sign(&mut relay, &request)?;

    // -- Check
    assert_eq!(
        relay.flags,
        Some(RSA_SHA2_512_FLAG),
        "RSA asks the daemon for rsa-sha2-512"
    );
    assert_eq!(relay.signatures, 1);
    Ok(())
}

#[test]
fn test_ssh_relay_sign_matches_the_wire_form_not_the_comment() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut served = PublicKey::from_openssh(material.public_line())?;
    served.set_comment("rewritten by another client");
    let mut relay = MockRelay::serving(&[])?;
    relay.served = vec![served];
    let (request, data_file) = request(dir.path(), &material)?;

    // -- Exec
    sign(&mut relay, &request)?;

    // -- Check
    assert!(sig_path(&data_file).exists());
    Ok(())
}

#[test]
fn test_ssh_relay_sign_with_an_unserved_key_reports_no_match() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let served = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let requested = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut relay = MockRelay::serving(&[&served])?;
    let (request, data_file) = request(dir.path(), &requested)?;

    // -- Exec
    let result = sign(&mut relay, &request);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(()) => return Err("expected a missing-key error".into()),
    };
    assert!(matches!(error, HelperError::NoMatchingKey));
    assert_eq!(error.to_string(), NO_MATCH_MESSAGE);
    assert_eq!(relay.signatures, 0, "an unserved key asks for no signature");
    assert!(!sig_path(&data_file).exists(), "no armor on a miss");
    Ok(())
}

#[test]
fn test_ssh_relay_sign_maps_a_refusal_to_no_match() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut relay = MockRelay::serving(&[&material])?;
    relay.refusal = true;
    let (request, data_file) = request(dir.path(), &material)?;

    // -- Exec
    let result = sign(&mut relay, &request);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(()) => return Err("expected a refused signature".into()),
    };
    assert!(matches!(error, HelperError::NoMatchingKey));
    assert_eq!(error.to_string(), NO_MATCH_MESSAGE);
    assert!(!sig_path(&data_file).exists());
    Ok(())
}

#[test]
fn test_ssh_relay_sign_maps_an_unavailable_daemon_to_locked() -> Result<()> {
    // -- Setup & Fixtures
    let dir = tempfile::tempdir()?;
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut relay = MockRelay::serving(&[&material])?;
    relay.unavailable = true;
    let (request, data_file) = request(dir.path(), &material)?;

    // -- Exec
    let result = sign(&mut relay, &request);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(()) => return Err("expected a locked error".into()),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);
    assert!(!sig_path(&data_file).exists());
    Ok(())
}

// endregion: --- Sign

// region:    --- Default key

#[test]
fn test_ssh_relay_default_key_prints_the_only_served_key() -> Result<()> {
    // -- Setup & Fixtures
    let material = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let mut relay = MockRelay::serving(&[&material])?;

    // -- Exec
    let line = default_key(&mut relay)?;

    // -- Check
    assert_eq!(line, material.public_line());
    Ok(())
}

#[test]
fn test_ssh_relay_default_key_with_no_served_key_reports_the_contract_message() -> Result<()> {
    // -- Setup & Fixtures
    let mut relay = MockRelay::serving(&[])?;

    // -- Exec
    let result = default_key(&mut relay);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::NoAgentServableKeys));
    assert_eq!(error.to_string(), NO_SERVABLE_MESSAGE);
    Ok(())
}

#[test]
fn test_ssh_relay_default_key_with_several_keys_refuses_to_pick() -> Result<()> {
    // -- Setup & Fixtures
    let first = SshKeyMaterial::keygen_ed25519("alice@laptop")?;
    let second = SshKeyMaterial::keygen_ed25519("bob@work")?;
    let mut relay = MockRelay::serving(&[&first, &second])?;

    // -- Exec
    let result = default_key(&mut relay);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::MultipleAgentServableKeys));
    assert_eq!(error.to_string(), MULTIPLE_SERVABLE_MESSAGE);
    Ok(())
}

#[test]
fn test_ssh_relay_default_key_maps_an_unavailable_daemon_to_locked() -> Result<()> {
    // -- Setup & Fixtures
    let mut relay = MockRelay::serving(&[])?;
    relay.unavailable = true;

    // -- Exec
    let result = default_key(&mut relay);

    // -- Check
    let error = match result {
        Err(error) => error,
        Ok(line) => return Err(format!("expected an error, got '{line}'").into()),
    };
    assert!(matches!(error, HelperError::Locked));
    assert_eq!(error.to_string(), LOCKED_MESSAGE);
    Ok(())
}

// endregion: --- Default key
