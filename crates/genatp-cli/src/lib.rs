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
}
