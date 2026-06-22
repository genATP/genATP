//! Local registry storage — SQLite-backed store for cassette manifests and
//! publisher information in self-host mode.
//!
//! In production (genATP Cloud), the registry is backed by S3 + Postgres.
//! This local store is for self-host and development use.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;
use crate::manifest::{Capability, Publisher, RegistryManifest, parse_constraint, resolve_version};

/// Default registry storage directory: `~/.genatp/registry/`.
pub fn default_registry_dir() -> Result<PathBuf> {
    let proj = directories::ProjectDirs::from("dev", "genatp", "genatp")
        .ok_or_else(|| crate::RegistryError::Storage("cannot resolve home directory".into()))?;
    Ok(proj.data_dir().join("registry"))
}

/// SQLite-backed registry store.
pub struct RegistryStore {
    conn: Connection,
    path: PathBuf,
}

impl RegistryStore {
    /// Open or create a registry store at the given path.
    ///
    /// # Errors
    /// Surfaces SQLite open / DDL failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|e| crate::RegistryError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let conn = Connection::open(&path_buf)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS publishers (
                slug            TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                public_key_hex  TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                verified        INTEGER NOT NULL DEFAULT 0,
                verified_at     INTEGER,
                cassette_count  INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS cassettes (
                slug            TEXT NOT NULL,
                version         TEXT NOT NULL,
                description     TEXT NOT NULL DEFAULT '',
                goal            TEXT NOT NULL DEFAULT '',
                publisher       TEXT NOT NULL,
                publisher_key   TEXT NOT NULL DEFAULT '',
                published_at    INTEGER NOT NULL,
                content_hash    TEXT NOT NULL,
                compressed_size INTEGER NOT NULL DEFAULT 0,
                step_count      INTEGER NOT NULL DEFAULT 0,
                trust_level     TEXT NOT NULL DEFAULT 'self_signed',
                manifest_json   TEXT NOT NULL,
                PRIMARY KEY (slug, version),
                FOREIGN KEY (publisher) REFERENCES publishers(slug)
            );

            CREATE TABLE IF NOT EXISTS cassette_domains (
                slug    TEXT NOT NULL,
                domain  TEXT NOT NULL,
                PRIMARY KEY (slug, domain)
            );

