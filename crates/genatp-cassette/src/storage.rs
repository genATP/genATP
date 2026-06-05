//! Cassette filesystem storage.
//!
//! ## Directory layout
//!
//! ```text
//! ~/.genatp/cassettes/
//! └── github-login-repo-lookup/           # slug directory
//!     └── versions/
//!         ├── 1.0.0.cassette              # v1.0.0
//!         └── 1.1.0.cassette              # v1.1.0 (self-healed)
//! ```
//!
//! Cassette files are `zstd(canonical_json(Cassette)) + 64-byte ed25519 sig`.
//! The signature covers the zstd-compressed payload.

use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, SigningKey};
use tracing::{debug, info};

use crate::error::{CassetteError, Result};
use crate::schema::Cassette;
use crate::signing;

/// Default zstd compression level for cassettes.
const ZSTD_LEVEL: i32 = 3;

/// Size of an ed25519 signature in bytes.
const SIGNATURE_LEN: usize = 64;

/// Resolve the cassettes root directory: `~/.genatp/cassettes/`.
///
/// # Errors
/// Forwards from [`signing::genatp_data_dir`].
pub fn cassettes_dir() -> Result<PathBuf> {
    Ok(signing::genatp_data_dir()?.join("cassettes"))
}

/// Full path for a cassette file:
/// `~/.genatp/cassettes/<slug>/versions/<version>.cassette`.
#[must_use]
pub fn cassette_path(root: &Path, slug: &str, version: &str) -> PathBuf {
    root.join(slug)
        .join("versions")
        .join(format!("{version}.cassette"))
}

/// Save a cassette to disk — compress, sign, write.
///
/// Creates the directory tree if it doesn't exist.
///
/// # Errors
/// Returns I/O errors on directory creation or file writes, and
/// signing errors if the key is invalid.
pub fn save(cassette: &Cassette, root: &Path, signing_key: &SigningKey) -> Result<PathBuf> {
    let path = cassette_path(root, &cassette.header.slug, &cassette.header.version);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 1) Canonical JSON.
    let json = crate::canon::canonical_json(&serde_json::to_value(cassette)?);

    // 2) Zstd compress.
    let compressed = zstd::encode_all(json.as_slice(), ZSTD_LEVEL)
        .map_err(|e| CassetteError::Zstd(e.to_string()))?;

    // 3) Sign the compressed payload.
    let sig = signing::sign(signing_key, &compressed);

    // 4) Write: compressed || signature.
    let mut output = compressed;
    output.extend_from_slice(&sig.to_bytes());

    fs::write(&path, &output)?;
    info!(
        slug = %cassette.header.slug,
        version = %cassette.header.version,
        path = %path.display(),
        bytes = output.len(),
        "cassette saved"
    );
    Ok(path)
}

/// Load a cassette from disk — read, split signature, verify, decompress,
/// parse, verify hash chain.
///
/// # Errors
/// Returns errors for missing files, bad signatures, decompression
/// failures, parse failures, or broken hash chains.
pub fn load(path: &Path, verifying_key: &ed25519_dalek::VerifyingKey) -> Result<Cassette> {
    if !path.exists() {
        return Err(CassetteError::NotFound {
            path: path.to_path_buf(),
        });
    }

    let raw = fs::read(path)?;
    load_from_bytes(&raw, verifying_key)
}

/// Load a cassette from raw bytes (useful for testing without filesystem).
///
/// # Errors
/// Same as [`load`].
pub fn load_from_bytes(
    raw: &[u8],
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> Result<Cassette> {
    if raw.len() < SIGNATURE_LEN {
        return Err(CassetteError::SignatureInvalid);
    }

    // Split: payload || 64-byte signature.
    let (compressed, sig_bytes) = raw.split_at(raw.len() - SIGNATURE_LEN);
    let sig = Signature::from_slice(sig_bytes).map_err(|_| CassetteError::SignatureInvalid)?;

    // Verify signature on the compressed payload.
    signing::verify(verifying_key, compressed, &sig)?;

    // Decompress.
    let json = zstd::decode_all(compressed).map_err(|e| CassetteError::Zstd(e.to_string()))?;

    // Parse.
    let cassette: Cassette = serde_json::from_slice(&json)?;

    // Verify schema version.
    if cassette.header.schema_version != crate::schema::SCHEMA_VERSION {
        return Err(CassetteError::UnsupportedVersion {
            version: cassette.header.schema_version,
        });
    }

    // Verify hash chain.
    cassette.verify_hash_chain()?;

    debug!(
        slug = %cassette.header.slug,
        version = %cassette.header.version,
        steps = cassette.steps.len(),
        "cassette loaded and verified"
    );
    Ok(cassette)
}

/// List all slugs in the cassettes directory.
///
/// # Errors
/// Returns I/O errors on directory reads.
pub fn list_slugs(root: &Path) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut slugs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Only include dirs that have a versions/ subdirectory.
                if entry.path().join("versions").is_dir() {
                    slugs.push(name.to_string());
                }
            }
        }
    }
    slugs.sort();
    Ok(slugs)
}

