//! Hidden git signing helper — the `gpg.ssh.program` contract.
//!
//! git runs `gpg.ssh.program` for both signing and verifying SSH signatures.
//! The sign shape is answered from the vault; every other shape is forwarded
//! verbatim to `ssh-keygen`, so `allowed_signers` handling stays ssh-keygen's.
//!
//! Identity gate: the helper reads the staged identity from the keychain and
//! never stores, deletes, or prompts for it.

use bogita_core::agent::KeyRing;
use bogita_core::domain::{AgeIdentity, Entry, FieldType, SshKeyMaterial};
use bogita_core::error::Error as VaultError;
use bogita_core::ports::{KeychainStore, Storage};
use bogita_core::session::Session;
use bogita_core::vault::registry::VaultRegistry;
use ssh_key::{HashAlg, LineEnding, PrivateKey, PublicKey, SshSig};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

// region:    --- Constants

/// The sign shape git builds in `gpg-interface.c:1105-1112` (git 2.56.0):
///
/// ```text
/// -Y sign -n git -f <key-file> [-U] <data-file>
/// ```
///
/// `-U` appears only when `user.signingKey` holds a literal key line
/// (`gpg-interface.c:1072`): git writes the line to a temporary key file and
/// passes `-U`. When the value names a file, git interpolates the path as a
/// file path (`:1089`) and omits `-U`. The same two shapes were captured on
/// this machine with `GIT_TRACE=1` (git 2.56.0, OpenSSH 10.6p1).
///
/// Every other shape — the verify stages at `gpg-interface.c:504-575`
/// (`-Y find-principals`, `-Y check-novalidate` with the payload on stdin,
/// `-Y verify`) and any `-Y sign` invocation carrying an option git never
/// sends — is forwarded to `ssh-keygen` unchanged. The verify stages carry
/// `-Overify-time=<stamp>` as a single argv element, so an option the parser
/// does not expect forces the forward path instead of being dropped.
pub const GIT_SIGN_ARGV: &[&str] = &[
    "-Y",
    "sign",
    "-n",
    "git",
    "-f",
    "<key-file>",
    "-U",
    "<data-file>",
];

/// Printed when the keychain holds no identity. Nothing else may appear on
/// the helper's stderr for this case.
pub const LOCKED_MESSAGE: &str = "vault is locked; run bogita unlock";

/// Printed when no stored SSH key matches the requested public key.
pub const NO_MATCH_MESSAGE: &str = "no vault key matches user.signingKey";

/// Printed by the default-key subcommand when the agent serves no key.
pub const NO_SERVABLE_MESSAGE: &str = "no agent-servable keys";

/// Printed by the default-key subcommand when more than one key is servable.
pub const MULTIPLE_SERVABLE_MESSAGE: &str = "multiple agent-servable keys; set user.signingKey";

/// Hash algorithm git's signatures use (ssh-keygen's sshsig default).
const SIGN_HASH_ALG: HashAlg = HashAlg::Sha512;

// endregion: --- Constants

// region:    --- Types

/// What a helper invocation means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperAction {
    /// The captured sign shape: answer it from the vault.
    Sign(SignRequest),
    /// Anything else: run `ssh-keygen` with these arguments unchanged.
    Forward(Vec<String>),
}

/// A parsed `-Y sign` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignRequest {
    /// Signature namespace git asked for (`git` in the captured shape).
    pub namespace: String,
    /// File holding the public key line the signer must use.
    pub key_file: PathBuf,
    /// File holding the data to sign; the armor goes to `<data-file>.sig`.
    pub data_file: PathBuf,
    /// git's `-U`: sign with the key named by its public half. The vault key is
    /// the signing key, so the flag is accepted and does not change the path.
    pub use_agent: bool,
}

