//! `genatp-cli` — clap-based command-line interface.
//!
//! Phase 4 surface (`v0.1.0`):
//!
//! ```text
//! genatp serve --stdio        # spawn an MCP server on stdin/stdout (default)
//! genatp serve --port 3000    # bind an HTTP MCP server on 127.0.0.1:3000
//! genatp version              # print version
//! ```
//!
//! Future phases extend this with `record`, `run`, `test`, `replay`,
//! `install` subcommands.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

/// Returns this crate's package version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Top-level CLI definition.
#[derive(Debug, Parser)]
#[command(
    name = "genatp",
    version,
    about = "genATP — agent-grade browser automation"
)]
pub struct Cli {
    /// Verbose logging — `-v` for `INFO`, `-vv` for `DEBUG`. Logs go to
    /// stderr so stdout stays clean for the MCP stdio transport.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// All supported subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print version and exit.
    Version,
    /// Start an MCP server (stdio or HTTP transport).
    Serve(ServeArgs),
    /// Manage the encrypted auth vault (cookies + localStorage).
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Record a successful agent run as a signed cassette.
    Record(RecordArgs),
    /// Manage cassettes (list, inspect, replay).
    Cassette {
        #[command(subcommand)]
        action: CassetteAction,
    },
}

/// Subcommands under `genatp vault`.
#[derive(Debug, Subcommand)]
pub enum VaultAction {
    /// List every domain stored in the vault, with entry counts.
    List,
    /// Revoke (delete) every entry under the given domain.
    Revoke {
        /// Bare domain (e.g. `github.com`).
        domain: String,
    },
    /// Print the on-disk vault path that would be used.
    Path,
}

/// Arguments for `genatp record`.
#[derive(Debug, clap::Args)]
pub struct RecordArgs {
    /// Human-readable slug for the cassette (e.g. `github-login-repo-lookup`).
    pub slug: String,
    /// URL to start the agent on.
    pub url: String,
    /// Free-form goal for the agent.
    pub goal: String,
    /// LLM model to use (e.g. `gpt-4`, `claude-sonnet-4-20250514`).
    #[arg(long, default_value = "gpt-4")]
    pub model: String,
    /// Maximum agent steps before giving up.
    #[arg(long, default_value_t = 30)]
    pub max_steps: u32,
    /// Launch Chrome in headed mode (visible window).
    #[arg(long)]
    pub headed: bool,
    /// Description for the cassette metadata.
    #[arg(long, default_value = "")]
    pub description: String,
}

/// Subcommands under `genatp cassette`.
#[derive(Debug, Subcommand)]
pub enum CassetteAction {
    /// List all stored cassettes and their versions.
    List,
    /// Inspect a cassette: decode, verify, and print its contents.
    Inspect {
        /// Cassette slug to inspect.
        slug: String,
        /// Specific version (default: latest).
        #[arg(long)]
        version: Option<String>,
    },
    /// Replay a cassette deterministically (zero LLM calls).
    /// Only actions that don't need element refs (Navigate, Scroll, Press,
    /// Wait, Finish) will succeed. Click and Fill require a running browser
    /// with a ref resolver.
    Replay {
        /// Cassette slug to replay.
        slug: String,
        /// Specific version (default: latest).
        #[arg(long)]
        version: Option<String>,
    },
}

/// Arguments for `genatp serve`.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Use the stdio (newline-delimited JSON-RPC) transport. This is
    /// the default and the transport expected by Claude Desktop,
    /// Cursor, and the MCP Inspector.
    #[arg(long, conflicts_with = "port", default_value_t = true)]
    pub stdio: bool,

    /// Bind an HTTP transport on `127.0.0.1:<port>`. Mutually exclusive
    /// with `--stdio`.
    #[arg(long)]
    pub port: Option<u16>,

    /// Launch Chrome in headed mode (visible window). Default is
    /// headless. Useful for debugging.
    #[arg(long)]
    pub headed: bool,

    /// Persistent user-data directory. Defaults to a fresh temp
    /// directory (ephemeral). Pass an existing path to reuse cookies
    /// across runs.
    #[arg(long, env = "GENATP_USER_DATA_DIR")]
    pub user_data_dir: Option<PathBuf>,
}

