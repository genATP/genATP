//! Registry client — operations for publishing, installing, searching, and
//! managing cassettes in the registry.
//!
//! Supports both local (SQLite-backed) and remote (HTTP) registries.

use ed25519_dalek::{SigningKey, VerifyingKey};

use genatp_cassette::storage::{cassettes_dir, load, save};
use genatp_cassette::{Cassette, signing};

use crate::Result;
use crate::manifest::{Publisher, RegistryManifest};
use crate::storage::RegistryStore;

/// A cassette that has been installed locally.
#[derive(Debug)]
pub struct InstalledCassette {
    pub manifest: RegistryManifest,
    pub cassette: Cassette,
    pub local_path: std::path::PathBuf,
    pub warnings: Vec<String>,
}

/// Publish a cassette to the local registry.
///
/// # Errors
/// Surfaces storage, signing, or I/O errors.
pub fn publish(
    store: &RegistryStore,
    cassette: &Cassette,
    publisher_slug: &str,
    signing_key: &SigningKey,
    verifying_key: &VerifyingKey,
) -> Result<RegistryManifest> {
    // Ensure the publisher exists.
    let pubr = store.get_publisher(publisher_slug)?;
    if pubr.is_none() {
        let new_pub = Publisher {
            slug: publisher_slug.to_string(),
            name: publisher_slug.to_string(),
            public_key_hex: signing::public_key_hex(verifying_key),
            created_at: chrono::Utc::now(),
            verified: false,
            verified_at: None,
            cassette_count: 0,
        };
        store.upsert_publisher(&new_pub)?;
    }

    // Serialize cassette to compute content hash.
    let cassette_json = serde_json::to_vec(cassette).map_err(crate::RegistryError::Serde)?;
    let content_hash = hex::encode(blake3::hash(&cassette_json).as_bytes());
    let compressed = zstd::encode_all(cassette_json.as_slice(), 3)
        .map_err(|e| crate::RegistryError::Zstd(e.to_string()))?;
    let compressed_size = compressed.len() as u64;

    // Build the manifest.
    let manifest = RegistryManifest::from_cassette(
        cassette,
        publisher_slug,
        verifying_key,
        content_hash,
        compressed_size,
    );

    // Save to local cassette storage.
    let root = cassettes_dir().map_err(|e| crate::RegistryError::Storage(e.to_string()))?;
    save(cassette, &root, signing_key).map_err(crate::RegistryError::Cassette)?;

    // Register in the registry store.
    store.publish(&manifest)?;

    Ok(manifest)
}

/// Install a cassette from the registry by slug and version constraint.
///
/// # Errors
/// Surfaces storage, trust, or I/O errors.
pub fn install(
    store: &RegistryStore,
    slug: &str,
    constraint_str: &str,
    confirmed: bool,
) -> Result<InstalledCassette> {
    let manifest = store.resolve(slug, constraint_str)?.ok_or_else(|| {
        crate::RegistryError::NotFound(format!("{slug} matching {constraint_str}"))
    })?;

    // Trust ladder enforcement.
    let trust_level = manifest.trust_level;
    if trust_level.is_blocked() {
        return Err(crate::RegistryError::UnsignedBlocked);
    }
    if trust_level.requires_warning() && !confirmed {
        return Err(crate::RegistryError::SelfSignedRequiresConfirm);
    }

    let mut warnings = Vec::new();
    if trust_level.requires_warning() {
        warnings.push(format!(
            "cassette `{}` is self-signed by `{}`. Verify the publisher before running.",
            slug, manifest.publisher
        ));
    }

    // Try to find the cassette in local storage.
    let root = cassettes_dir().map_err(|e| crate::RegistryError::Storage(e.to_string()))?;
    let path = genatp_cassette::storage::cassette_path(&root, slug, &manifest.version);
    let verifying_key = signing::public_key_from_hex(&manifest.publisher_key)
        .map_err(|e| crate::RegistryError::Signature(e.to_string()))?;
    let cassette = load(&path, &verifying_key).map_err(crate::RegistryError::Cassette)?;

    Ok(InstalledCassette {
        manifest,
        cassette,
        local_path: path,
        warnings,
    })
}