/// Why the helper refused to work.
#[derive(Debug)]
pub enum HelperError {
    /// No staged identity: the vault is locked.
    Locked,
    /// The requested public key is not in any vault.
    NoMatchingKey,
    /// The agent serves no key.
    NoAgentServableKeys,
    /// The agent serves more than one key.
    MultipleAgentServableKeys,
    /// A helper input could not be read.
    Read { path: PathBuf, source: io::Error },
    /// The signature could not be written.
    Write { path: PathBuf, source: io::Error },
    /// The key material cannot be used.
    Key(String),
    /// `ssh-keygen` could not be spawned.
    Spawn(String),
    /// The vault read failed.
    Vault(VaultError),
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => f.write_str(LOCKED_MESSAGE),
            Self::NoMatchingKey => f.write_str(NO_MATCH_MESSAGE),
            Self::NoAgentServableKeys => f.write_str(NO_SERVABLE_MESSAGE),
            Self::MultipleAgentServableKeys => f.write_str(MULTIPLE_SERVABLE_MESSAGE),
            Self::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Write { path, source } => write!(f, "cannot write {}: {source}", path.display()),
            Self::Key(message) => write!(f, "cannot use the SSH key: {message}"),
            Self::Spawn(message) => write!(f, "cannot run ssh-keygen: {message}"),
            Self::Vault(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HelperError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Vault(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VaultError> for HelperError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

// endregion: --- Types

// region:    --- Entry point

/// Gate the helper, then route the invocation.
///
/// The identity gate runs before routing and before any file work, so a locked
/// vault answers every helper invocation with the same message and exit code.
/// No stored key material is touched on the locked path.
pub fn prepare<K>(
    args: &[String],
    session: &Session<K>,
) -> Result<(AgeIdentity, HelperAction), HelperError>
where
    K: KeychainStore,
{
    let identity = gate(session)?;
    Ok((identity, route(args)))
}

/// Read the staged identity. Read-only: never stores, deletes, or prompts.
pub fn gate<K>(session: &Session<K>) -> Result<AgeIdentity, HelperError>
where
    K: KeychainStore,
{
    session
        .get_identity()
        .ok()
        .flatten()
        .ok_or(HelperError::Locked)
}

/// Map one argv to exactly one action.
pub fn route(args: &[String]) -> HelperAction {
    match parse_sign(args) {
        Some(request) => HelperAction::Sign(request),
        None => HelperAction::Forward(args.to_vec()),
    }
}

// endregion: --- Entry point

// region:    --- Sign path

/// Sign `request.data_file` with the vault key that matches `request.key_file`
/// and write the armor to `<data-file>.sig`.
pub async fn handle_sign<S>(
    identity: &AgeIdentity,
    request: &SignRequest,
    registry: VaultRegistry<S>,
) -> Result<(), HelperError>
where
    S: Storage,
{
    let entries = all_entries(&registry, identity).await?;
    sign_data(&entries, request)
}

/// Resolve the requested public key against `entries`, sign, and write the
/// signature next to the data file.
pub fn sign_data(entries: &[Entry], request: &SignRequest) -> Result<(), HelperError> {
    let requested = read_public_key(&request.key_file)?;
    let pem = match_key_pem(entries, &requested).ok_or(HelperError::NoMatchingKey)?;
    let data = read(&request.data_file)?;
    let signature = sign_bytes(&pem, &request.namespace, &data)?;
    let path = signature_path(&request.data_file);
    write(&path, signature.as_bytes())
}

/// The private PEM of the entry that holds the requested key, compared on the
/// wire form so a differing comment does not matter.
fn match_key_pem(entries: &[Entry], requested: &PublicKey) -> Option<String> {
    entries.iter().find_map(|entry| {
        let pem = private_pem(entry)?;
        let material = SshKeyMaterial::validate(pem).ok()?;
        let stored = PublicKey::from_openssh(material.public_line()).ok()?;
        (stored.key_data() == requested.key_data()).then(|| material.private_pem().to_string())
    })
}

/// Produce the SSHSIG armor for `data`.
fn sign_bytes(pem: &str, namespace: &str, data: &[u8]) -> Result<String, HelperError> {
    let key = PrivateKey::from_openssh(pem.as_bytes())
        .map_err(|error| HelperError::Key(error.to_string()))?;
    let signature: SshSig = key
        .sign(namespace, SIGN_HASH_ALG, data)
        .map_err(|error| HelperError::Key(error.to_string()))?;
    signature
        .to_pem(LineEnding::LF)
        .map_err(|error| HelperError::Key(error.to_string()))
}

// endregion: --- Sign path

// region:    --- Default key path

/// Print the public key line of the only agent-servable key.
///
/// Zero or several servable keys are an error: the helper never picks silently.
pub async fn handle_default_key<S>(
    identity: &AgeIdentity,
    registry: VaultRegistry<S>,
) -> Result<String, HelperError>
where
    S: Storage,
{
    let entries = all_entries(&registry, identity).await?;
    default_key_line(&entries)
}

/// The default-key rule over a set of entries.
pub fn default_key_line(entries: &[Entry]) -> Result<String, HelperError> {
    let mut ring = KeyRing::new();
    ring.load_entries(entries.iter().cloned());
    match ring.entries() {
        [] => Err(HelperError::NoAgentServableKeys),
        [key] => PublicKey::new(key.public_key().clone(), key.comment())
            .to_openssh()
            .map_err(|error| HelperError::Key(error.to_string())),
        _ => Err(HelperError::MultipleAgentServableKeys),
    }
}

// endregion: --- Default key path

// region:    --- Forward path

/// Run `ssh-keygen` with the given arguments and return its exit status.
///
/// Standard streams are inherited, so ssh-keygen reads the signed data from
/// the same stdin git wired up and writes its diagnostics to the same stderr.
pub fn forward(args: &[String]) -> Result<i32, HelperError> {
    let status = Command::new("ssh-keygen")
        .args(args)
        .status()
        .map_err(|error| HelperError::Spawn(error.to_string()))?;
    Ok(status.code().unwrap_or(1))
}

// endregion: --- Forward path

// region:    --- Support

/// Parse the captured sign shape; `None` means "forward".
fn parse_sign(args: &[String]) -> Option<SignRequest> {
    let mut arguments = args.iter();
    if arguments.next()? != "-Y" || arguments.next()? != "sign" {
        return None;
    }

    let mut namespace = None;
    let mut key_file = None;
    let mut data_file = None;
    let mut use_agent = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-n" => namespace = Some(arguments.next()?.clone()),
            "-f" => key_file = Some(PathBuf::from(arguments.next()?)),
            "-U" => use_agent = true,
            other if other.starts_with('-') => return None,
            _ if data_file.is_some() => return None,
            other => data_file = Some(PathBuf::from(other)),
        }
    }

    Some(SignRequest {
        namespace: namespace?,
        key_file: key_file?,
        data_file: data_file?,
        use_agent,
    })
}

/// Every entry of every vault, decrypted with `identity`.
async fn all_entries<S>(
    registry: &VaultRegistry<S>,
    identity: &AgeIdentity,
) -> Result<Vec<Entry>, HelperError>
where
    S: Storage,
{
    let mut entries = Vec::new();
    for vault in registry.list_vaults().await? {
        let service = registry.vault_service_for(&vault, identity.clone());
        entries.extend(service.list_entries(vault.id, None).await?);
    }
    Ok(entries)
}

/// The stored OpenSSH private key PEM, if the entry has one.
fn private_pem(entry: &Entry) -> Option<String> {
    entry
        .fields
        .iter()
        .find(|field| field.field_type == FieldType::SshPrivateKey)
        .map(|field| field.value.to_string())
}

/// The path git expects the armor at: exactly `<data-file>.sig`.
fn signature_path(data_file: &Path) -> PathBuf {
    let mut path = data_file.as_os_str().to_owned();
    path.push(".sig");
    PathBuf::from(path)
}

/// Parse the public key line git wrote to the key file.
fn read_public_key(path: &Path) -> Result<PublicKey, HelperError> {
    let contents = read_text(path)?;
    PublicKey::from_openssh(contents.trim()).map_err(|error| HelperError::Key(error.to_string()))
}

fn read(path: &Path) -> Result<Vec<u8>, HelperError> {
    std::fs::read(path).map_err(|source| HelperError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn read_text(path: &Path) -> Result<String, HelperError> {
    std::fs::read_to_string(path).map_err(|source| HelperError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, contents: &[u8]) -> Result<(), HelperError> {
    std::fs::write(path, contents).map_err(|source| HelperError::Write {
        path: path.to_path_buf(),
        source,
    })
}

// endregion: --- Support

// region:    --- Tests
#[cfg(test)]
#[path = "../../test/handlers/ssh_sign.rs"]
mod ssh_sign_test;

#[cfg(all(test, feature = "integration"))]
#[path = "../../test/handlers/ssh_sign_roundtrip.rs"]
mod ssh_sign_roundtrip_test;
// endregion: --- Tests
