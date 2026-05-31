//! Crypto primitives for the vault.
//!
//! ## Threat model
//!
//! - Adversary has read access to the vault SQLite file (e.g. a stolen
//!   laptop drive) but **not** to the OS keychain.
//! - Adversary knows the algorithms (we publish them in this file).
//! - Adversary cannot tamper with our binary (out of scope — that's
//!   handled by binary signing / OS code-signing).
//!
//! ## Design
//!
//! - **KEK (master key)** — 32 random bytes, generated on `init`,
//!   stored in the OS keychain. Never written to disk.
//! - **DEK (per-domain key)** — derived from the KEK via HKDF-SHA256
//!   using the canonical domain string as `info`. Distinct domains
//!   *always* derive distinct DEKs; identical domains derive
//!   identical DEKs (so multiple entries under one domain share a key
//!   — fine because the AAD is per-entry).
//! - **Cipher** — AES-256-GCM. 12-byte nonce, randomly sampled per
//!   ciphertext (96-bit random nonces are safe up to ~2³² messages
//!   per key per NIST SP 800-38D §8.3 — vastly more than any vault
//!   entry will ever see).
//! - **AAD** — `entry_kind || ":" || domain || ":" || sub_key`,
//!   binding the ciphertext to its row so swapping ciphertexts between
//!   rows fails GCM authentication.
//!
//! All primitives below take borrowed `&MasterKey` / `&Dek` to avoid
//! cloning key material around the call graph.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Result, VaultError};
use crate::keystore::MasterKey;

/// HKDF salt baked into the binary. This is a public constant — its
/// purpose is to **domain-separate** our derivations from any other
/// HKDF use of the same KEK in this app, not to act as a secret.
const KDF_SALT: &[u8] = b"genatp-vault-kdf-v1";

/// 32-byte derived key for a single domain. Wraps a `[u8; 32]` and
/// zeroizes on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Dek {
    bytes: [u8; 32],
}

impl Dek {
    /// Borrow as raw bytes (used only by GCM seal/open in this module).
    fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

impl std::fmt::Debug for Dek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dek([REDACTED])")
    }
}

/// Derive the DEK for `domain` from the master key.
///
/// HKDF-SHA256 is deterministic — calling this twice with the same
/// inputs returns identical bytes. That property is what lets us
/// re-open a vault after a process restart without persisting any
/// per-domain key material on disk.
#[must_use]
pub fn derive_dek(kek: &MasterKey, domain: &str) -> Dek {
    let hk = Hkdf::<Sha256>::new(Some(KDF_SALT), kek.as_bytes());
    let mut out = [0u8; 32];
    let info = format!("genatp-vault/dek/{domain}");
    hk.expand(info.as_bytes(), &mut out)
        .expect("HKDF-SHA256 expand to 32 bytes never fails");
    Dek { bytes: out }
}

/// Generate a fresh 32-byte master key from the OS RNG.
#[must_use]
pub fn generate_master_key() -> MasterKey {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    MasterKey::from_bytes(buf)
}

/// Sealed envelope: ciphertext + nonce, ready for SQLite columns.
///
/// We keep the wire format explicit so we can audit it in a single
/// place — column writes go through here and nowhere else.
#[derive(Debug, Clone)]
pub struct Sealed {
    /// 12-byte AES-GCM nonce.
    pub nonce: [u8; 12],
    /// Ciphertext + 16-byte trailing GCM tag.
    pub ciphertext: Vec<u8>,
}

impl Sealed {
    /// Length of the nonce in bytes (12 for AES-GCM).
    pub const NONCE_LEN: usize = 12;
}

/// Encrypt `plaintext` under `dek`, binding the ciphertext to `aad`.
///
/// `aad` is authenticated but not encrypted — typically the caller
/// constructs it as `entry_kind:domain:sub_key`. Mismatched AAD on
/// open returns [`VaultError::Decrypt`].
///
/// # Errors
/// Returns [`VaultError::Decrypt`] if the underlying cipher fails to
/// allocate or encrypt (in practice, only OOM).
pub fn seal(dek: &Dek, aad: &[u8], plaintext: &[u8]) -> Result<Sealed> {
    let key = Key::<Aes256Gcm>::from_slice(dek.as_bytes());
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Decrypt)?;
    let mut nonce_arr = [0u8; 12];
    nonce_arr.copy_from_slice(nonce.as_slice());
    Ok(Sealed {
        nonce: nonce_arr,
        ciphertext,
    })
}

