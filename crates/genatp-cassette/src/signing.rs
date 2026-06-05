//! Ed25519 signing and verification for Action Cassettes.
//!
//! ## Key management
//!
//! Project signing keys live at `~/.genatp/keys/`:
//!
//! ```text
//! ~/.genatp/keys/
//! ├── signing.key         # 32-byte ed25519 secret (base64 wrapped)
//! └── signing.pub         # 32-byte ed25519 public key (hex)
//! ```
//!
//! The secret key file is `0600`-permissioned on Unix. On Windows the
//! ACL inherits the user's home directory permissions.
//!
//! ## Security invariants
//!
//! - Secret key bytes are `zeroize`d on drop.
//! - Debug impls never print key material.
//! - Signature verification is constant-time (`ed25519-dalek` default).
//! - A failed verification returns [`CassetteError::SignatureInvalid`] —
//!   deliberately vague per vault convention.

use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use tracing::{debug, info};

use crate::error::{CassetteError, Result};

/// Subdirectory under the genatp data dir for signing keys.
const KEYS_DIR: &str = "keys";
/// Filename for the secret key.
const SECRET_KEY_FILE: &str = "signing.key";
/// Filename for the public key.
const PUBLIC_KEY_FILE: &str = "signing.pub";

/// Resolve the genatp data directory: `~/.genatp/`.
///
/// # Errors
/// Returns [`CassetteError::Storage`] if the home directory is unresolvable.
pub fn genatp_data_dir() -> Result<PathBuf> {
    let proj = directories::ProjectDirs::from("dev", "genatp", "genatp").ok_or_else(|| {
        CassetteError::Storage("cannot resolve home directory for key storage".into())
    })?;
    Ok(proj.data_dir().to_path_buf())
}

/// Full path to the keys directory: `~/.genatp/keys/`.
///
/// # Errors
/// Forwards from [`genatp_data_dir`].
pub fn keys_dir() -> Result<PathBuf> {
    Ok(genatp_data_dir()?.join(KEYS_DIR))
}

/// Generate a new ed25519 keypair and persist it to `~/.genatp/keys/`.
///
/// If keys already exist, this is a no-op and returns the existing
/// public key. To force regeneration, delete the key files first.
///
/// # Errors
/// Returns I/O errors on directory creation or file writes.
pub fn ensure_keypair() -> Result<VerifyingKey> {
    let dir = keys_dir()?;
    let secret_path = dir.join(SECRET_KEY_FILE);
    let public_path = dir.join(PUBLIC_KEY_FILE);

    if secret_path.exists() && public_path.exists() {
        debug!("signing keypair already exists");
        return load_public_key(&public_path);
    }

    info!("generating new ed25519 signing keypair");
    fs::create_dir_all(&dir)?;

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    // Write secret key as base64.
    let secret_b64 = base64_encode(signing_key.as_bytes());
    fs::write(&secret_path, secret_b64.as_bytes())?;

    // Restrict permissions on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret_path, fs::Permissions::from_mode(0o600))?;
    }

    // Write public key as hex.
    let pub_hex = hex::encode(verifying_key.as_bytes());
    fs::write(&public_path, pub_hex.as_bytes())?;

    info!(public_key = %pub_hex, "keypair generated");
    Ok(verifying_key)
}

/// Load the project signing key from `~/.genatp/keys/signing.key`.
///
/// # Errors
/// Returns [`CassetteError::SigningKey`] if the file is missing or malformed.
pub fn load_signing_key() -> Result<SigningKey> {
    let path = keys_dir()?.join(SECRET_KEY_FILE);
    load_signing_key_from(&path)
}

