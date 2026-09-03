//! Load pinned verifying keys from `keys.toml` (shipped with this crate).

use std::collections::HashMap;
use std::path::Path;

use ed25519_dalek::VerifyingKey;

use crate::error::{RegistryError, Result};

/// Parsed `keys.toml` contents.
#[derive(Debug, Default, Clone)]
pub struct PinnedKeys {
    /// Maintainer slug → verifying key.
    pub maintainers: HashMap<String, VerifyingKey>,
    /// Org slug → verifying key.
    pub org: HashMap<String, VerifyingKey>,
}

impl PinnedKeys {
    /// True when the hex public key is listed under maintainers or org.
    #[must_use]
    pub fn contains_hex(&self, public_key_hex: &str) -> bool {
        self.maintainers
            .values()
            .chain(self.org.values())
            .any(|k| hex::encode(k.as_bytes()) == public_key_hex.to_ascii_lowercase())
    }

    /// Number of pinned keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.maintainers.len() + self.org.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Load keys from the crate-embedded `keys.toml` (next to this source file
/// at build time via `include_str` fallback path).
pub fn load_bundled() -> Result<PinnedKeys> {
    const RAW: &str = include_str!("../keys.toml");
    parse_keys_toml(RAW)
}

/// Load keys from an arbitrary path (tests / custom installs).
pub fn load_from_path(path: &Path) -> Result<PinnedKeys> {
    let raw = std::fs::read_to_string(path).map_err(|source| RegistryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_keys_toml(&raw)
}

fn parse_keys_toml(raw: &str) -> Result<PinnedKeys> {
    let value: toml::Value =
        toml::from_str(raw).map_err(|e| RegistryError::Config(format!("keys.toml: {e}")))?;
    let mut out = PinnedKeys::default();
    if let Some(table) = value.get("maintainers").and_then(|v| v.as_table()) {
        for (slug, key) in table {
            if let Some(hex_key) = key.as_str() {
                if hex_key.trim().is_empty() || hex_key.trim().starts_with('#') {
                    continue;
                }
                out.maintainers
                    .insert(slug.clone(), parse_verifying_key(hex_key)?);
            }
        }
    }
    if let Some(table) = value.get("org").and_then(|v| v.as_table()) {
        for (slug, key) in table {
            if let Some(hex_key) = key.as_str() {
                if hex_key.trim().is_empty() {
                    continue;
                }
                out.org.insert(slug.clone(), parse_verifying_key(hex_key)?);
            }
        }
    }
    Ok(out)
}

fn parse_verifying_key(hex_key: &str) -> Result<VerifyingKey> {
    let bytes = hex::decode(hex_key.trim())
        .map_err(|e| RegistryError::Config(format!("invalid key hex: {e}")))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| RegistryError::Config("ed25519 public key must be 32 bytes".into()))?;
    VerifyingKey::from_bytes(&arr)
        .map_err(|e| RegistryError::Config(format!("invalid ed25519 key: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_keys_parse() {
        let keys = load_bundled().expect("keys.toml must parse");
        // Empty until official keys are published — that is intentional.
        assert!(
            keys.is_empty(),
            "bundled keys.toml should stay empty until official keys ship"
        );
    }
}