/// Decrypt a [`Sealed`] envelope.
///
/// On any failure (wrong DEK, tampered ciphertext, mismatched AAD)
/// returns [`VaultError::Decrypt`] — the variant deliberately does
/// *not* distinguish causes to avoid side-channel leaks.
///
/// # Errors
/// As above.
pub fn open(dek: &Dek, aad: &[u8], sealed: &Sealed) -> Result<Vec<u8>> {
    let key = Key::<Aes256Gcm>::from_slice(dek.as_bytes());
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&sealed.nonce);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &sealed.ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::Decrypt)
}

/// Zeroize a buffer on drop. Used at call sites that hold plaintext
/// briefly between deserialisation and re-use.
pub fn zeroize_buf(buf: &mut Vec<u8>) {
    buf.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kek() -> MasterKey {
        MasterKey::from_bytes([0x42u8; 32])
    }

    #[test]
    fn derive_is_deterministic_for_same_domain() {
        let a = derive_dek(&kek(), "github.com");
        let b = derive_dek(&kek(), "github.com");
        assert_eq!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn derive_differs_for_different_domains() {
        let a = derive_dek(&kek(), "github.com");
        let b = derive_dek(&kek(), "gitlab.com");
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn seal_and_open_round_trip_with_matching_aad() {
        let dek = derive_dek(&kek(), "github.com");
        let aad = b"cookie:github.com:session";
        let pt = b"hello-secret";
        let sealed = seal(&dek, aad, pt).unwrap();
        let out = open(&dek, aad, &sealed).unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn open_with_wrong_dek_fails() {
        let dek_a = derive_dek(&kek(), "github.com");
        let dek_b = derive_dek(&kek(), "gitlab.com");
        let sealed = seal(&dek_a, b"aad", b"x").unwrap();
        let err = open(&dek_b, b"aad", &sealed).unwrap_err();
        assert!(matches!(err, VaultError::Decrypt));
    }

    #[test]
    fn open_with_wrong_aad_fails() {
        let dek = derive_dek(&kek(), "github.com");
        let sealed = seal(&dek, b"aad-1", b"x").unwrap();
        let err = open(&dek, b"aad-2", &sealed).unwrap_err();
        assert!(matches!(err, VaultError::Decrypt));
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let dek = derive_dek(&kek(), "github.com");
        let mut sealed = seal(&dek, b"aad", b"hello").unwrap();
        // Flip a single ciphertext bit.
        sealed.ciphertext[0] ^= 0x01;
        let err = open(&dek, b"aad", &sealed).unwrap_err();
        assert!(matches!(err, VaultError::Decrypt));
    }

    #[test]
    fn tampered_nonce_fails_authentication() {
        let dek = derive_dek(&kek(), "github.com");
        let mut sealed = seal(&dek, b"aad", b"hello").unwrap();
        sealed.nonce[0] ^= 0x01;
        let err = open(&dek, b"aad", &sealed).unwrap_err();
        assert!(matches!(err, VaultError::Decrypt));
    }

    #[test]
    fn sealed_ciphertext_does_not_contain_plaintext_bytes() {
        let dek = derive_dek(&kek(), "github.com");
        let pt = b"VERY-DISTINCTIVE-PLAINTEXT";
        let sealed = seal(&dek, b"aad", pt).unwrap();
        assert!(
            !sealed.ciphertext.windows(pt.len()).any(|w| w == pt),
            "plaintext leaked into ciphertext"
        );
    }

    #[test]
    fn nonces_differ_across_two_seals_of_same_payload() {
        let dek = derive_dek(&kek(), "github.com");
        let s1 = seal(&dek, b"aad", b"x").unwrap();
        let s2 = seal(&dek, b"aad", b"x").unwrap();
        assert_ne!(s1.nonce, s2.nonce, "nonces must be unique per-seal");
        assert_ne!(
            s1.ciphertext, s2.ciphertext,
            "AES-GCM should produce distinct ciphertexts for fresh nonces"
        );
    }

    #[test]
    fn generate_master_key_is_high_entropy() {
        let a = generate_master_key();
        let b = generate_master_key();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }
}