/// Load a signing key from an arbitrary path (for testing).
pub fn load_signing_key_from(path: &Path) -> Result<SigningKey> {
    let content = fs::read_to_string(path).map_err(|e| {
        CassetteError::SigningKey(format!(
            "cannot read signing key at {}: {e}",
            path.display()
        ))
    })?;
    let bytes = base64_decode(content.trim())
        .map_err(|e| CassetteError::SigningKey(format!("signing key base64 decode failed: {e}")))?;
    if bytes.len() != 32 {
        return Err(CassetteError::SigningKey(format!(
            "signing key must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&key_bytes))
}

/// Load the project public key from `~/.genatp/keys/signing.pub`.
///
/// # Errors
/// Returns [`CassetteError::SigningKey`] if the file is missing or malformed.
pub fn load_verifying_key() -> Result<VerifyingKey> {
    let path = keys_dir()?.join(PUBLIC_KEY_FILE);
    load_public_key(&path)
}

/// Load a public key from an arbitrary path (for testing).
fn load_public_key(path: &Path) -> Result<VerifyingKey> {
    let content = fs::read_to_string(path).map_err(|e| {
        CassetteError::SigningKey(format!("cannot read public key at {}: {e}", path.display()))
    })?;
    let bytes = hex::decode(content.trim())
        .map_err(|e| CassetteError::SigningKey(format!("public key hex decode failed: {e}")))?;
    if bytes.len() != 32 {
        return Err(CassetteError::SigningKey(format!(
            "public key must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| CassetteError::SigningKey(format!("invalid public key: {e}")))
}

/// Sign a payload with the given signing key. Returns the 64-byte signature.
pub fn sign(key: &SigningKey, payload: &[u8]) -> Signature {
    key.sign(payload)
}

/// Verify a signature against a public key and payload.
///
/// # Errors
/// Returns [`CassetteError::SignatureInvalid`] on any verification failure.
pub fn verify(key: &VerifyingKey, payload: &[u8], signature: &Signature) -> Result<()> {
    key.verify(payload, signature)
        .map_err(|_| CassetteError::SignatureInvalid)
}

/// Hex-encode a verifying key.
#[must_use]
pub fn public_key_hex(key: &VerifyingKey) -> String {
    hex::encode(key.as_bytes())
}

/// Parse a verifying key from hex.
///
/// # Errors
/// Returns [`CassetteError::SigningKey`] if the hex is malformed.
pub fn public_key_from_hex(hex_str: &str) -> Result<VerifyingKey> {
    let bytes =
        hex::decode(hex_str).map_err(|e| CassetteError::SigningKey(format!("hex decode: {e}")))?;
    if bytes.len() != 32 {
        return Err(CassetteError::SigningKey(format!(
            "public key must be 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| CassetteError::SigningKey(format!("invalid public key: {e}")))
}

// ── Base64 helpers ─────────────────────────────────────────────────────

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let verifying = key.verifying_key();
        let payload = b"hello cassette world";
        let sig = sign(&key, payload);
        verify(&verifying, payload, &sig).expect("verification must succeed");
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let key = SigningKey::generate(&mut OsRng);
        let verifying = key.verifying_key();
        let payload = b"original data";
        let sig = sign(&key, payload);
        let err = verify(&verifying, b"tampered data", &sig).unwrap_err();
        assert!(matches!(err, CassetteError::SignatureInvalid));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let key1 = SigningKey::generate(&mut OsRng);
        let key2 = SigningKey::generate(&mut OsRng);
        let payload = b"test";
        let sig = sign(&key1, payload);
        let err = verify(&key2.verifying_key(), payload, &sig).unwrap_err();
        assert!(matches!(err, CassetteError::SignatureInvalid));
    }

    #[test]
    fn public_key_hex_roundtrip() {
        let key = SigningKey::generate(&mut OsRng);
        let verifying = key.verifying_key();
        let hex_str = public_key_hex(&verifying);
        let recovered = public_key_from_hex(&hex_str).unwrap();
        assert_eq!(verifying, recovered);
    }

    #[test]
    fn keypair_persistence_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let secret_path = tmp.path().join("signing.key");
        let public_path = tmp.path().join("signing.pub");

        // Generate and write.
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let secret_b64 = base64_encode(signing_key.as_bytes());
        fs::write(&secret_path, secret_b64.as_bytes()).unwrap();
        let pub_hex = hex::encode(verifying_key.as_bytes());
        fs::write(&public_path, pub_hex.as_bytes()).unwrap();

        // Read back.
        let loaded = load_signing_key_from(&secret_path).unwrap();
        assert_eq!(loaded.verifying_key(), verifying_key);
        let loaded_pub = load_public_key(&public_path).unwrap();
        assert_eq!(loaded_pub, verifying_key);
    }

    #[test]
    fn invalid_hex_public_key_errors() {
        let err = public_key_from_hex("not-hex").unwrap_err();
        assert!(matches!(err, CassetteError::SigningKey(_)));
    }

    #[test]
    fn wrong_length_public_key_errors() {
        let err = public_key_from_hex("aabbccdd").unwrap_err();
        assert!(matches!(err, CassetteError::SigningKey(_)));
    }
}
