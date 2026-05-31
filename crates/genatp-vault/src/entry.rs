//! Vault entry types — the *plaintext* shape of what we encrypt.
//!
//! Two kinds for v0.3.0: HTTP cookies and `localStorage` key/value
//! pairs. Both deserialise from JSON so the wire format and the test
//! fixtures stay legible.

use serde::{Deserialize, Serialize};

use genatp_browser::storage::Cookie;

/// Discriminator used in SQLite `entries.kind` and in the AAD of
/// every ciphertext.
pub const KIND_COOKIE: &str = "cookie";
/// Discriminator for `localStorage` entries.
pub const KIND_LOCAL_STORAGE: &str = "local_storage";

/// One vault entry — either a cookie or a `localStorage` pair.
///
/// `domain` is canonicalized via [`canonical_domain`] before being
/// stored so equality checks on the storage layer always see the same
/// representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// HTTP cookie (subset of `Network.CookieParam`).
    Cookie {
        /// Canonicalized domain.
        domain: String,
        /// Full cookie definition; reused as the wire type with
        /// `genatp-browser::storage::Cookie`.
        cookie: Cookie,
    },
    /// One `localStorage` key/value pair.
    LocalStorage {
        /// Canonicalized domain.
        domain: String,
        /// `localStorage` key.
        key: String,
        /// `localStorage` value (UTF-8).
        value: String,
    },
}

impl Entry {
    /// Construct a cookie entry. `domain` is canonicalized.
    ///
    /// # Errors
    /// Returns [`crate::error::VaultError::InvalidDomain`] if the
    /// domain fails canonicalization.
    pub fn cookie(domain: &str, cookie: Cookie) -> crate::error::Result<Self> {
        Ok(Self::Cookie {
            domain: canonical_domain(domain)?,
            cookie,
        })
    }

    /// Construct a `localStorage` entry. `domain` is canonicalized.
    ///
    /// # Errors
    /// Returns [`crate::error::VaultError::InvalidDomain`] if the
    /// domain fails canonicalization.
    pub fn local_storage(
        domain: &str,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> crate::error::Result<Self> {
        Ok(Self::LocalStorage {
            domain: canonical_domain(domain)?,
            key: key.into(),
            value: value.into(),
        })
    }

    /// Canonical domain owned by this entry.
    #[must_use]
    pub fn domain(&self) -> &str {
        match self {
            Self::Cookie { domain, .. } | Self::LocalStorage { domain, .. } => domain,
        }
    }

    /// Storage discriminator.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Cookie { .. } => KIND_COOKIE,
            Self::LocalStorage { .. } => KIND_LOCAL_STORAGE,
        }
    }

    /// Per-entry sub-key — used in the SQLite `(domain, kind, sub_key)`
    /// uniqueness constraint and in the AAD.
    #[must_use]
    pub fn sub_key(&self) -> String {
        match self {
            Self::Cookie { cookie, .. } => {
                format!("{}|{}", cookie.name, cookie.path.as_deref().unwrap_or("/"))
            }
            Self::LocalStorage { key, .. } => key.clone(),
        }
    }
}

/// Canonicalize a domain string.
///
/// Rules (intentionally narrow — we want predictable round-trips, not
/// IDNA correctness here; that's `genatp-perception`'s job):
///   1. Trim ASCII whitespace.
///   2. Reject empty inputs.
///   3. Reject inputs that look like a URL (`scheme://…`).
///   4. Strip a leading `.` (cookie-style) — `.github.com` → `github.com`.
///   5. Lower-case ASCII letters (domains are case-insensitive).
///   6. Reject inputs containing `/`, `?`, or `#` — they aren't bare
///      domains.
///
/// # Errors
/// Returns [`crate::error::VaultError::InvalidDomain`] if any rule fails.
pub fn canonical_domain(input: &str) -> crate::error::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(crate::error::VaultError::InvalidDomain(input.into()));
    }
    if trimmed.contains("://")
        || trimmed.contains('/')
        || trimmed.contains('?')
        || trimmed.contains('#')
    {
        return Err(crate::error::VaultError::InvalidDomain(input.into()));
    }
    let stripped = trimmed.strip_prefix('.').unwrap_or(trimmed);
    if stripped.is_empty() {
        return Err(crate::error::VaultError::InvalidDomain(input.into()));
    }
    Ok(stripped.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_canonicalises_domain() {
        let e = Entry::cookie(
            ".GitHub.com",
            Cookie::for_url("session", "v", "https://github.com/"),
        )
        .unwrap();
        assert_eq!(e.domain(), "github.com");
    }

    #[test]
    fn rejects_url_input() {
        assert!(canonical_domain("https://github.com/").is_err());
        assert!(canonical_domain("github.com/path").is_err());
        assert!(canonical_domain("github.com?x=1").is_err());
    }

    #[test]
    fn rejects_empty_after_dot_strip() {
        assert!(canonical_domain(".").is_err());
        assert!(canonical_domain("").is_err());
    }

    #[test]
    fn entry_sub_key_includes_path_for_cookie() {
        let mut c = Cookie::for_url("session", "v", "https://github.com/");
        c.path = Some("/login".into());
        let e = Entry::Cookie {
            domain: "github.com".into(),
            cookie: c,
        };
        assert_eq!(e.sub_key(), "session|/login");
    }

    #[test]
    fn entry_sub_key_for_local_storage_is_key() {
        let e = Entry::local_storage("example.com", "auth_token", "x").unwrap();
        assert_eq!(e.sub_key(), "auth_token");
    }
}