/// List all versions available for a given slug, sorted ascending.
///
/// # Errors
/// Returns [`CassetteError::SlugNotFound`] if the slug directory doesn't exist.
pub fn list_versions(root: &Path, slug: &str) -> Result<Vec<String>> {
    let versions_dir = root.join(slug).join("versions");
    if !versions_dir.exists() {
        return Err(CassetteError::SlugNotFound {
            slug: slug.to_string(),
        });
    }

    let ext = format!(".{}", crate::schema::CASSETTE_EXT);
    let mut versions = Vec::new();
    for entry in fs::read_dir(&versions_dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(&ext) {
                versions.push(name.trim_end_matches(&ext).to_string());
            }
        }
    }
    versions.sort();
    Ok(versions)
}

/// Resolve the latest version of a cassette by slug.
///
/// # Errors
/// Returns [`CassetteError::SlugNotFound`] if no versions exist.
pub fn latest_version_path(root: &Path, slug: &str) -> Result<PathBuf> {
    let versions = list_versions(root, slug)?;
    let latest = versions.last().ok_or_else(|| CassetteError::SlugNotFound {
        slug: slug.to_string(),
    })?;
    Ok(cassette_path(root, slug, latest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CassetteHeader, CassetteStep, SCHEMA_VERSION};
    use genatp_core::action::Action;
    use genatp_core::budget::BudgetSpend;
    use rand::rngs::OsRng;

    fn make_test_cassette() -> Cassette {
        let action = Action::Navigate {
            url: "https://example.com".into(),
        };
        let step = CassetteStep {
            seq: 0,
            at: chrono::Utc::now(),
            url: "https://example.com".into(),
            snapshot_hash: "abc123".into(),
            action,
            action_success: true,
            action_message: Some("navigated".into()),
            parent_hash: String::new(),
            hash: String::new(),
        };
        // Compute hash.
        let payload = crate::schema::step_payload_for_hash(&step);
        let hash = crate::schema::compute_step_hash("", &payload);
        let step = CassetteStep {
            hash: hash.clone(),
            ..step
        };

        Cassette {
            header: CassetteHeader {
                schema_version: SCHEMA_VERSION,
                slug: "test-slug".into(),
                version: "1.0.0".into(),
                description: "test cassette".into(),
                goal: "test goal".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 1,
                tip_hash: hash,
                signer_public_key: String::new(),
                domains: vec!["example.com".into()],
            },
            steps: vec![step],
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let mut cassette = make_test_cassette();
        cassette.header.signer_public_key = signing::public_key_hex(&verifying_key);

        let path = save(&cassette, &root, &signing_key).unwrap();
        assert!(path.exists());

        let loaded = load(&path, &verifying_key).unwrap();
        assert_eq!(loaded.header.slug, "test-slug");
        assert_eq!(loaded.header.version, "1.0.0");
        assert_eq!(loaded.steps.len(), 1);
    }

    #[test]
    fn tampered_cassette_fails_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let cassette = make_test_cassette();
        let path = save(&cassette, &root, &signing_key).unwrap();

        // Tamper: flip a byte in the compressed payload.
        let mut raw = fs::read(&path).unwrap();
        if raw.len() > SIGNATURE_LEN + 1 {
            raw[0] ^= 0xFF;
        }
        fs::write(&path, &raw).unwrap();

        let err = load(&path, &verifying_key).unwrap_err();
        assert!(matches!(err, CassetteError::SignatureInvalid));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();

        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);

        let cassette = make_test_cassette();
        let path = save(&cassette, &root, &signing_key).unwrap();

        let err = load(&path, &wrong_key.verifying_key()).unwrap_err();
        assert!(matches!(err, CassetteError::SignatureInvalid));
    }

    #[test]
    fn list_slugs_and_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let signing_key = SigningKey::generate(&mut OsRng);

        // Save two versions of the same slug.
        let mut c = make_test_cassette();
        save(&c, &root, &signing_key).unwrap();
        c.header.version = "1.1.0".into();
        save(&c, &root, &signing_key).unwrap();

        let slugs = list_slugs(&root).unwrap();
        assert_eq!(slugs, vec!["test-slug"]);

        let versions = list_versions(&root, "test-slug").unwrap();
        assert_eq!(versions, vec!["1.0.0", "1.1.0"]);
    }

    #[test]
    fn latest_version_path_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let signing_key = SigningKey::generate(&mut OsRng);

        let mut c = make_test_cassette();
        save(&c, &root, &signing_key).unwrap();
        c.header.version = "1.1.0".into();
        save(&c, &root, &signing_key).unwrap();

        let path = latest_version_path(&root, "test-slug").unwrap();
        assert!(path.ends_with("1.1.0.cassette"));
    }

    #[test]
    fn not_found_slug_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err = list_versions(tmp.path(), "nonexistent").unwrap_err();
        assert!(matches!(err, CassetteError::SlugNotFound { .. }));
    }

    #[test]
    fn cassette_file_size_is_compact() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let signing_key = SigningKey::generate(&mut OsRng);
        let cassette = make_test_cassette();
        let path = save(&cassette, &root, &signing_key).unwrap();
        let size = fs::metadata(&path).unwrap().len();
        // A single-step cassette should be well under 10 KB.
        assert!(
            size < 10_000,
            "cassette file should be < 10 KB, got {size} bytes"
        );
    }
}
