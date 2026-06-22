//! Registry manifest types for cassette publishing and installation.
//!
//! The manifest is the metadata document that accompanies a cassette in the
//! registry. It declares what the cassette does, which domains it needs,
//! signing information, and version constraints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use ed25519_dalek::VerifyingKey;

use genatp_cassette::Cassette;
use genatp_cassette::signing::public_key_hex;

/// Current manifest schema version.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The trust level assigned to a publisher or cassette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// No signature — cassette is unsigned and will be blocked.
    Unsigned,
    /// Signed with a self-generated key — user is warned before install.
    SelfSigned,
    /// Signed with an organization key — auto-runs within the org.
    OrgSigned,
    /// Signed by a verified publisher — auto-runs everywhere.
    Verified,
}

impl TrustLevel {
    /// Whether cassettes at this trust level should be blocked by default.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Unsigned)
    }

    /// Whether cassettes at this trust level require user warning before install.
    #[must_use]
    pub fn requires_warning(&self) -> bool {
        matches!(self, Self::SelfSigned)
    }

    /// Whether cassettes at this trust level auto-run without prompts.
    #[must_use]
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::OrgSigned | Self::Verified)
    }
}

/// Publisher metadata stored in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publisher {
    /// Unique publisher slug (e.g. `"genatp-team"`).
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Ed25519 public key (hex-encoded, 64 chars).
    pub public_key_hex: String,
    /// When this publisher first published.
    pub created_at: DateTime<Utc>,
    /// Whether the publisher is verified (earned trust).
    pub verified: bool,
    /// Timestamp when verification was granted (if verified).
    #[serde(default)]
    pub verified_at: Option<DateTime<Utc>>,
    /// Number of cassettes published.
    #[serde(default)]
    pub cassette_count: u32,
}

/// Version constraint for semver resolution (npm-style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionConstraint {
    /// Exact match: `1.2.3`
    Exact(String),
    /// Compatible with: `^1.2.3` (≥1.2.3, <2.0.0)
    Compatible(String),
    /// Approximately equivalent: `~1.2.3` (≥1.2.3, <1.3.0)
    Approximate(String),
    /// Any version (latest).
    Any,
    /// Prerelease tag: `1.2.3-beta.1`
    Prerelease(String),
}

/// Parsed semver components for comparison.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
}

impl Semver {
    /// Parse a version string like `"1.2.3"` or `"1.2.3-beta.1"`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (base, prerelease) = if let Some(idx) = s.find('-') {
            let (b, pr) = s.split_at(idx);
            (b, Some(pr[1..].to_string()))
        } else {
            (s, None)
        };
        let parts: Vec<&str> = base.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Check if this version satisfies a constraint.
    #[must_use]
    pub fn satisfies(&self, constraint: &VersionConstraint) -> bool {
        match constraint {
            VersionConstraint::Exact(v) => Self::parse(v).is_some_and(|other| self == &other),
            VersionConstraint::Compatible(v) => Self::parse(v).is_some_and(|other| {
                self.major == other.major
                    && (self.minor > other.minor
                        || (self.minor == other.minor && self.patch >= other.patch))
                    && self.prerelease.is_none()
            }),
            VersionConstraint::Approximate(v) => Self::parse(v).is_some_and(|other| {
                self.major == other.major
                    && self.minor == other.minor
                    && self.patch >= other.patch
                    && self.prerelease.is_none()
            }),
            VersionConstraint::Any => self.prerelease.is_none(),
            VersionConstraint::Prerelease(v) => Self::parse(v).is_some_and(|other| self == &other),
        }
    }
}

/// Capability a cassette declares.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Navigate to URLs on a specific domain.
    Navigate(String),
    /// Read content from a specific domain.
    Read(String),
    /// Fill forms on a specific domain.
    Fill(String),
    /// Click elements on a specific domain.
    Click(String),
    /// Extract structured data.
    Extract,
    /// Use vault auth for a domain.
    Auth(String),
}