            CREATE TABLE IF NOT EXISTS cassette_capabilities (
                slug        TEXT NOT NULL,
                version     TEXT NOT NULL,
                capability  TEXT NOT NULL,
                domain      TEXT,
                PRIMARY KEY (slug, version, capability, domain)
            );
            ",
        )?;

        Ok(Self {
            conn,
            path: path_buf,
        })
    }

    /// Return the store path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Register or update a publisher.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn upsert_publisher(&self, publisher: &Publisher) -> Result<()> {
        self.conn.execute(
            r"INSERT INTO publishers (slug, name, public_key_hex, created_at, verified, verified_at, cassette_count)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
              ON CONFLICT(slug) DO UPDATE SET
                name = excluded.name,
                public_key_hex = excluded.public_key_hex,
                verified = excluded.verified,
                verified_at = excluded.verified_at,
                cassette_count = excluded.cassette_count",
            params![
                publisher.slug,
                publisher.name,
                publisher.public_key_hex,
                publisher.created_at.timestamp_millis(),
                publisher.verified as i32,
                publisher.verified_at.map(|d| d.timestamp_millis()),
                publisher.cassette_count,
            ],
        )?;
        Ok(())
    }

    /// Get a publisher by slug.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn get_publisher(&self, slug: &str) -> Result<Option<Publisher>> {
        let row = self
            .conn
            .query_row(
                "SELECT slug, name, public_key_hex, created_at, verified, verified_at, cassette_count
                 FROM publishers WHERE slug = ?1",
                params![slug],
                |row| {
                    let created_ms: i64 = row.get(3)?;
                    let verified: i32 = row.get(4)?;
                    let verified_ms: Option<i64> = row.get(5)?;
                    Ok(Publisher {
                        slug: row.get(0)?,
                        name: row.get(1)?,
                        public_key_hex: row.get(2)?,
                        created_at: DateTime::from_timestamp_millis(created_ms)
                            .unwrap_or_default(),
                        verified: verified != 0,
                        verified_at: verified_ms
                            .and_then(DateTime::from_timestamp_millis),
                        cassette_count: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// List all publishers.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn list_publishers(&self) -> Result<Vec<Publisher>> {
        let mut stmt = self.conn.prepare(
            "SELECT slug, name, public_key_hex, created_at, verified, verified_at, cassette_count
             FROM publishers ORDER BY slug",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let created_ms: i64 = row.get(3)?;
                let verified: i32 = row.get(4)?;
                let verified_ms: Option<i64> = row.get(5)?;
                Ok(Publisher {
                    slug: row.get(0)?,
                    name: row.get(1)?,
                    public_key_hex: row.get(2)?,
                    created_at: DateTime::from_timestamp_millis(created_ms).unwrap_or_default(),
                    verified: verified != 0,
                    verified_at: verified_ms.and_then(DateTime::from_timestamp_millis),
                    cassette_count: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Publish a cassette manifest.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn publish(&self, manifest: &RegistryManifest) -> Result<()> {
        // Auto-create publisher if it doesn't exist (FK constraint).
        if self.get_publisher(&manifest.publisher)?.is_none() {
            let new_pub = Publisher {
                slug: manifest.publisher.clone(),
                name: manifest.publisher.clone(),
                public_key_hex: manifest.publisher_key.clone(),
                created_at: chrono::Utc::now(),
                verified: false,
                verified_at: None,
                cassette_count: 0,
            };
            self.upsert_publisher(&new_pub)?;
        }

        let manifest_json =
            serde_json::to_string(&manifest).map_err(crate::RegistryError::Serde)?;

        // Use a transaction for atomicity.
        self.conn.execute_batch("BEGIN TRANSACTION")?;

        let result = (|| -> Result<()> {
            self.conn.execute(
                r"INSERT OR REPLACE INTO cassettes
                  (slug, version, description, goal, publisher, publisher_key,
                   published_at, content_hash, compressed_size, step_count, trust_level, manifest_json)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    manifest.slug,
                    manifest.version,
                    manifest.description,
                    manifest.goal,
                    manifest.publisher,
                    manifest.publisher_key,
                    manifest.published_at.timestamp_millis(),
                    manifest.content_hash,
                    manifest.compressed_size as i64,
                    manifest.step_count as i64,
                    format!("{:?}", manifest.trust_level).to_lowercase(),
                    &manifest_json,
                ],
            )?;

            // Store domains.
            for domain in &manifest.domains {
                self.conn.execute(
                    "INSERT OR IGNORE INTO cassette_domains (slug, domain) VALUES (?1, ?2)",
                    params![manifest.slug, domain],
                )?;
            }

            // Store capabilities.
            for cap in &manifest.capabilities {
                let (cap_name, cap_domain) = match cap {
                    Capability::Navigate(d)
                    | Capability::Read(d)
                    | Capability::Fill(d)
                    | Capability::Click(d)
                    | Capability::Auth(d) => {
                        let s = format!("{cap:?}");
                        (
                            s.split('(').next().unwrap_or("unknown").to_lowercase(),
                            Some(d.clone()),
                        )
                    }
                    Capability::Extract => ("extract".into(), None::<String>),
                };
                self.conn.execute(
                    "INSERT OR IGNORE INTO cassette_capabilities (slug, version, capability, domain)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![manifest.slug, manifest.version, cap_name, cap_domain],
                )?;
            }

            // Bump publisher cassette count.
            self.conn.execute(
                "UPDATE publishers SET cassette_count = cassette_count + 1 WHERE slug = ?1",
                params![manifest.publisher],
            )?;

            Ok(())
        })();

        if result.is_ok() {
            self.conn.execute_batch("COMMIT")?;
        } else {
            self.conn.execute_batch("ROLLBACK")?;
        }

        result
    }

    /// Get a manifest by slug and version.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn get_manifest(&self, slug: &str, version: &str) -> Result<Option<RegistryManifest>> {
        let row: Option<String> = self
            .conn
            .query_row(
                "SELECT manifest_json FROM cassettes WHERE slug = ?1 AND version = ?2",
                params![slug, version],
                |row| row.get(0),
            )
            .optional()?;

        match row {
            Some(json) => {
                let manifest: RegistryManifest =
                    serde_json::from_str(&json).map_err(crate::RegistryError::Serde)?;
                Ok(Some(manifest))
            }
            None => Ok(None),
        }
    }

    /// List all versions of a cassette, sorted.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn list_versions(&self, slug: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT version FROM cassettes WHERE slug = ?1 ORDER BY version")?;
        let versions = stmt
            .query_map(params![slug], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(versions)
    }

    /// Resolve a version constraint against available versions.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn resolve(&self, slug: &str, constraint_str: &str) -> Result<Option<RegistryManifest>> {
        let versions = self.list_versions(slug)?;
        if versions.is_empty() {
            return Ok(None);
        }
        let constraint = parse_constraint(constraint_str)
            .ok_or_else(|| crate::RegistryError::InvalidConstraint(constraint_str.to_string()))?;
        let resolved = resolve_version(&versions, &constraint);
        match resolved {
            Some(v) => self.get_manifest(slug, &v),
            None => Ok(None),
        }
    }

    /// Search cassettes by keyword (matches slug, description, or goal).
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn search(&self, query: &str) -> Result<Vec<RegistryManifest>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT manifest_json FROM cassettes
             WHERE slug LIKE ?1 OR description LIKE ?1 OR goal LIKE ?1
             ORDER BY slug, version DESC",
        )?;
        let results = stmt
            .query_map(params![pattern], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect::<Vec<RegistryManifest>>();
        Ok(results)
    }

    /// List all cassette slugs.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn list_slugs(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT slug FROM cassettes ORDER BY slug")?;
        let slugs = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(slugs)
    }

    /// Grant verified status to a publisher (admin function).
    ///
    /// # Errors
    /// Surfaces SQLite errors or if publisher not found.
    pub fn verify_publisher(&self, slug: &str) -> Result<bool> {
        let now = Utc::now().timestamp_millis();
        let updated = self.conn.execute(
            "UPDATE publishers SET verified = 1, verified_at = ?1 WHERE slug = ?2",
            params![now, slug],
        )?;
        Ok(updated > 0)
    }

    /// Check if a publisher was created within the cooldown period (24h).
    /// New publishers cannot be verified until 24h after first publish.
    ///
    /// # Errors
    /// Surfaces SQLite errors.
    pub fn is_publisher_in_cooldown(&self, slug: &str) -> Result<bool> {
        let pubr = self.get_publisher(slug)?;
        match pubr {
            Some(p) => {
                let cooldown_end = p.created_at.timestamp() + 86_400; // 24 hours
                let now = Utc::now().timestamp();
                Ok(now < cooldown_end)
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RegistryManifest;

    fn test_manifest(slug: &str, version: &str) -> RegistryManifest {
        RegistryManifest {
            schema_version: 1,
            slug: slug.into(),
            version: version.into(),
            description: format!("{slug} v{version}"),
            goal: "test goal".into(),
            domains: vec!["example.com".into()],
            capabilities: vec![],
            publisher: "test-pub".into(),
            publisher_key: "abc123".into(),
            published_at: Utc::now(),
            content_hash: "hash".into(),
            compressed_size: 100,
            step_count: 1,
            trust_level: crate::manifest::TrustLevel::SelfSigned,
        }
    }

    #[test]
    fn publish_and_get() {
        let db = RegistryStore::open(":memory:").unwrap();
        let m = test_manifest("test-cassette", "1.0.0");
        db.publish(&m).unwrap();
        let loaded = db.get_manifest("test-cassette", "1.0.0").unwrap().unwrap();
        assert_eq!(loaded.slug, "test-cassette");
        assert_eq!(loaded.version, "1.0.0");
    }

    #[test]
    fn list_versions() {
        let db = RegistryStore::open(":memory:").unwrap();
        db.publish(&test_manifest("demo", "1.0.0")).unwrap();
        db.publish(&test_manifest("demo", "1.1.0")).unwrap();
        db.publish(&test_manifest("demo", "2.0.0")).unwrap();

        let versions = db.list_versions("demo").unwrap();
        assert_eq!(versions, vec!["1.0.0", "1.1.0", "2.0.0"]);
    }

    #[test]
    fn resolve_constraint() {
        let db = RegistryStore::open(":memory:").unwrap();
        db.publish(&test_manifest("demo", "1.0.0")).unwrap();
        db.publish(&test_manifest("demo", "1.5.0")).unwrap();
        db.publish(&test_manifest("demo", "2.0.0")).unwrap();

        let resolved = db.resolve("demo", "^1.0.0").unwrap().unwrap();
        assert_eq!(resolved.version, "1.5.0");

        let resolved = db.resolve("demo", "~1.0.0").unwrap().unwrap();
        assert_eq!(resolved.version, "1.0.0");

        let resolved = db.resolve("demo", "2.0.0").unwrap().unwrap();
        assert_eq!(resolved.version, "2.0.0");

        assert!(db.resolve("demo", "^3.0.0").unwrap().is_none());
    }

    #[test]
    fn search_cassettes() {
        let db = RegistryStore::open(":memory:").unwrap();
        db.publish(&test_manifest("github-login", "1.0.0")).unwrap();
        db.publish(&test_manifest("stripe-checkout", "1.0.0"))
            .unwrap();

        let results = db.search("login").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].slug, "github-login");
    }

    #[test]
    fn publisher_management() {
        let db = RegistryStore::open(":memory:").unwrap();
        let pubr = Publisher {
            slug: "test-pub".into(),
            name: "Test Publisher".into(),
            public_key_hex: "hexkey".into(),
            created_at: Utc::now(),
            verified: false,
            verified_at: None,
            cassette_count: 0,
        };
        db.upsert_publisher(&pubr).unwrap();
        let loaded = db.get_publisher("test-pub").unwrap().unwrap();
        assert_eq!(loaded.slug, "test-pub");

        db.verify_publisher("test-pub").unwrap();
        let verified = db.get_publisher("test-pub").unwrap().unwrap();
        assert!(verified.verified);
    }

    #[test]
    fn list_slugs() {
        let db = RegistryStore::open(":memory:").unwrap();
        db.publish(&test_manifest("a", "1.0.0")).unwrap();
        db.publish(&test_manifest("b", "1.0.0")).unwrap();
        let slugs = db.list_slugs().unwrap();
        assert_eq!(slugs, vec!["a", "b"]);
    }

    #[test]
    fn publisher_cooldown() {
        let db = RegistryStore::open(":memory:").unwrap();
        let pubr = Publisher {
            slug: "new-pub".into(),
            name: "New".into(),
            public_key_hex: "key".into(),
            created_at: Utc::now(),
            verified: false,
            verified_at: None,
            cassette_count: 0,
        };
        db.upsert_publisher(&pubr).unwrap();
        assert!(db.is_publisher_in_cooldown("new-pub").unwrap());
    }
}