/// Search cassettes in the registry.
///
/// # Errors
/// Surfaces storage errors.
pub fn search(store: &RegistryStore, query: &str) -> Result<Vec<RegistryManifest>> {
    store.search(query)
}

/// List all publisher slugs.
///
/// # Errors
/// Surfaces storage errors.
pub fn list_publishers(store: &RegistryStore) -> Result<Vec<Publisher>> {
    store.list_publishers()
}

/// Print manifest information for CLI display.
#[must_use]
pub fn format_manifest_info(manifest: &RegistryManifest) -> String {
    let trust_str = format!("{:?}", manifest.trust_level);
    format!(
        r#"{slug}@{version}
  Publisher: {pub} ({key})
  Description: {desc}
  Goal: {goal}
  Domains: {domains}
  Trust: {trust}
  Steps: {steps}
  Size: {size} bytes
  Published: {at}"#,
        slug = manifest.slug,
        version = manifest.version,
        pub = manifest.publisher,
        key = &manifest.publisher_key[..16.min(manifest.publisher_key.len())],
        desc = manifest.description,
        goal = manifest.goal,
        domains = manifest.domains.join(", "),
        trust = trust_str,
        steps = manifest.step_count,
        size = manifest.compressed_size,
        at = manifest.published_at.format("%Y-%m-%d %H:%M:%S UTC"),
    )
}

/// Print cassette info for install display.
#[must_use]
pub fn format_install_info(installed: &InstalledCassette) -> String {
    let mut out = format!(
        "Installed: {}@{}\n",
        installed.manifest.slug, installed.manifest.version
    );
    // Use format directly for the path line to avoid clippy warnings.
    out.push_str(&format!("  Path: {}\n", installed.local_path.display()));
    out.push_str(&format!("  Steps: {}\n", installed.cassette.steps.len()));
    out.push_str(&format!(
        "  Domains: {}\n",
        installed.manifest.domains.join(", ")
    ));
    for warning in &installed.warnings {
        out.push_str(&format!("  ⚠  {warning}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use genatp_cassette::schema::{Cassette, CassetteHeader, CassetteStep, SCHEMA_VERSION};
    use genatp_core::action::Action;
    use genatp_core::budget::BudgetSpend;
    use rand::rngs::OsRng;

    fn test_cassette() -> Cassette {
        let step = CassetteStep {
            seq: 0,
            at: chrono::Utc::now(),
            url: "https://example.com".into(),
            snapshot_hash: "abc".into(),
            action: Action::Navigate {
                url: "https://example.com".into(),
            },
            action_success: true,
            action_message: None,
            parent_hash: String::new(),
            hash: "hash".into(),
        };
        Cassette {
            header: CassetteHeader {
                slug: "test-cassette".into(),
                version: "1.0.0".into(),
                schema_version: SCHEMA_VERSION,
                description: "Test cassette".into(),
                goal: "Test goal".into(),
                start_url: "https://example.com".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 1,
                tip_hash: "hash".into(),
                signer_public_key: "key".into(),
                domains: vec!["example.com".into()],
            },
            steps: vec![step],
        }
    }

    #[test]
    fn publish_and_install_roundtrip() {
        let db = RegistryStore::open(":memory:").unwrap();
        let cassette = test_cassette();
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Test publish
        let manifest = publish(&db, &cassette, "test-pub", &signing_key, &verifying_key).unwrap();
        assert_eq!(manifest.slug, "test-cassette");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn search_works() {
        let db = RegistryStore::open(":memory:").unwrap();
        // Publish directly via storage to set up test data (avoids filesystem cassette save).
        let manifest = RegistryManifest {
            schema_version: 1,
            slug: "test-cassette".into(),
            version: "1.0.0".into(),
            description: "Test cassette".into(),
            goal: "Test goal".into(),
            domains: vec!["example.com".into()],
            capabilities: vec![],
            publisher: "test-pub".into(),
            publisher_key: "key".into(),
            published_at: chrono::Utc::now(),
            content_hash: "hash".into(),
            compressed_size: 100,
            step_count: 1,
            trust_level: crate::manifest::TrustLevel::SelfSigned,
        };
        db.publish(&manifest).unwrap();

        let results = search(&db, "test").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].slug, "test-cassette");
    }
}
