//! Application-level encryption for the two most personal fields we hold:
//! `users.phone` and `users.mailing_address`.
//!
//! AES-256-GCM, a fresh random 96-bit nonce per value, stored with the
//! ciphertext as `enc1:` + hex(nonce ‖ ciphertext+tag). The key comes from the
//! `PII_ENCRYPTION_KEY` env var (64 hex chars) and lives only in process
//! memory. A database dump, a backup, or someone with read access to the
//! Postgres instance sees ciphertext only.
//!
//! What this does and does not buy: it protects data *at rest in the
//! database*. The running API holds the key and decrypts for admin and
//! self views, so it is not protection against a compromised API process.
//!
//! **Lazy by design.** [`Sealed`] stores the stored string as-is and only
//! decrypts when it is *serialised* — and serialisation only happens in the
//! admin and applicant-own views. Loading a `User` or `BookingRow` for any
//! other purpose (auth, calendar for an anonymous caller) never decrypts and
//! never needs the key.
//!
//! **Legacy values.** `users.phone` existed before this module, as plaintext
//! from the profile form. A stored value without the `enc1:` prefix is read as
//! plaintext and is re-written as ciphertext the next time it is saved. No
//! backfill is run, by instruction.
//!
//! A value that cannot be decrypted (wrong key, corrupt) reads as absent and
//! is logged *without* its contents — it must never take down sign-in.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use serde::{Serialize, Serializer};
use std::sync::OnceLock;

const PREFIX: &str = "enc1:";
const NONCE_LEN: usize = 12;

static KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Installs the process-wide key. First call wins; later calls are ignored,
/// which is what lets many tests in one process share a key.
pub fn init(key: [u8; 32]) {
    let _ = KEY.set(key);
}

/// Parses the 64-hex-char key from `PII_ENCRYPTION_KEY`.
pub fn parse_key(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 || !s.is_ascii() {
        anyhow::bail!(
            "PII_ENCRYPTION_KEY must be 64 hex characters (generate with: openssl rand -hex 32)"
        );
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("PII_ENCRYPTION_KEY must be hex"))?;
    }
    Ok(out)
}

/// The fixed key used ONLY when running locally with no key configured.
/// Deliberately public and useless: anything "encrypted" with it is not
/// protected, and `Config::from_env` refuses to use it outside local dev.
pub fn dev_key() -> [u8; 32] {
    *b"dulacmycamp-LOCAL-DEV-KEY-NOT-OK"
}

fn cipher() -> anyhow::Result<Aes256Gcm> {
    let key = KEY
        .get()
        .ok_or_else(|| anyhow::anyhow!("PII key not initialised"))?;
    Ok(Aes256Gcm::new(key.into()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

/// A value as stored in the database: ciphertext (or legacy plaintext).
/// Debug is redacted so it cannot reach a log line by accident.
#[derive(Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct Sealed(String);

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealed(..)")
    }
}

impl Sealed {
    /// Encrypts `plain` with a fresh random nonce.
    pub fn seal(plain: &str) -> anyhow::Result<Self> {
        let cipher = cipher()?;
        let mut nonce = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ct);
        Ok(Self(format!("{PREFIX}{}", to_hex(&blob))))
    }

    /// Decrypts. Legacy plaintext (no prefix) is returned as-is; an
    /// undecryptable value is `None` and is logged without its contents.
    pub fn open(&self) -> Option<String> {
        let Some(hex) = self.0.strip_prefix(PREFIX) else {
            return Some(self.0.clone()).filter(|s| !s.is_empty());
        };
        let opened = (|| {
            let blob = from_hex(hex)?;
            if blob.len() <= NONCE_LEN {
                return None;
            }
            let (nonce, ct) = blob.split_at(NONCE_LEN);
            let pt = cipher().ok()?.decrypt(Nonce::from_slice(nonce), ct).ok()?;
            String::from_utf8(pt).ok()
        })();
        if opened.is_none() {
            tracing::error!(
                "a stored PII value could not be decrypted (wrong PII_ENCRYPTION_KEY?)"
            );
        }
        opened
    }
}

/// Serialises the *decrypted* value. Only ever reached from the admin and
/// own-profile responses — see the module docs.
impl Serialize for Sealed {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.open() {
            Some(v) => s.serialize_str(&v),
            None => s.serialize_none(),
        }
    }
}

#[cfg(test)]
pub fn init_for_tests() {
    init(dev_key());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_not_plaintext() {
        init_for_tests();
        let s = Sealed::seal("+15551234567").unwrap();
        assert!(s.0.starts_with("enc1:"));
        assert!(!s.0.contains("5551234567"));
        assert_eq!(s.open().as_deref(), Some("+15551234567"));
    }

    #[test]
    fn same_value_gets_a_different_ciphertext_each_time() {
        init_for_tests();
        let a = Sealed::seal("123 Bayou Rd").unwrap();
        let b = Sealed::seal("123 Bayou Rd").unwrap();
        assert_ne!(a.0, b.0, "nonce must be random per value");
    }

    #[test]
    fn tampering_is_detected() {
        init_for_tests();
        let mut s = Sealed::seal("secret").unwrap();
        let last = s.0.pop().unwrap();
        s.0.push(if last == '0' { '1' } else { '0' });
        assert_eq!(s.open(), None);
    }

    #[test]
    fn legacy_plaintext_still_reads() {
        init_for_tests();
        assert_eq!(
            Sealed("985-555-0134".into()).open().as_deref(),
            Some("985-555-0134")
        );
    }

    #[test]
    fn debug_never_shows_the_value() {
        init_for_tests();
        let s = Sealed::seal("5551234567").unwrap();
        assert_eq!(format!("{s:?}"), "Sealed(..)");
    }

    #[test]
    fn key_parsing() {
        assert!(parse_key(&"ab".repeat(32)).is_ok());
        assert!(parse_key("short").is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
    }
}
