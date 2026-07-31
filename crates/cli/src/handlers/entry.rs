//! Entry CLI command handlers (read-only: ls, get, search).

use crate::args::EntryCommands;
use bogita_core::crypto::AgeCrypto;
use bogita_core::domain::{AgeIdentity, Entry, FieldType, FieldValue};
use bogita_core::error::{DbError, Error, Result};
use bogita_core::storage::sqlite::SqliteStorage;
use bogita_core::vault::registry::VaultRegistry;

pub enum EntryOutput {
    /// `ls` / `search` — list of entries
    List(Vec<Entry>),
    /// `get` without --field — full entry
    Entry(Entry),
    /// `get --field` — single resolved field value
    Field(String),
}

/// Resolve the target vault: use `--vault <name>` if supplied, else the default vault.
async fn resolve_vault(
    vault_name: &Option<String>,
    registry: &VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
) -> Result<bogita_core::domain::Vault> {    match vault_name {
        Some(name) => {
            let vaults = registry.list_vaults().await?;
            vaults
                .into_iter()
                .find(|v| &v.name == name)
                .ok_or_else(|| Error::Database(DbError::Query(format!("vault '{name}' not found"))))
        }
        None => registry
            .default_vault()
            .await?
            .ok_or_else(|| Error::Database(DbError::Query("no default vault set".to_string()))),
    }
}

/// `bogita entry ls [--vault <name>]`
///
/// Lists all entries (all types) in the resolved vault.
pub async fn handle_ls(
    cmd: EntryCommands,
    registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    identity: AgeIdentity,
) -> Result<EntryOutput> {
    let vault_name = match &cmd {
        EntryCommands::Ls { vault } => vault,
        _ => unreachable!("handle_ls called with non-Ls command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, None).await?;
    Ok(EntryOutput::List(entries))
}

/// `bogita entry get <name> [--field <key>] [--vault <name>]`
///
/// Without `--field`: returns the full entry.
/// With `--field`: returns the resolved field value as a string.
///   - `TotpSecret` fields compute and return the live TOTP code instead of the raw secret.
pub async fn handle_get(
    cmd: EntryCommands,
    registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    identity: AgeIdentity,
) -> Result<EntryOutput> {
    let (name, field_key, vault_name) = match &cmd {
        EntryCommands::Get { name, field, vault } => (name, field, vault),
        _ => unreachable!("handle_get called with non-Get command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, None).await?;

    let entry = entries
        .into_iter()
        .find(|e| &e.name == name)
        .ok_or_else(|| Error::Database(DbError::Query(format!("entry '{name}' not found"))))?;

    match field_key {
        None => Ok(EntryOutput::Entry(entry)),
        Some(key) => {
            let field = entry.fields.iter().find(|f| &f.key == key).ok_or_else(|| {
                Error::Database(DbError::Query(format!(
                    "field '{key}' not found in entry '{name}'"
                )))
            })?;

            let value = if field.field_type == FieldType::TotpSecret {
                compute_totp(&entry, field)?
            } else {
                field_value_to_string(&field.value)
            };

            Ok(EntryOutput::Field(value))
        }
    }
}

/// `bogita entry search <query> [--vault <name>]`
///
/// Searches all entries in the resolved vault for the given query string.
pub async fn handle_search(
    cmd: EntryCommands,
    registry: VaultRegistry<SqliteStorage<AgeCrypto>, AgeCrypto>,
    identity: AgeIdentity,
) -> Result<EntryOutput> {
    let (query, vault_name) = match &cmd {
        EntryCommands::Search { query, vault } => (query, vault),
        _ => unreachable!("handle_search called with non-Search command"),
    };

    let vault = resolve_vault(vault_name, &registry).await?;
    let svc = registry.vault_service_for(&vault, identity);
    let entries = svc.list_entries(vault.id, Some(query)).await?;
    Ok(EntryOutput::List(entries))
}

/// Compute the current TOTP code from a TotpSecret field and sibling TOTP fields.
fn compute_totp(entry: &Entry, secret_field: &bogita_core::domain::Field) -> Result<String> {    let secret = field_value_to_string(&secret_field.value);

    let period = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::TotpPeriod)
        .and_then(|f| match &f.value {
            FieldValue::Number(n) => Some(*n as u64),
            FieldValue::Text(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(30);

    let digits = entry
        .fields
        .iter()
        .find(|f| f.field_type == FieldType::TotpDigits)
        .and_then(|f| match &f.value {
            FieldValue::Number(n) => Some(*n as u32),
            FieldValue::Text(s) => s.parse().ok(),
            _ => None,
        })
        .unwrap_or(6);

    totp_compute(&secret, period, digits)
        .map_err(|e| Error::Database(DbError::Query(format!("TOTP computation failed: {e}"))))
}

/// Compute a TOTP code using HMAC-SHA1 per RFC 6238.
fn totp_compute(
    secret_base32: &str,
    period: u64,
    digits: u32,
) -> std::result::Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let key = base32_decode(secret_base32)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let counter = timestamp / period;
    let code = hotp(&key, counter, digits);
    Ok(format!("{:0>width$}", code, width = digits as usize))
}

/// HOTP per RFC 4226.
fn hotp(key: &[u8], counter: u64, digits: u32) -> u32 {
    let msg = counter.to_be_bytes();
    let mac = hmac_sha1(key, &msg);
    let offset = (mac[19] & 0x0f) as usize;
    let code = u32::from_be_bytes([
        mac[offset] & 0x7f,
        mac[offset + 1],
        mac[offset + 2],
        mac[offset + 3],
    ]);
    code % 10u32.pow(digits)
}

/// HMAC-SHA1 — no external crate.
fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let h = sha1(key);
        k[..20].copy_from_slice(&h);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Vec::with_capacity(BLOCK + msg.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(msg);
    let inner_hash = sha1(&inner);
    let mut outer = Vec::with_capacity(BLOCK + 20);
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    sha1(&outer)
}

/// SHA-1 — pure Rust, no external crate.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        #[allow(clippy::needless_range_loop)]
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, &val) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

/// Decode a base32 string (RFC 4648, case-insensitive, padding optional).
fn base32_decode(input: &str) -> std::result::Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let input = input.to_uppercase().replace('=', "");
    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    let mut out = Vec::new();
    for ch in input.chars() {
        let val = ALPHABET
            .iter()
            .position(|&b| b == ch as u8)
            .ok_or_else(|| format!("invalid base32 character: {ch}"))? as u32;
        bits = (bits << 5) | val;
        bit_count += 5;
        if bit_count >= 8 {
            bit_count -= 8;
            out.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    Ok(out)
}

fn field_value_to_string(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(s) | FieldValue::Hidden(s) | FieldValue::Url(s) | FieldValue::Email(s) => {
            s.clone()
        }
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::Number(n) => n.to_string(),
        FieldValue::Date(ts) => ts.to_string(),
    }
}
