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
    /// Parse, validate, and list ATP specs.
    Atp {
        #[command(subcommand)]
        action: AtpAction,
    },
    /// Replay a past run from its event log (time-travel debugger).
    Replay(ReplayArgs),
    /// Manage the cassette registry (search, install, publish).
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },
}

/// Subcommands under `genatp registry`.
#[derive(Debug, Subcommand)]
pub enum RegistryAction {
    /// Search cassettes in the registry by keyword.
    Search {
        /// Search query.
        query: String,
    },
    /// Show information about a cassette.
    Info {
        /// Cassette slug.
        slug: String,
        /// Version constraint (default: latest).
        #[arg(default_value = "latest")]
        version: String,
    },
    /// Install a cassette from the registry.
    Install {
        /// Cassette slug.
        slug: String,
        /// Version constraint (default: latest).
        #[arg(default_value = "latest")]
        version: String,
        /// Skip confirmation prompt for self-signed cassettes.
        #[arg(long)]
        yes: bool,
    },
    /// Publish a cassette to the registry.
    Publish {
        /// Path to the cassette file.
        path: std::path::PathBuf,
        /// Publisher slug.
        publisher: String,
    },
    /// List all registered publishers.
    Publishers,
    /// Start the registry API server.
    Serve {
        /// Port to serve on.
        #[arg(long, default_value_t = 3002)]
        port: u16,
    },
}

/// Subcommands under `genatp atp`.
#[derive(Debug, Subcommand)]
pub enum AtpAction {
    /// Parse and validate a .atp.yml spec file.
    Validate {
        /// Path to the .atp.yml file.
        path: PathBuf,
    },
    /// Dump the parsed IR as pretty JSON.
    Dump {
        /// Path to the .atp.yml file.
        path: PathBuf,
    },
    /// List .atp.yml files in a directory (default: current dir).
    List {
        /// Directory to search.
        #[arg(default_value = ".")]
        directory: PathBuf,
    },
    /// Print a human-readable step summary for a spec.
    Steps {
        /// Path to the .atp.yml file.
        path: PathBuf,
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
    /// Path to the Chrome / Chromium executable. Auto-detected if not
    /// set (checks `CHROME_PATH` env var, Windows registry, and common
    /// installation directories).
    #[arg(long, env = "CHROME_PATH")]
    pub chrome_path: Option<PathBuf>,
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
    /// Launches a browser, navigates to the start URL, builds a ref
    /// resolver from the accessibility tree, and executes every step.
    /// Click and Fill actions are resolved against live DOM nodes.
    Replay {
        /// Cassette slug to replay.
        slug: String,
        /// Specific version (default: latest).
        #[arg(long)]
        version: Option<String>,
        /// Launch Chrome in headed mode (visible window).
        #[arg(long)]
        headed: bool,
        /// Path to the Chrome / Chromium executable. Auto-detected if
        /// not set (checks `CHROME_PATH` env var, Windows registry,
        /// and common installation directories).
        #[arg(long, env = "CHROME_PATH")]
        chrome_path: Option<PathBuf>,
    },
}

/// Arguments for `genatp replay` (time-travel debugger).
#[derive(Debug, clap::Args)]
pub struct ReplayArgs {
    /// Run identifier (event log filename stem). Required for CLI
    /// replay mode. Optional in `--serve` mode — the API server serves
    /// all available runs regardless.
    pub run_id: Option<String>,
    /// Step sequence number to show (default: last).
    #[arg(long, short = 'a')]
    pub at: Option<u64>,
    /// Launch an interactive API server for the web UI.
    #[arg(long)]
    pub serve: bool,
    /// Port for the API server.
    #[arg(long, default_value_t = 3001)]
    pub port: u16,
    /// Path to snapshot CAS store.
    #[arg(long = "snapshot-store")]
    pub snapshot_store_path: Option<PathBuf>,
    /// Path to runs directory.
    #[arg(long = "runs-dir")]
    pub runs_dir: Option<PathBuf>,
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