/// Full registry manifest for a cassette version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Cassette slug.
    pub slug: String,
    /// Semver version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Goal the cassette accomplishes.
    pub goal: String,
    /// Domains the cassette accesses.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Declared capabilities.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Publisher slug.
    pub publisher: String,
    /// Publisher's public key (hex).
    pub publisher_key: String,
    /// When this version was published.
    pub published_at: DateTime<Utc>,
    /// BLAKE3 hash of the cassette content (for integrity check).
    pub content_hash: String,
    /// Compressed size in bytes.
    #[serde(default)]
    pub compressed_size: u64,
    /// Number of steps in the cassette.
    pub step_count: u32,
    /// Trust level of this cassette.
    pub trust_level: TrustLevel,
}

/// Result of a manifest verification.
#[derive(Debug)]
pub struct VerificationResult {
    pub manifest: RegistryManifest,
    pub trust_level: TrustLevel,
    pub warnings: Vec<String>,
}

impl RegistryManifest {
    /// Build a manifest from a cassette and publisher info.
    #[must_use]
    pub fn from_cassette(
        cassette: &Cassette,
        publisher_slug: &str,
        verifying_key: &VerifyingKey,
        content_hash: String,
        compressed_size: u64,
    ) -> Self {
        let pub_key_hex = public_key_hex(verifying_key);
        let trust_level = TrustLevel::SelfSigned;

        let mut capabilities = Vec::new();
        for domain in &cassette.header.domains {
            capabilities.push(Capability::Navigate(domain.clone()));
            capabilities.push(Capability::Read(domain.clone()));
        }
        capabilities.push(Capability::Extract);

        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            slug: cassette.header.slug.clone(),
            version: cassette.header.version.clone(),
            description: cassette.header.description.clone(),
            goal: cassette.header.goal.clone(),
            domains: cassette.header.domains.clone(),
            capabilities,
            publisher: publisher_slug.to_string(),
            publisher_key: pub_key_hex,
            published_at: chrono::Utc::now(),
            content_hash,
            compressed_size,
            step_count: cassette.header.step_count,
            trust_level,
        }
    }

    /// Determine trust level for a manifest given a known publisher list.
    pub fn evaluate_trust(&self, known_publishers: &[Publisher]) -> TrustLevel {
        if self.publisher_key.is_empty() {
            return TrustLevel::Unsigned;
        }
        for pubr in known_publishers {
            if pubr.public_key_hex == self.publisher_key {
                if pubr.verified {
                    return TrustLevel::Verified;
                }
                return TrustLevel::SelfSigned;
            }
        }
        TrustLevel::SelfSigned
    }
}

/// Parse a version constraint string (npm-style).
///
/// # Errors
/// Returns `None` if the string cannot be parsed.
#[must_use]
pub fn parse_constraint(s: &str) -> Option<VersionConstraint> {
    let s = s.trim();
    if s == "*" || s == "latest" || s.is_empty() {
        return Some(VersionConstraint::Any);
    }
    if let Some(v) = s.strip_prefix('^') {
        return Semver::parse(v).map(|_| VersionConstraint::Compatible(v.to_string()));
    }
    if let Some(v) = s.strip_prefix('~') {
        return Semver::parse(v).map(|_| VersionConstraint::Approximate(v.to_string()));
    }
    if s.contains('-') {
        return Semver::parse(s).map(|_| VersionConstraint::Prerelease(s.to_string()));
    }
    Semver::parse(s).map(|_| VersionConstraint::Exact(s.to_string()))
}

