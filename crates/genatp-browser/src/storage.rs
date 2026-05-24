//! Cookie + localStorage hydration helpers.
//!
//! These are the seams Phase 6 ([Auth Vault](../../../docs/DEVELOPMENT_PLAN.md))
//! plugs into when re-hydrating a logged-in session into a fresh browser
//! context. We keep the API small and side-effect free here so it's easy
//! to test independently of crypto.

use chromiumoxide::cdp::browser_protocol::network::{CookieParam, CookieSameSite};
use serde::{Deserialize, Serialize};

use crate::cdp::Page;
use crate::error::Result;

/// JSON-friendly cookie representation. Mirrors the subset of CDP's
/// `Network.CookieParam` that Phase 1 needs. Phase 6 adds the rest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Domain scope (e.g. `".github.com"`). Required by CDP if `url` not set.
    pub domain: Option<String>,
    /// Path scope (default `"/"`).
    pub path: Option<String>,
    /// `Secure` flag.
    #[serde(default)]
    pub secure: bool,
    /// `HttpOnly` flag.
    #[serde(default)]
    pub http_only: bool,
    /// `SameSite` attribute (`"Strict"`, `"Lax"`, `"None"`).
    pub same_site: Option<String>,
    /// Optional URL — when set, Chrome derives `domain`/`path` from it.
    pub url: Option<String>,
}

impl Cookie {
    /// Construct a minimal cookie pinned to a URL.
    #[must_use]
    pub fn for_url(
        name: impl Into<String>,
        value: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            url: Some(url.into()),
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    fn into_cdp(self) -> CookieParam {
        let mut p = CookieParam::new(self.name, self.value);
        p.url = self.url;
        p.domain = self.domain;
        p.path = self.path;
        p.secure = Some(self.secure);
        p.http_only = Some(self.http_only);
        p.same_site = self.same_site.and_then(|s| match s.as_str() {
            "Strict" => Some(CookieSameSite::Strict),
            "Lax" => Some(CookieSameSite::Lax),
            "None" => Some(CookieSameSite::None),
            _ => None,
        });
        p
    }
}

/// Set the supplied cookies on `page`. The page must already have navigated
/// to a URL on the same registrable domain (Chrome rejects orphan cookies).
///
/// # Errors
/// Surfaces any CDP failure verbatim.
pub async fn set_cookies(page: &Page, cookies: Vec<Cookie>) -> Result<()> {
    let cdp_cookies: Vec<CookieParam> = cookies.into_iter().map(Cookie::into_cdp).collect();
    page.raw().set_cookies(cdp_cookies).await?;
    Ok(())
}

/// Set a single named localStorage value for the given origin.
///
/// Implementation note: Phase 1 uses `Page.evaluate` (a single round-trip)
/// rather than the `DOMStorage` domain because the latter requires an extra
/// `DOMStorage.enable` and a `StorageId` scoped to a security origin that
/// the test harness may not yet know. Phase 6 swaps to `DOMStorage` for
/// vault hydration where origin is known up-front.
///
/// # Errors
/// Returns the underlying CDP error if `Runtime.evaluate` fails.
pub async fn set_local_storage(page: &Page, key: &str, value: &str) -> Result<()> {
    let escaped_key = serde_json::to_string(key).expect("string serializes");
    let escaped_value = serde_json::to_string(value).expect("string serializes");
    let script = format!("localStorage.setItem({escaped_key}, {escaped_value});");
    page.raw().evaluate(script).await?;
    Ok(())
}

/// Read a single localStorage value, returning `None` if the key is absent.
///
/// # Errors
/// Returns the underlying CDP error if `Runtime.evaluate` fails.
pub async fn get_local_storage(page: &Page, key: &str) -> Result<Option<String>> {
    let escaped_key = serde_json::to_string(key).expect("string serializes");
    let script = format!("localStorage.getItem({escaped_key})");
    let result = page.raw().evaluate(script).await?;
    Ok(result.into_value().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_for_url_constructor_sets_url_and_no_domain() {
        let c = Cookie::for_url("session", "abc", "https://example.com/");
        assert_eq!(c.name, "session");
        assert_eq!(c.value, "abc");
        assert_eq!(c.url.as_deref(), Some("https://example.com/"));
        assert!(c.domain.is_none());
    }

    #[test]
    fn cookie_round_trips_through_serde() {
        let c = Cookie::for_url("k", "v", "https://example.com/");
        let json = serde_json::to_string(&c).unwrap();
        let back: Cookie = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn cookie_into_cdp_propagates_flags() {
        let c = Cookie {
            name: "k".into(),
            value: "v".into(),
            domain: Some(".example.com".into()),
            path: Some("/".into()),
            secure: true,
            http_only: true,
            same_site: Some("Strict".into()),
            url: None,
        };
        let cdp = c.into_cdp();
        assert_eq!(cdp.secure, Some(true));
        assert_eq!(cdp.http_only, Some(true));
        assert!(matches!(cdp.same_site, Some(CookieSameSite::Strict)));
    }

    #[test]
    fn unknown_same_site_is_ignored() {
        let c = Cookie {
            name: "k".into(),
            value: "v".into(),
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: Some("Bogus".into()),
            url: Some("https://example.com/".into()),
        };
        assert!(c.into_cdp().same_site.is_none());
    }
}