    /// Path to the Chrome / Chromium executable. Auto-detected if not
    /// set (checks `CHROME_PATH` env var, Windows registry, and common
    /// installation directories).
    #[arg(long, env = "CHROME_PATH")]
    pub chrome_path: Option<PathBuf>,

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
        Some(Command::Atp { action }) => match run_atp(action) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: atp failed: {e:#}");
                1
            }
        },
        Some(Command::Replay(args)) => match run_debugger_replay(args) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: replay failed: {e:#}");
                1
            }
        },
        Some(Command::Registry { action }) => match run_registry(action) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("genatp: registry failed: {e:#}");
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
            chrome_path: args.chrome_path.clone(),
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
        CassetteAction::Replay {
            slug,
            version,
            headed,
            chrome_path,
        } => run_replay(slug, version, headed, chrome_path),
    }
}

#[allow(clippy::field_reassign_with_default)]
fn run_replay(
    slug: String,
    version: Option<String>,
    headed: bool,
    chrome_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    use std::collections::HashMap;

    use genatp_browser::LaunchOptions;
    use genatp_browser::launch::HeadlessMode;
    use genatp_browser::pool::{BrowserPool, PoolConfig};
    use genatp_core::action::Action;
    use genatp_core::dispatcher::BrowserAdapter;
    use genatp_core::page_adapter::PageAdapter;
    use genatp_core::perception_bridge::raw_tree_from_accessibility;
    use genatp_perception::{RefMap, StripPolicy, strip};

    // ── Load cassette ────────────────────────────────────────────────
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

    let step_count = cassette.steps.len();
    println!(
        "Cassette: {} v{} — live replay with browser",
        cassette.header.slug, cassette.header.version,
    );
    println!("  Goal:       {}", cassette.header.goal);
    println!("  Start URL:  {}", cassette.header.start_url);
    println!("  Steps:      {step_count} (zero LLM calls)");
    println!();

    let replayer = genatp_cassette::CassetteReplayer::new(cassette);

    // ── Launch browser ───────────────────────────────────────────────
    let mut launch = LaunchOptions::default();
    launch.headless = if headed {
        HeadlessMode::Visible
    } else {
        HeadlessMode::New
    };
    if let Some(path) = chrome_path {
        launch.chrome_executable = Some(path);
    }
    let pool_cfg = PoolConfig::default()
        .launch(launch)
        .max_concurrency(1)
        .disable_stealth();
    let pool = BrowserPool::new(pool_cfg);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        // ── Helper: build ref map from current page state ──────────────
        async fn build_ref_map(
            adapter: &dyn BrowserAdapter,
        ) -> anyhow::Result<HashMap<String, String>> {
            let url = adapter
                .current_url()
                .await
                .unwrap_or(None)
                .unwrap_or_default();
            let tree = adapter.accessibility().await?;
            let raw = raw_tree_from_accessibility(&url, &tree);
            let stripped = strip(&raw, &StripPolicy::default());
            let mut refmap = RefMap::new();
            let node_to_ref = refmap.assign_for_tree(&stripped);

            let a11y_to_backend: HashMap<String, String> = tree
                .nodes()
                .iter()
                .filter_map(|n| {
                    n.backend_dom_node_id
                        .map(|b| (n.node_id.clone(), b.to_string()))
                })
                .collect();

            let ref_to_backend: HashMap<String, String> = node_to_ref
                .iter()
                .filter_map(|(a11y_id, r)| {
                    a11y_to_backend.get(a11y_id).map(|b| (r.clone(), b.clone()))
                })
                .collect();

            Ok(ref_to_backend)
        }

        // Acquire page
        let page_guard = pool
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!("failed to acquire browser page: {e}"))?;
        let adapter = PageAdapter::new(page_guard.page().clone());
        println!("  ✓ Browser launched");

        // Navigate to start URL
        let start_url = replayer.cassette().header.start_url.clone();
        adapter
            .navigate(&start_url)
            .await
            .map_err(|e| anyhow::anyhow!("navigation to {start_url} failed: {e}"))?;
        let current_url = adapter
            .current_url()
            .await
            .unwrap_or(None)
            .unwrap_or_default();
        println!("  ✓ Navigated to {current_url}");
        println!();

        // ── Execute steps ────────────────────────────────────────────
        let mut succeeded = 0u32;
        let mut failed = 0u32;
        let mut ref_to_backend: HashMap<String, String> = HashMap::new();

        for step in &replayer.cassette().steps {
            // Refresh ref resolver before steps that need element refs
            let needs_ref = matches!(&step.action, Action::Click { .. } | Action::Fill { .. });
            if needs_ref {
                match build_ref_map(&adapter).await {
                    Ok(map) => ref_to_backend = map,
                    Err(e) => {
                        eprintln!("  [{:>2}] ⚠ snapshot failed: {e}", step.seq);
                    }
                }
            }

            let result = replayer
                .execute_step(
                    &adapter,
                    &|ref_str| ref_to_backend.get(ref_str).cloned(),
                    step,
                )
                .await
                .unwrap_or_else(|e| genatp_cassette::ReplayStepResult {
                    seq: step.seq,
                    action: step.action.clone(),
                    status: genatp_cassette::ReplayStepStatus::Failed {
                        reason: e.to_string(),
                    },
                    url: step.url.clone(),
                });

            match &result.status {
                genatp_cassette::ReplayStepStatus::Success { url_matches } => {
                    succeeded += 1;
                    let url_flag = if *url_matches {
                        String::new()
                    } else {
                        format!(" (URL: {} → {})", step.url, result.url.as_str())
                    };
                    println!("  [{:>2}] ✓ {:?}{}", step.seq, step.action, url_flag);
                }
                genatp_cassette::ReplayStepStatus::Failed { reason } => {
                    failed += 1;
                    println!("  [{:>2}] ✗ {:?} — {}", step.seq, step.action, reason);
                }
            }

            if matches!(result.action, Action::Finish { .. }) {
                break;
            }
        }

        // ── Cleanup ──────────────────────────────────────────────────
        drop(adapter);
        page_guard.close().await.unwrap_or_else(|e| {
            eprintln!("  ⚠ page close: {e}");
        });
        pool.shutdown().await.unwrap_or_else(|e| {
            eprintln!("  ⚠ browser shutdown: {e}");
        });

        println!();
        println!("  Result: {succeeded} succeeded, {failed} failed");
        if failed == 0 {
            println!("  ✓ All steps replayed successfully (zero LLM tokens)");
        }

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn run_atp(action: AtpAction) -> anyhow::Result<()> {
    match action {
        AtpAction::Validate { path } => {
            let result = genatp_atp_spec::cli::validate_spec_pretty(&path);
            println!("{result}");
            Ok(())
        }
        AtpAction::Dump { path } => {
            let json =
                genatp_atp_spec::cli::dump_spec_json(&path).map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        AtpAction::List { directory } => {
            let specs = genatp_atp_spec::cli::list_specs(&directory)?;
            if specs.is_empty() {
                println!("no .atp.yml files found in {}", directory.display());
            } else {
                for spec in &specs {
                    println!("{}", spec.display());
                }
            }
            Ok(())
        }
        AtpAction::Steps { path } => {
            let out =
                genatp_atp_spec::cli::print_steps(&path).map_err(|e| anyhow::anyhow!("{e}"))?;
            print!("{out}");
            Ok(())
        }
    }
}

fn run_debugger_replay(args: ReplayArgs) -> anyhow::Result<()> {
    // Determine the runs directory.
    let base_dir = if let Some(runs_dir) = &args.runs_dir {
        runs_dir.clone()
    } else {
        let data_dir = directories::BaseDirs::new()
            .map(|d| d.data_dir().join("genatp"))
            .unwrap_or_else(|| PathBuf::from("./genatp-data"));
        data_dir.join("runs")
    };

    // In CLI replay mode (non-serve), report a helpful error if the runs
    // directory doesn't exist yet. In --serve mode the API just shows an
    // empty list, so we always proceed.
    if !args.serve && !base_dir.exists() {
        eprintln!("genatp: runs directory not found at {}", base_dir.display());
        eprintln!(
            "  No event logs exist — run a task first using `genatp record` or the MCP server."
        );
        return Ok(());
    }

    // Optionally open snapshot store.
    let snapshot_store = match &args.snapshot_store_path {
        Some(path) => match genatp_perception::SnapshotStore::open(path) {
            Ok(store) => Some(std::sync::Arc::new(std::sync::Mutex::new(store))),
            Err(e) => {
                eprintln!(
                    "genatp: warning: could not open snapshot store at {}: {e}",
                    path.display()
                );
                eprintln!(
                    "  Continuing without snapshot resolution — snapshots will show hashes only."
                );
                None
            }
        },
        None => None,
    };

    if args.serve {
        // Launch the debugger API server. Run ID is optional here — the
        // API serves all available runs.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move {
            let state = genatp_debugger::api::AppState {
                runs_dir: base_dir,
                snapshot_store,
            };
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], args.port));
            println!("Starting debugger API server at http://{addr}");
            println!("  Open http://localhost:{}/runs to see all runs", args.port);
            println!("  Use a run ID for CLI replay: genatp replay <RUN_ID> [--at N]");
            genatp_debugger::api::serve(state, &addr).await?;
            Ok::<(), anyhow::Error>(())
        })?;
    } else {
        // CLI replay mode — run_id is required.
        use genatp_debugger::replay_to_step;
        let run_id = args.run_id.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "a run ID is required for CLI replay mode\n  Usage: genatp replay <RUN_ID> [--at N]"
            )
        })?;
        let result = replay_to_step(&base_dir, run_id, args.at)?;

        println!(
            "Run: {} ({} entries, {} steps, outcome: {})",
            result.summary.id,
            result.summary.entry_count,
            result.summary.step_count,
            result.summary.outcome,
        );
        println!("Hash chain valid: {}", result.summary.hash_chain_valid);
        if let Some(bad) = result.summary.first_bad_seq {
            println!("  ⚠ First bad entry at seq {bad}");
        }
        println!("Goal: {}", result.summary.goal);
        println!();

        if let Some(step) = &result.current_step {
            println!("Current step [{}]", step.seq);
            println!("  Type:     {}", step.event_type);
            println!("  Time:     {}", step.at);
            println!("  Summary:  {}", step.summary);
            if let Some(hash) = &step.snapshot_hash {
                println!("  Snapshot: {hash}");
            }
            if let Some(desc) = &step.action_description {
                println!("  Action:   {desc}");
            }
        } else {
            println!("No events found at the requested step.");
        }

        if !result.is_at_end {
            println!();
            println!(
                "Use `--at {}` to advance to the next step.",
                result.current_step.as_ref().map(|s| s.seq + 1).unwrap_or(0)
            );
        }
    }

    Ok(())
}