/// Configure tracing once. Logs unconditionally go to stderr — stdout
/// is reserved for the MCP stdio transport's JSON-RPC traffic. The
/// CLI tolerates being called more than once (subsequent calls
/// silently no-op via `try_init`).
fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Parse `argv` and dispatch. Returns the intended process exit code.
///
/// This is the synchronous entrypoint called from `main`. It builds a
/// fresh `tokio` runtime so the binary itself doesn't depend on
/// `tokio::main` — keeping the binary surface lean.
#[must_use]
pub fn run() -> i32 {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli.command {
        None => {
            // `genatp` with no args prints a banner so the user
            // isn't left wondering what to do.
            println!("genatp {} — try `genatp --help`", version());
            0
        }
        Some(Command::Version) => {
            println!("genatp {}", version());
            0
        }
        Some(Command::Serve(args)) => match run_serve(args) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: serve failed: {e:#}");
                1
            }
        },
        Some(Command::Vault { action }) => match run_vault(action) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: vault failed: {e:#}");
                1
            }
        },
        Some(Command::Record(args)) => match run_record(args) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: record failed: {e:#}");
                1
            }
        },
        Some(Command::Cassette { action }) => match run_cassette(action) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: cassette failed: {e:#}");
                1
            }
        },
    }
}

fn run_serve(args: ServeArgs) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let session_cfg = genatp_mcp::SessionConfig {
            headless: !args.headed,
            user_data_dir: args.user_data_dir.clone(),
            pool_capacity: 1,
            ..genatp_mcp::SessionConfig::default()
        };
        let session = genatp_mcp::McpSession::new(session_cfg).await?;
        let server = genatp_mcp::McpServer::new(session);

        if let Some(port) = args.port {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            tracing::info!(%addr, "starting MCP HTTP server");
            genatp_mcp::http::serve(server, addr).await?;
        } else {
            tracing::info!("starting MCP stdio server");
            genatp_mcp::stdio::serve(server).await?;
        }
        Ok::<(), anyhow::Error>(())
    })
}

fn run_record(_args: RecordArgs) -> anyhow::Result<()> {
    // Phase 7: recording requires the full orchestrator + browser. For now
    // wire the skeleton so the CLI compiles; the live implementation plugs
    // in when the orchestrator exposes a "record mode" flag (Phase 7 E2E
    // integration, tested via `cargo run -p genatp-cli -- record ...`).
    //
    // The actual recording flow is:
    //   1. Launch browser (headed/headless per --headed flag).
    //   2. Build a Task from the CLI args.
    //   3. Run the orchestrator with an EventLog.
    //   4. On success, feed the EventLog to CassetteRecorder.
    //   5. Sign and save to ~/.genatp/cassettes/<slug>/versions/1.0.0.cassette.
    eprintln!("genatp record: this command will be fully wired in Phase 7 E2E integration.");
    eprintln!("The cassette recording library is ready — see `genatp-cassette` crate.");
    eprintln!("Use `genatp cassette list` and `genatp cassette inspect` for existing cassettes.");
    Ok(())
}

fn run_cassette(action: CassetteAction) -> anyhow::Result<()> {
    match action {
        CassetteAction::List => {
            let root = genatp_cassette::cassettes_dir()
                .map_err(|e| anyhow::anyhow!("cassette storage: {e}"))?;
            let slugs = genatp_cassette::list_slugs(&root)
                .map_err(|e| anyhow::anyhow!("list slugs: {e}"))?;
            if slugs.is_empty() {
                println!("no cassettes recorded yet");
                return Ok(());
            }
            println!("{:<40} versions", "slug");
            for slug in &slugs {
                let versions = genatp_cassette::list_versions(&root, slug).unwrap_or_default();
                println!("{:<40} {}", slug, versions.join(", "));
            }
            Ok(())
        }
        CassetteAction::Inspect { slug, version } => {
            let root = genatp_cassette::cassettes_dir()
                .map_err(|e| anyhow::anyhow!("cassette storage: {e}"))?;

            let path = if let Some(v) = version {
                genatp_cassette::cassette_path(&root, &slug, &v)
            } else {
                genatp_cassette::storage::latest_version_path(&root, &slug)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            };

            // Load the signing key for verification.
            let verifying_key = genatp_cassette::signing::load_verifying_key()
                .map_err(|e| anyhow::anyhow!("cannot load signing key: {e}"))?;
            let cassette = genatp_cassette::load(&path, &verifying_key)
                .map_err(|e| anyhow::anyhow!("load failed: {e}"))?;

            println!(
                "Cassette: {} v{}",
                cassette.header.slug, cassette.header.version
            );
            println!("  Schema version: {}", cassette.header.schema_version);
            println!("  Goal:           {}", cassette.header.goal);
            println!("  Start URL:      {}", cassette.header.start_url);
            println!("  Model:          {}", cassette.header.model);
            println!("  Recorded at:    {}", cassette.header.recorded_at);
            println!("  Steps:          {}", cassette.header.step_count);
            println!("  Signer:         {}", cassette.header.signer_public_key);
            if !cassette.header.domains.is_empty() {
                println!("  Domains:        {}", cassette.header.domains.join(", "));
            }
            println!("  Signature:      VALID ✓");
            println!("  Hash chain:     VALID ✓");
            println!();
            println!("Steps:");
            for step in &cassette.steps {
                println!(
                    "  [{:>2}] {} → {:?} ({})",
                    step.seq,
                    step.url,
                    step.action,
                    if step.action_success { "ok" } else { "FAIL" }
                );
            }
            Ok(())
        }
        CassetteAction::Replay { slug, version } => run_replay(slug, version),
    }
}