/// Resolve a list of available versions against a constraint.
/// Returns the best matching version, or `None` if none match.
#[must_use]
pub fn resolve_version(available: &[String], constraint: &VersionConstraint) -> Option<String> {
    let mut parsed: Vec<(Semver, String)> = available
        .iter()
        .filter_map(|v| {
            let semver = Semver::parse(v)?;
            if semver.satisfies(constraint) {
                Some((semver, v.clone()))
            } else {
                None
            }
        })
        .collect();
    parsed.sort_by(|a, b| a.0.cmp(&b.0));
    parsed.last().map(|(_, v)| v.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parse_valid() {
        let s = Semver::parse("1.2.3").unwrap();
        assert_eq!(s.major, 1);
        assert_eq!(s.minor, 2);
        assert_eq!(s.patch, 3);
        assert!(s.prerelease.is_none());
    }

    #[test]
    fn semver_parse_prerelease() {
        let s = Semver::parse("1.2.3-beta.1").unwrap();
        assert_eq!(s.prerelease, Some("beta.1".into()));
    }

    #[test]
    fn semver_parse_invalid() {
        assert!(Semver::parse("").is_none());
        assert!(Semver::parse("1.2").is_none());
        assert!(Semver::parse("abc").is_none());
    }

    #[test]
    fn constraint_exact_match() {
        let c = parse_constraint("1.2.3").unwrap();
        assert_eq!(c, VersionConstraint::Exact("1.2.3".into()));
        assert!(Semver::parse("1.2.3").unwrap().satisfies(&c));
        assert!(!Semver::parse("1.2.4").unwrap().satisfies(&c));
    }

    #[test]
    fn constraint_compatible() {
        let c = parse_constraint("^1.2.3").unwrap();
        assert_eq!(c, VersionConstraint::Compatible("1.2.3".into()));
        assert!(Semver::parse("1.2.3").unwrap().satisfies(&c));
        assert!(Semver::parse("1.5.0").unwrap().satisfies(&c));
        assert!(!Semver::parse("2.0.0").unwrap().satisfies(&c));
    }

    #[test]
    fn constraint_approximate() {
        let c = parse_constraint("~1.2.3").unwrap();
        assert!(Semver::parse("1.2.3").unwrap().satisfies(&c));
        assert!(Semver::parse("1.2.9").unwrap().satisfies(&c));
        assert!(!Semver::parse("1.3.0").unwrap().satisfies(&c));
    }

    #[test]
    fn constraint_any() {
        let c = parse_constraint("latest").unwrap();
        assert_eq!(c, VersionConstraint::Any);
        assert!(Semver::parse("99.0.0").unwrap().satisfies(&c));
        assert!(!Semver::parse("1.0.0-beta").unwrap().satisfies(&c));
    }

    #[test]
    fn resolve_versions() {
        let available = vec![
            "1.0.0".into(),
            "1.2.0".into(),
            "1.5.0".into(),
            "2.0.0".into(),
        ];
        let constraint = parse_constraint("^1.0.0").unwrap();
        let resolved = resolve_version(&available, &constraint);
        assert_eq!(resolved, Some("1.5.0".into()));
    }

    #[test]
    fn resolve_no_match() {
        let available = vec!["2.0.0".into(), "3.0.0".into()];
        let constraint = parse_constraint("^1.0.0").unwrap();
        assert!(resolve_version(&available, &constraint).is_none());
    }

    #[test]
    fn trust_level_behaviors() {
        assert!(TrustLevel::Unsigned.is_blocked());
        assert!(!TrustLevel::SelfSigned.is_blocked());
        assert!(TrustLevel::SelfSigned.requires_warning());
        assert!(!TrustLevel::Verified.requires_warning());
        assert!(TrustLevel::Verified.is_trusted());
        assert!(TrustLevel::OrgSigned.is_trusted());
    }

    #[test]
    fn evaluate_trust_ladder() {
        let verified_publisher = Publisher {
            slug: "trusted".into(),
            name: "Trusted Pub".into(),
            public_key_hex: "abcd".into(),
            created_at: Utc::now(),
            verified: true,
            verified_at: Some(Utc::now()),
            cassette_count: 10,
        };
        let manifest = RegistryManifest {
            schema_version: 1,
            slug: "test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            goal: String::new(),
            domains: vec![],
            capabilities: vec![],
            publisher: "trusted".into(),
            publisher_key: "abcd".into(),
            published_at: Utc::now(),
            content_hash: "hash".into(),
            compressed_size: 100,
            step_count: 1,
            trust_level: TrustLevel::SelfSigned,
        };
        let level = manifest.evaluate_trust(&[verified_publisher]);
        assert_eq!(level, TrustLevel::Verified);
    }

    #[test]
    fn unsigned_manifest_blocked() {
        let manifest = RegistryManifest {
            publisher_key: String::new(),
            ..create_test_manifest()
        };
        let level = manifest.evaluate_trust(&[]);
        assert_eq!(level, TrustLevel::Unsigned);
    }

    fn create_test_manifest() -> RegistryManifest {
        RegistryManifest {
            schema_version: 1,
            slug: "test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            goal: String::new(),
            domains: vec![],
            capabilities: vec![],
            publisher: "test".into(),
            publisher_key: "hexkey".into(),
            published_at: Utc::now(),
            content_hash: "hash".into(),
            compressed_size: 100,
            step_count: 1,
            trust_level: TrustLevel::SelfSigned,
        }
    }
}