fn run_registry(action: RegistryAction) -> anyhow::Result<()> {
    match action {
        RegistryAction::Search { query } => {
            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            let results = genatp_registry::search(&store, &query)?;

            if results.is_empty() {
                println!("No cassettes found matching \"{query}\"");
            } else {
                let slugs: std::collections::BTreeSet<&str> =
                    results.iter().map(|m| m.slug.as_str()).collect();
                println!("Found {} cassette(s):", slugs.len());
                for slug in &slugs {
                    let versions: Vec<&str> = results
                        .iter()
                        .filter(|m| m.slug == *slug)
                        .map(|m| m.version.as_str())
                        .collect();
                    let m = results.iter().find(|m| m.slug == *slug).unwrap();
                    println!(
                        "  {slug} ({}) — {} — {} step(s)",
                        versions.join(", "),
                        m.publisher,
                        m.step_count
                    );
                }
            }
            Ok(())
        }
        RegistryAction::Info { slug, version } => {
            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            let manifest = store.resolve(&slug, &version)?;
            match manifest {
                Some(m) => {
                    println!("{}", genatp_registry::format_manifest_info(&m));
                    Ok(())
                }
                None => {
                    eprintln!("cassette {slug} matching \"{version}\" not found");
                    Ok(())
                }
            }
        }
        RegistryAction::Install { slug, version, yes } => {
            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            match genatp_registry::install(&store, &slug, &version, yes) {
                Ok(installed) => {
                    println!("{}", genatp_registry::format_install_info(&installed));
                    println!("Use `genatp cassette inspect {}` to see the steps.", slug);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("install failed: {e}");
                    if format!("{e}").contains("requires confirmation") {
                        eprintln!("  Pass `--yes` to confirm self-signed installation.");
                    }
                    Ok(())
                }
            }
        }
        RegistryAction::Publish { path, publisher } => {
            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            let signing_key = genatp_cassette::signing::load_signing_key()
                .map_err(|e| anyhow::anyhow!("cannot load signing key: {e}"))?;
            let verifying_key = signing_key.verifying_key();

            let cassette_bytes = std::fs::read(&path)?;
            let cassette: genatp_cassette::Cassette = serde_json::from_slice(&cassette_bytes)
                .map_err(|e| anyhow::anyhow!("invalid cassette JSON: {e}"))?;

            let manifest = genatp_registry::publish(
                &store,
                &cassette,
                &publisher,
                &signing_key,
                &verifying_key,
            )?;
            println!(
                "Published {}@{} from {}",
                manifest.slug,
                manifest.version,
                path.display()
            );
            Ok(())
        }
        RegistryAction::Publishers => {
            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            let publishers = genatp_registry::list_publishers(&store)?;
            if publishers.is_empty() {
                println!("No registered publishers.");
            } else {
                println!("{:<30} {:>8}  verified", "slug", "cassettes");
                for p in &publishers {
                    println!(
                        "{:<30} {:>8}  {}",
                        p.slug,
                        p.cassette_count,
                        if p.verified {
                            "✓ verified"
                        } else {
                            "self-signed"
                        }
                    );
                }
            }
            Ok(())
        }
        RegistryAction::Serve { port } => {
            use genatp_registry::api::AppState;

            let db_path = genatp_registry::default_registry_dir()?.join("registry.db");
            let store = genatp_registry::RegistryStore::open(&db_path)?;
            let blob_dir = genatp_registry::default_registry_dir()?.join("blobs");
            std::fs::create_dir_all(&blob_dir)?;

            let state = AppState {
                store: std::sync::Arc::new(std::sync::Mutex::new(store)),
                blob_dir,
                known_publishers: genatp_registry::list_publishers(
                    &genatp_registry::RegistryStore::open(&db_path)?,
                )?,
            };

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(async move {
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                println!("Starting registry API server at http://{addr}");
                println!("  OCI API:   http://localhost:{port}/v2/");
                println!("  Search:    http://localhost:{port}/search?q=<query>");
                println!("  Health:    http://localhost:{port}/health");
                genatp_registry::api::serve(state, &addr).await?;
                Ok::<(), anyhow::Error>(())
            })?;
            Ok(())
        }
    }
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
                action:
                    CassetteAction::Replay {
                        slug,
                        version,
                        headed: _,
                        chrome_path: _,
                    },
            }) => {
                assert_eq!(slug, "my-slug");
                assert!(version.is_none());
            }
            _ => panic!("expected cassette replay subcommand"),
        }
    }

    #[test]
    fn parse_atp_validate() {
        let cli = Cli::try_parse_from(["genatp", "atp", "validate", "test.atp.yml"]).unwrap();
        match cli.command {
            Some(Command::Atp {
                action: AtpAction::Validate { path },
            }) => {
                assert_eq!(path, PathBuf::from("test.atp.yml"));
            }
            _ => panic!("expected atp validate subcommand"),
        }
    }

    #[test]
    fn parse_atp_dump() {
        let cli = Cli::try_parse_from(["genatp", "atp", "dump", "spec.atp.yml"]).unwrap();
        match cli.command {
            Some(Command::Atp {
                action: AtpAction::Dump { path },
            }) => assert_eq!(path, PathBuf::from("spec.atp.yml")),
            _ => panic!("expected atp dump"),
        }
    }

    #[test]
    fn parse_atp_list() {
        let cli = Cli::try_parse_from(["genatp", "atp", "list"]).unwrap();
        match cli.command {
            Some(Command::Atp {
                action: AtpAction::List { .. },
            }) => {} // ok
            _ => panic!("expected atp list"),
        }
    }

    #[test]
    fn parse_atp_list_with_dir() {
        let cli = Cli::try_parse_from(["genatp", "atp", "list", "fixtures/"]).unwrap();
        match cli.command {
            Some(Command::Atp {
                action: AtpAction::List { directory },
            }) => assert_eq!(directory, PathBuf::from("fixtures/")),
            _ => panic!("expected atp list with dir"),
        }
    }

    #[test]
    fn parse_atp_steps() {
        let cli = Cli::try_parse_from(["genatp", "atp", "steps", "spec.atp.yml"]).unwrap();
        match cli.command {
            Some(Command::Atp {
                action: AtpAction::Steps { path },
            }) => assert_eq!(path, PathBuf::from("spec.atp.yml")),
            _ => panic!("expected atp steps"),
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
                action:
                    CassetteAction::Replay {
                        slug,
                        version,
                        headed: _,
                        chrome_path: _,
                    },
            }) => {
                assert_eq!(slug, "my-slug");
                assert_eq!(version, Some("1.1.0".into()));
            }
            _ => panic!("expected cassette replay subcommand"),
        }
    }
}