fn run_replay(slug: String, version: Option<String>) -> anyhow::Result<()> {
    let root =
        genatp_cassette::cassettes_dir().map_err(|e| anyhow::anyhow!("cassette storage: {e}"))?;

    let path = if let Some(v) = version {
        genatp_cassette::cassette_path(&root, &slug, &v)
    } else {
        genatp_cassette::storage::latest_version_path(&root, &slug)
            .map_err(|e| anyhow::anyhow!("{e}"))?
    };

    let verifying_key = genatp_cassette::signing::load_verifying_key()
        .map_err(|e| anyhow::anyhow!("cannot load signing key: {e}"))?;
    let cassette = genatp_cassette::load(&path, &verifying_key)
        .map_err(|e| anyhow::anyhow!("load failed: {e}"))?;

    println!(
        "Cassette: {} v{} — replaying {} steps without LLM calls...",
        cassette.header.slug,
        cassette.header.version,
        cassette.steps.len()
    );
    println!("  Goal:       {}", cassette.header.goal);
    println!("  Start URL:  {}", cassette.header.start_url);
    println!();

    // Phase 8: full replay with a live browser requires launching Chrome
    // and building a ref resolver from the accessibility tree. For now we
    // report what would happen; the library is ready (see `genatp-cassette`
    // crate and its tests).
    println!("  ⚠  Full deterministic replay requires a running browser.");
    println!("     The `genatp-cassette` replay engine is production-ready:");
    println!("     - Zero LLM calls: actions execute directly from the cassette");
    println!("     - Ref resolver: Click/Fill actions need live page refs");
    println!("     - URL tracking: each step checks URL matches recording");
    println!();
    println!(
        "  Use `genatp cassette inspect {}` to see the steps.",
        cassette.header.slug
    );
    println!();

    // Print a dry-run of what would happen.
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    for step in &cassette.steps {
        let needs_ref = matches!(
            &step.action,
            genatp_core::action::Action::Click { .. } | genatp_core::action::Action::Fill { .. }
        );
        if needs_ref {
            failed += 1;
            println!(
                "  [{:>2}] ⚠ {:?} — needs ref resolver",
                step.seq, step.action
            );
        } else {
            succeeded += 1;
            println!("  [{:>2}] ✓ {:?}", step.seq, step.action);
        }
    }
    println!();
    println!("  Dry-run: {succeeded} would succeed, {failed} would need a ref resolver");

    Ok(())
}

