//! Capability tokens — the authorisation layer above the vault.
//!
//! A cassette (or any caller) declares which domains it needs auth
//! for. The user grants the capability once; subsequent vault
//! load/save calls check the requested domain against the grant.
//!
//! Capabilities are *runtime* objects — they're not persisted in the
//! vault. The vault file itself is gated by the OS keychain; the
//! capability just bounds what a single process can do once it has
//! the master key.

use std::collections::HashSet;

use crate::entry::canonical_domain;
use crate::error::{Result, VaultError};

/// A grant authorising auth operations on a closed set of domains.
///
/// `Capability::any()` is intended for the CLI / interactive use; the
/// MCP server and cassette runtime always pass a tight grant.
#[derive(Debug, Clone)]
pub struct Capability {
    /// `None` ⇒ no domain restriction (for CLI / interactive use).
    /// `Some(set)` ⇒ exactly these canonicalized domains are allowed.
    allowed: Option<HashSet<String>>,
}

impl Capability {
    /// Build a capability granting access to `domains`. Each domain is
    /// canonicalized; duplicates collapse.
    ///
    /// # Errors
    /// Returns [`VaultError::InvalidDomain`] if any domain fails
    /// canonicalization.
    pub fn for_domains<I, S>(domains: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = HashSet::new();
        for d in domains {
            set.insert(canonical_domain(d.as_ref())?);
        }
        Ok(Self { allowed: Some(set) })
    }

    /// Grant access to *all* domains. Use this only in the CLI, never
    /// from an LLM-driven path.
    #[must_use]
    pub fn any() -> Self {
        Self { allowed: None }
    }

    /// `true` if `domain` (already canonicalized) is permitted.
    #[must_use]
    pub fn allows(&self, domain: &str) -> bool {
        match &self.allowed {
            None => true,
            Some(set) => set.contains(domain),
        }
    }

    /// Enforce — returns `Err(CapabilityDenied)` if the domain isn't
    /// allowed.
    ///
    /// # Errors
    /// Returns [`VaultError::CapabilityDenied`] if disallowed.
    pub fn require(&self, domain: &str) -> Result<()> {
        if self.allows(domain) {
            Ok(())
        } else {
            Err(VaultError::CapabilityDenied(domain.into()))
        }
    }

    /// Iterate the allowed domains, if scoped. Returns `None` for
    /// `Capability::any()`.
    #[must_use]
    pub fn domains(&self) -> Option<impl Iterator<Item = &str>> {
        self.allowed.as_ref().map(|s| s.iter().map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_allows_every_domain() {
        let cap = Capability::any();
        assert!(cap.allows("github.com"));
        assert!(cap.allows("evil.example.com"));
    }

    #[test]
    fn scoped_canonicalises_inputs() {
        let cap = Capability::for_domains([".GITHUB.com", "Stripe.COM"]).unwrap();
        assert!(cap.allows("github.com"));
        assert!(cap.allows("stripe.com"));
        assert!(!cap.allows("evil.com"));
    }

    #[test]
    fn require_returns_capability_denied() {
        let cap = Capability::for_domains(["github.com"]).unwrap();
        let err = cap.require("evil.com").unwrap_err();
        assert!(matches!(err, VaultError::CapabilityDenied(d) if d == "evil.com"));
    }

    #[test]
    fn invalid_domain_at_construction() {
        assert!(Capability::for_domains(["https://x.com/"]).is_err());
    }
}