fn run_vault(action: VaultAction) -> anyhow::Result<()> {
    use std::sync::Arc;

    use genatp_vault::{OsKeyStore, Vault, default_vault_path};

    let path = default_vault_path()?;
    match action {
        VaultAction::Path => {
            println!("{}", path.display());
            Ok(())
        }
        VaultAction::List => {
            let ks: Arc<dyn genatp_vault::KeyStore> = Arc::new(OsKeyStore::new());
            // Don't auto-init for `list` — if nothing's there, say so.
            let v = match Vault::open(&path, ks) {
                Ok(v) => v,
                Err(genatp_vault::VaultError::NotInitialized) => {
                    println!("vault not initialised yet — log in via an agent first");
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };
            let rows = v.list()?;
            if rows.is_empty() {
                println!("vault is empty");
            } else {
                println!("{:<40} {:>6}  updated", "domain", "count");
                for r in rows {
                    let ts = chrono::DateTime::from_timestamp_millis(r.updated_at_ms)
                        .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_default();
                    println!("{:<40} {:>6}  {ts}", r.domain, r.entry_count);
                }
            }
            Ok(())
        }
        VaultAction::Revoke { domain } => {
            let ks: Arc<dyn genatp_vault::KeyStore> = Arc::new(OsKeyStore::new());
            let mut v = Vault::open(&path, ks)?;
            let n = v.revoke(&domain)?;
            if n == 0 {
                println!("no entries for `{domain}` (already revoked or never stored)");
            } else {
                println!("revoked all entries for `{domain}`");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn parse_serve_stdio_default() {
        let cli = Cli::try_parse_from(["genatp", "serve"]).unwrap();
        match cli.command {
            Some(Command::Serve(a)) => {
                assert!(a.stdio);
                assert!(a.port.is_none());
            }
            _ => panic!("expected serve subcommand"),
        }
    }

    #[test]
    fn parse_serve_port() {
        let cli = Cli::try_parse_from(["genatp", "serve", "--port", "3000"]).unwrap();
        match cli.command {
            Some(Command::Serve(a)) => assert_eq!(a.port, Some(3000)),
            _ => panic!("expected serve subcommand"),
        }
    }

    #[test]
    fn parse_serve_rejects_stdio_and_port_together() {
        let err = Cli::try_parse_from(["genatp", "serve", "--stdio", "--port", "3000"])
            .expect_err("must conflict");
        assert!(format!("{err}").contains("cannot be used with"));
    }

    #[test]
    fn parse_version_subcommand() {
        let cli = Cli::try_parse_from(["genatp", "version"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Version)));
    }

    #[test]
    fn parse_headed_and_user_data_dir() {
        let cli = Cli::try_parse_from(["genatp", "serve", "--headed", "--user-data-dir", "/tmp/x"])
            .unwrap();
        match cli.command {
            Some(Command::Serve(a)) => {
                assert!(a.headed);
                assert_eq!(a.user_data_dir, Some(PathBuf::from("/tmp/x")));
            }
            _ => panic!("expected serve"),
        }
    }

    #[test]
    fn parse_record_subcommand() {
        let cli = Cli::try_parse_from([
            "genatp",
            "record",
            "my-slug",
            "https://example.com",
            "Click the button",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Record(a)) => {
                assert_eq!(a.slug, "my-slug");
                assert_eq!(a.url, "https://example.com");
                assert_eq!(a.goal, "Click the button");
                assert_eq!(a.model, "gpt-4"); // default
                assert_eq!(a.max_steps, 30); // default
                assert!(!a.headed);
            }
            _ => panic!("expected record subcommand"),
        }
    }

    #[test]
    fn parse_record_with_options() {
        let cli = Cli::try_parse_from([
            "genatp",
            "record",
            "my-slug",
            "https://example.com",
            "goal",
            "--model",
            "claude-sonnet-4-20250514",
            "--max-steps",
            "10",
            "--headed",
            "--description",
            "Login flow",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Record(a)) => {
                assert_eq!(a.model, "claude-sonnet-4-20250514");
                assert_eq!(a.max_steps, 10);
                assert!(a.headed);
                assert_eq!(a.description, "Login flow");
            }
            _ => panic!("expected record subcommand"),
        }
    }

    #[test]
    fn parse_cassette_list() {
        let cli = Cli::try_parse_from(["genatp", "cassette", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Cassette {
                action: CassetteAction::List
            })
        ));
    }

    #[test]
    fn parse_cassette_inspect() {
        let cli = Cli::try_parse_from(["genatp", "cassette", "inspect", "my-slug"]).unwrap();
        match cli.command {
            Some(Command::Cassette {
                action: CassetteAction::Inspect { slug, version },
            }) => {
                assert_eq!(slug, "my-slug");
                assert!(version.is_none());
            }
            _ => panic!("expected cassette inspect subcommand"),
        }
    }

    #[test]
    fn parse_cassette_inspect_with_version() {
        let cli = Cli::try_parse_from([
            "genatp",
            "cassette",
            "inspect",
            "my-slug",
            "--version",
            "1.1.0",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Cassette {
                action: CassetteAction::Inspect { slug, version },
            }) => {
                assert_eq!(slug, "my-slug");
                assert_eq!(version, Some("1.1.0".into()));
            }
            _ => panic!("expected cassette inspect subcommand"),
        }
    }

    #[test]
    fn parse_cassette_replay() {
        let cli = Cli::try_parse_from(["genatp", "cassette", "replay", "my-slug"]).unwrap();
        match cli.command {
            Some(Command::Cassette {
                action: CassetteAction::Replay { slug, version },
            }) => {
                assert_eq!(slug, "my-slug");
                assert!(version.is_none());
            }
            _ => panic!("expected cassette replay subcommand"),
        }
    }

    #[test]
    fn parse_cassette_replay_with_version() {
        let cli = Cli::try_parse_from([
            "genatp",
            "cassette",
            "replay",
            "my-slug",
            "--version",
            "1.1.0",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Cassette {
                action: CassetteAction::Replay { slug, version },
            }) => {
                assert_eq!(slug, "my-slug");
                assert_eq!(version, Some("1.1.0".into()));
            }
            _ => panic!("expected cassette replay subcommand"),
        }
    }
}
