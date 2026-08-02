//! Brightflow command-line entry point.
//!
//! Thin dispatch over the library crates: every subcommand resolves a workspace,
//! constructs the store/scheduler/API pieces it needs, and delegates. Logic that
//! could be reused belongs in a crate rather than here — this file is the place
//! where argument shapes and human-readable output are decided, nothing else.

// Allow certain pedantic lints that are too strict for CLI code:
// - cognitive_complexity: CLI functions often have many branches
// - ref_option: &Option<T> is idiomatic in argument parsing
// - or_fun_call: unwrap_or with constant is more readable
// - print_stdout: CLI apps need to print output to users
// - too_many_lines: CLI handler functions are naturally verbose
// - case_sensitive_file_extension_comparisons: "lua" is always lowercase
// - shadow_unrelated/shadow_reuse: variable shadowing for option resolution is idiomatic
#![allow(
    clippy::cognitive_complexity,
    clippy::ref_option,
    clippy::or_fun_call,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::shadow_unrelated,
    clippy::shadow_reuse
)]

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Configure jemalloc to return freed memory to the OS immediately.
///
/// By default jemalloc holds dirty pages for ~10s before returning them (dirty_decay_ms).
/// With tokio's async workload and frequent dataset switches, this causes RSS to grow
/// because freed pages from the old dataset haven't been returned before the new one loads.
/// Setting decay to 0 forces immediate return on every deallocation.
#[cfg(not(target_env = "msvc"))]
#[allow(unsafe_code)]
fn tune_jemalloc() {
    // SAFETY: mallctl write with correct key names and value type (ssize_t = isize).
    // These are documented jemalloc configuration knobs, not arbitrary memory operations.
    unsafe {
        let _ = tikv_jemalloc_ctl::raw::write(b"arenas.dirty_decay_ms\0", 0_isize);
        let _ = tikv_jemalloc_ctl::raw::write(b"arenas.muzzy_decay_ms\0", 0_isize);
    }
}

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use polars::prelude::SerWriter;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing_subscriber::{
    fmt::writer::{BoxMakeWriter, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use brightflow_api::system::log_layer::{LogBroadcastLayer, LogEntry};
use brightflow_api::ServeConfig;
use brightflow_connect::list_builtin_connectors;
use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::scoring::ScoringContext;
use brightflow_engine::analysis::tree::{ReportType, ReviewCadence};
use brightflow_engine::data::loader::load_csv;
use brightflow_engine::data::schema::detect_schema;
use brightflow_engine::debug::DebugLog;
use brightflow_engine::output::html::write_html;
use brightflow_engine::output::json::write_output;
use brightflow_engine::output::markdown::write_markdown;
// Scheduler is now integrated into the API server
use brightflow_store::{IngestMode, IngestOptions, ParquetStore};

#[derive(Parser, Debug)]
#[command(name = "brightflow")]
#[command(about = "Brightflow analytics platform")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start both API server and scheduler
    RunAll {
        /// Host address to bind to
        #[arg(long)]
        host: Option<String>,

        /// Port to listen on
        #[arg(long)]
        port: Option<u16>,

        /// Default dataset to load on startup
        #[arg(long)]
        dataset: Option<String>,

        /// Path to Parquet store to auto-load tables from
        #[arg(long)]
        store: Option<String>,

        /// Specific tables to load (comma-separated, loads all if not specified)
        #[arg(long)]
        tables: Option<String>,

        /// Path to connector config directory
        #[arg(long)]
        connector_configs: Option<String>,

        /// SQLite database URL for auth/sessions
        #[arg(long)]
        database_url: Option<String>,
    },

    /// Start the API server only
    Serve {
        /// Host address to bind to
        #[arg(long)]
        host: Option<String>,

        /// Port to listen on
        #[arg(long)]
        port: Option<u16>,

        /// Default dataset to load on startup
        #[arg(long)]
        dataset: Option<String>,

        /// Path to Parquet store to auto-load tables from
        #[arg(long)]
        store: Option<String>,

        /// Specific tables to load (comma-separated, loads all if not specified)
        #[arg(long)]
        tables: Option<String>,

        /// Path to connector config directory
        #[arg(long)]
        connector_configs: Option<String>,

        /// SQLite database URL for auth/sessions
        #[arg(long)]
        database_url: Option<String>,
    },

    /// Start the scheduler daemon only (not yet implemented)
    Schedule,

    /// Run statistical analysis on data
    #[command(subcommand)]
    Insights(InsightsCommands),

    /// Data connector operations
    #[command(subcommand)]
    Connect(ConnectCommands),

    /// Parquet store operations
    #[command(subcommand)]
    Store(StoreCommands),

    /// Create an admin user
    CreateAdmin {
        /// Admin email address
        #[arg(long)]
        email: String,

        /// Admin display name
        #[arg(long)]
        name: String,

        /// SQLite database URL for auth (auto-derived from BRIGHTFLOW_DATA_DIR if not set)
        #[arg(long)]
        database_url: Option<String>,
    },

    /// Manage topic clusters (Model2Vec embeddings + k-means)
    Topics {
        #[command(subcommand)]
        action: TopicsAction,
    },

    /// Register existing event Parquet files in the Litehouse catalog
    MigrateEvents,

    /// Compact event files in a partition into a single file
    Compact {
        /// Source id (e.g. `web:<uuid>` — matches the source the table belongs to)
        #[arg(long)]
        source: String,

        /// Table name (e.g., events_<source_id>)
        #[arg(long)]
        table: String,

        /// Date partition to compact (YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,

        /// Compact all date partitions
        #[arg(long)]
        all_dates: bool,
    },
}

#[derive(Subcommand, Debug)]
enum InsightsCommands {
    /// Run review analysis: anomaly detection with attribution
    Review {
        #[command(flatten)]
        args: AnalyzeArgs,

        /// Review cadence: daily, weekly, monthly, or all
        #[arg(long, default_value = "all")]
        cadence: CadenceArg,
    },
    /// Run trends analysis: time-based patterns and forecasting
    Trends {
        #[command(flatten)]
        args: AnalyzeArgs,
    },
    /// Run drivers analysis: composition and driver analysis
    Drivers {
        #[command(flatten)]
        args: AnalyzeArgs,
    },
    /// Run all analyses (review, trends, drivers)
    All {
        #[command(flatten)]
        args: AnalyzeArgs,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum CadenceArg {
    Daily,
    Weekly,
    Monthly,
    All,
}

#[derive(Subcommand, Debug)]
enum ConnectCommands {
    /// List available built-in connectors
    List,
}

#[derive(Subcommand, Debug)]
enum TopicsAction {
    /// Fit a fresh model: embed all rows, refit TF-IDF + clusters + label centroids
    Fit {
        /// Source id (e.g. `connector:<uuid>`)
        #[arg(long)]
        source: String,

        /// Table name (default: issues)
        #[arg(long, default_value = "issues")]
        table: String,

        /// Number of topic clusters (k for k-means). Defaults to engine DEFAULT_K.
        #[arg(long)]
        clusters: Option<usize>,
    },

    /// List existing clusters for a source/table
    List {
        /// Source id
        #[arg(long)]
        source: String,

        /// Table name
        #[arg(long, default_value = "issues")]
        table: String,
    },

    /// A/B evaluate clustering algorithms and k on real data:
    /// prints silhouette, Davies-Bouldin, NPMI coherence, unassigned %, wall time
    Eval {
        /// Source id
        #[arg(long)]
        source: String,

        /// Table name
        #[arg(long, default_value = "issues")]
        table: String,

        /// Comma-separated algorithms: kmeans, hdbscan
        #[arg(long, default_value = "kmeans,hdbscan")]
        algorithms: String,

        /// k for k-means runs
        #[arg(long, default_value_t = 12)]
        k: usize,
    },

    /// Embed rows only (no clustering refit) - useful after model swap
    Embed {
        /// Source id
        #[arg(long)]
        source: String,

        /// Table name
        #[arg(long, default_value = "issues")]
        table: String,
    },

    /// Report near-duplicate rows (read-only): groups rows whose text is
    /// substantially the same. Does NOT find differently-worded reports of the
    /// same issue.
    NearDup {
        /// Source id
        #[arg(long)]
        source: String,

        /// Table name
        #[arg(long, default_value = "issues")]
        table: String,

        /// Cosine similarity threshold. Defaults to the engine's tuned value.
        #[arg(long)]
        threshold: Option<f32>,
    },

    /// Compare the trained classifier head against the nearest-centroid
    /// baseline on curated labels. Read-only — the go/no-go check.
    EvalClassifier {
        /// Source id
        #[arg(long)]
        source: String,

        /// Table name
        #[arg(long, default_value = "issues")]
        table: String,
    },
}

#[derive(Subcommand, Debug)]
enum StoreCommands {
    /// List all tables in the store
    List {
        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Show information about a table
    Info {
        /// Source id (e.g. `connector:<uuid>` or `web:<uuid>`)
        #[arg(long)]
        source: String,

        /// Table name
        name: String,

        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Ingest a Parquet file into a table
    Ingest {
        /// Source id (e.g. `connector:<uuid>` or `web:<uuid>`)
        #[arg(long)]
        source: String,

        /// Table name to create/append to
        table: String,

        /// Path to Parquet file(s) to ingest
        #[arg(short, long)]
        input: PathBuf,

        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Overwrite existing data instead of appending
        #[arg(long)]
        overwrite: bool,
    },

    /// Export a table to CSV
    Export {
        /// Source id (e.g. `connector:<uuid>` or `web:<uuid>`)
        #[arg(long)]
        source: String,

        /// Table name to export
        table: String,

        /// Output CSV file path
        #[arg(short, long)]
        output: PathBuf,

        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Delete a table from the store
    Delete {
        /// Source id (e.g. `connector:<uuid>` or `web:<uuid>`)
        #[arg(long)]
        source: String,

        /// Table name to delete
        table: String,

        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Parser, Debug)]
struct AnalyzeArgs {
    /// Input CSV file path
    #[arg(short, long)]
    input: PathBuf,

    /// Z-score threshold for anomaly detection (default: 2.0)
    #[arg(long, default_value = "2.0")]
    z_threshold: f64,

    /// P-value threshold for significance (default: 0.05)
    #[arg(long, default_value = "0.05")]
    p_threshold: f64,

    /// Maximum analysis depth (default: 3)
    #[arg(long, default_value = "3")]
    max_depth: usize,

    /// Pretty-print JSON output
    #[arg(long)]
    pretty: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Configure jemalloc for aggressive memory return before any allocations
    #[cfg(not(target_env = "msvc"))]
    tune_jemalloc();

    // Load .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::RunAll {
        host: None,
        port: None,
        dataset: None,
        store: None,
        tables: None,
        connector_configs: None,
        database_url: None,
    }) {
        Commands::RunAll {
            host,
            port,
            dataset,
            store,
            tables,
            connector_configs,
            database_url,
        } => {
            let log_sender = init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(
                host.as_deref(),
                port,
                dataset,
                store.as_deref(),
                tables,
                connector_configs.as_deref(),
                database_url.as_deref(),
            );

            // Scheduler is now integrated into the API server (started automatically)
            brightflow_api::serve(config, Some(log_sender)).await?;
        },

        Commands::Serve {
            host,
            port,
            dataset,
            store,
            tables,
            connector_configs,
            database_url,
        } => {
            let log_sender = init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(
                host.as_deref(),
                port,
                dataset,
                store.as_deref(),
                tables,
                connector_configs.as_deref(),
                database_url.as_deref(),
            );
            brightflow_api::serve(config, Some(log_sender)).await?;
        },

        Commands::Schedule => {
            init_tracing_simple("brightflow=info");
            println!("Scheduler is now integrated into the API server.");
            println!("Use `brightflow run-all` or `brightflow serve` to start with the scheduler enabled.");
        },

        Commands::Insights(insights_cmd) => match insights_cmd {
            InsightsCommands::Review { args, cadence } => match cadence {
                CadenceArg::All => {
                    for c in ReviewCadence::all() {
                        run_review(&args, *c)?;
                    }
                },
                CadenceArg::Daily => run_review(&args, ReviewCadence::Daily)?,
                CadenceArg::Weekly => run_review(&args, ReviewCadence::Weekly)?,
                CadenceArg::Monthly => run_review(&args, ReviewCadence::Monthly)?,
            },
            InsightsCommands::Trends { args } => run_report(&args, ReportType::Trends)?,
            InsightsCommands::Drivers { args } => run_report(&args, ReportType::Drivers)?,
            InsightsCommands::All { args } => {
                for c in ReviewCadence::all() {
                    run_review(&args, *c)?;
                }
                run_report(&args, ReportType::Trends)?;
                run_report(&args, ReportType::Drivers)?;
            },
        },

        Commands::Connect(connect_cmd) => {
            init_tracing_simple("brightflow=info");
            handle_connect_command(&connect_cmd);
        },

        Commands::Store(store_cmd) => {
            init_tracing_simple("brightflow=info");
            handle_store_command(store_cmd).await?;
        },

        Commands::CreateAdmin {
            email,
            name,
            database_url,
        } => {
            let database_url = database_url
                .unwrap_or_else(|| brightflow_core::WorkspacePaths::from_env().auth_url());
            handle_create_admin(&email, &name, &database_url).await?;
        },

        Commands::Topics { action } => {
            init_tracing_simple("brightflow=info");
            handle_topics(action).await?;
        },

        Commands::MigrateEvents => {
            init_tracing_simple("brightflow=info");
            let paths = brightflow_core::WorkspacePaths::from_env();
            let store = ParquetStore::new(paths.store(), &paths.litehouse_url()).await?;
            let events_path = paths.events_store();
            let count = store.register_existing_events(&events_path).await?;
            println!("Registered {count} event files in the catalog.");
        },

        Commands::Compact {
            source,
            table,
            date,
            all_dates,
        } => {
            init_tracing_simple("brightflow=info");
            let paths = brightflow_core::WorkspacePaths::from_env();
            let store = ParquetStore::new(paths.store(), &paths.litehouse_url()).await?;

            if let Some(date) = date {
                let merged = store
                    .compact_partition(&source, &table, "date", &date)
                    .await?;
                println!("Compacted {merged} files for {source}/{table}/date={date}");
            } else if all_dates {
                // List all distinct date partitions for this table
                let table_row = store
                    .table_info(&source, &table)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

                // Query files and extract partition values
                let files = store
                    .get_table_parquet_paths(&source, &table)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

                // Extract unique dates from file paths (events/{source}/{date}/*.parquet)
                let mut dates: Vec<String> = files
                    .iter()
                    .filter_map(|p| {
                        p.parent()
                            .and_then(|parent| parent.file_name())
                            .and_then(|name| name.to_str())
                            .map(ToString::to_string)
                    })
                    .collect();
                dates.sort();
                dates.dedup();

                let mut total = 0usize;
                for date_val in &dates {
                    let merged = store
                        .compact_partition(&source, &table, "date", date_val)
                        .await?;
                    if merged > 0 {
                        println!("Compacted {merged} files for {source}/{table}/date={date_val}");
                        total += merged;
                    }
                }
                println!(
                    "Done. Compacted {total} files across {} partitions ({} from table '{}')",
                    dates.len(),
                    table_row.name,
                    table
                );
            } else {
                println!("Specify --date <YYYY-MM-DD> or --all-dates");
            }
        },
    }

    Ok(())
}

use std::sync::OnceLock;

/// Holds the `WorkerGuard` for the file appender so logs are flushed on exit.
/// Leaked intentionally — the appender must live for the program's lifetime.
static FILE_APPENDER_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Directory where rotating backend log files are written.
const LOG_DIR: &str = "logs";

/// Filename prefix for the backend log. `tracing-appender` appends the date
/// (e.g. `backend.log.2026-08-02`) and rotates daily.
const LOG_PREFIX: &str = "backend.log";

/// Maximum number of rotated log files to retain on disk.
const MAX_LOG_FILES: usize = 7;

/// No-op `MakeWriter` used as a fallback when the rolling file appender cannot
/// be created, so logging continues to stdout only.
struct DiscardWriter;

impl<'a> MakeWriter<'a> for DiscardWriter {
    type Writer = std::io::Sink;
    fn make_writer(&'a self) -> Self::Writer {
        std::io::sink()
    }
}

/// Builds a daily-rotating non-blocking file writer for `logs/backend.log.*`,
/// keeping the last [`MAX_LOG_FILES`] files. The `WorkerGuard` is stored in a
/// static so it is only dropped (and flushed) at process exit. On failure it
/// falls back to a discard writer (stdout layer still logs) so the server
/// starts regardless.
#[allow(clippy::print_stderr)]
fn make_file_writer() -> BoxMakeWriter {
    if std::fs::create_dir_all(LOG_DIR).is_err() {
        eprintln!("tracing: could not create log dir '{LOG_DIR}'");
        return BoxMakeWriter::new(DiscardWriter);
    }
    let file_appender = match tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(LOG_DIR)
    {
        Ok(appender) => appender,
        Err(err) => {
            eprintln!("tracing: failed to create rolling file appender: {err}");
            return BoxMakeWriter::new(DiscardWriter);
        },
    };

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _guard_result = FILE_APPENDER_GUARD.set(guard);

    BoxMakeWriter::new(non_blocking)
}

fn init_tracing(default_filter: &str) -> broadcast::Sender<LogEntry> {
    let (log_sender, _) = broadcast::channel(1000);
    let log_layer = LogBroadcastLayer::new(log_sender.clone());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(make_file_writer())
                .with_ansi(false),
        )
        .with(log_layer)
        .init();

    log_sender
}

fn init_tracing_simple(default_filter: &str) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(make_file_writer())
                .with_ansi(false),
        )
        .init();
}

fn build_serve_config(
    host: Option<&str>,
    port: Option<u16>,
    dataset: Option<String>,
    store: Option<&str>,
    tables: Option<String>,
    connector_configs: Option<&str>,
    database_url: Option<&str>,
) -> ServeConfig {
    // Set env vars for CLI overrides so WorkspacePaths picks them up
    if let Some(store) = store {
        std::env::set_var("BRIGHTFLOW_STORE", store);
    }
    if let Some(configs) = connector_configs {
        std::env::set_var("BRIGHTFLOW_CONNECTOR_CONFIGS", configs);
    }
    if let Some(url) = database_url {
        std::env::set_var("BRIGHTFLOW_DATABASE_URL", url);
    }

    let mut config = ServeConfig::from_env();

    if let Some(host) = host {
        let host_parts: Vec<u8> = host.split('.').filter_map(|p| p.parse().ok()).collect();
        config.host = host_parts.try_into().unwrap_or([127, 0, 0, 1]);
    }
    if let Some(port) = port {
        config.port = port;
    }
    if dataset.is_some() {
        config.default_dataset = dataset;
    }
    if let Some(tables) = tables {
        config.tables = Some(tables.split(',').map(|t| t.trim().to_string()).collect());
    }

    config
}

fn run_review(args: &AnalyzeArgs, cadence: ReviewCadence) -> Result<()> {
    let df = load_csv(&args.input)?;
    let data_schema = detect_schema(&df)?;

    let suffix = format!("review_{}", cadence.suffix());
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth)
        .with_scoring_ctx(
            ScoringContext::new().with_kpis(data_schema.kpi_columns.iter().cloned().collect()),
        );
    let result =
        engine.run_review_with_cadence(&df, &data_schema, cadence, &DebugLog::disabled())?;
    let tree = result.tree;

    write_outputs(args, &tree, &suffix, cadence.title())?;

    tracing::info!(
        "[{}] {} root findings, {} total nodes",
        suffix,
        tree.roots.len(),
        tree.nodes.len()
    );

    Ok(())
}

fn run_report(args: &AnalyzeArgs, report_type: ReportType) -> Result<()> {
    let df = load_csv(&args.input)?;
    let data_schema = detect_schema(&df)?;

    let suffix = report_type.suffix();
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth)
        .with_scoring_ctx(
            ScoringContext::new().with_kpis(data_schema.kpi_columns.iter().cloned().collect()),
        );
    let result = engine.run_report(&df, &data_schema, report_type, &DebugLog::disabled())?;
    let tree = result.tree;

    write_outputs(args, &tree, suffix, report_type.title())?;

    tracing::info!(
        "[{}] {} root findings, {} total nodes",
        suffix,
        tree.roots.len(),
        tree.nodes.len()
    );

    Ok(())
}

fn write_outputs(
    args: &AnalyzeArgs,
    tree: &brightflow_engine::analysis::tree::AnalysisTree,
    suffix: &str,
    type_title: &str,
) -> Result<()> {
    let input_stem = args
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let input_dir = args.input.parent().unwrap_or(std::path::Path::new("."));

    let json_path = input_dir.join(format!("{input_stem}_{suffix}.json"));
    let md_path = input_dir.join(format!("{input_stem}_{suffix}.md"));
    let html_path = input_dir.join(format!("{input_stem}_{suffix}.html"));

    write_output(&json_path, tree, args.pretty)?;
    write_markdown(&md_path, tree, Some(type_title))?;
    write_html(&html_path, tree, Some(type_title))?;

    Ok(())
}

fn handle_connect_command(cmd: &ConnectCommands) {
    match cmd {
        ConnectCommands::List => {
            let connectors = list_builtin_connectors();
            if connectors.is_empty() {
                println!("No built-in connectors found.");
            } else {
                println!("Available connectors:");
                for name in connectors {
                    println!("  - {name}");
                }
            }
        },
    }
}

async fn handle_store_command(cmd: StoreCommands) -> Result<()> {
    let wp = brightflow_core::WorkspacePaths::from_env();
    let litehouse_url = wp.litehouse_url();

    match cmd {
        StoreCommands::List { path } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let tables = store.list_tables().await?;

            if tables.is_empty() {
                println!("No tables found in store at {}", store_path.display());
            } else {
                println!("Tables in {}:", store_path.display());
                for table in tables {
                    println!("  - {} ({})", table.name, table.source_id);
                }
            }
        },

        StoreCommands::Info { source, name, path } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let info = store.table_info(&source, &name).await?;

            println!("Source: {}", info.source_id);
            println!("Table: {}", info.name);
            println!("Path: {}", info.path);
            println!("Version: {}", info.version);
            println!("Files: {}", info.num_files);
            if let Some(rows) = info.num_rows {
                println!("Rows: {rows}");
            }
            if let Some(created) = info.created_at {
                println!("Created: {created}");
            }
            if let Some(schema) = &info.schema {
                println!("Schema: {}", serde_json::to_string_pretty(schema)?);
            }
        },

        StoreCommands::Ingest {
            source,
            table,
            input,
            path,
            overwrite,
        } => {
            if !input.exists() {
                anyhow::bail!("Input file not found: {}", input.display());
            }

            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            std::fs::create_dir_all(&store_path)?;

            let options = IngestOptions {
                mode: if overwrite {
                    IngestMode::Overwrite
                } else {
                    IngestMode::Append
                },
                ..Default::default()
            };

            tracing::info!("Ingesting {} into {}/{}", input.display(), source, table);
            let info = store
                .ingest_parquet(&source, &table, &input, Some(options))
                .await?;

            println!("Ingested into table '{}/{}'", info.source_id, info.name);
            println!("Version: {}", info.version);
            println!("Files: {}", info.num_files);
        },

        StoreCommands::Export {
            source,
            table,
            output,
            path,
        } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let df = store.read_table(&source, &table).await?;

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Write to CSV
            let mut file = std::fs::File::create(&output)?;
            polars::io::csv::write::CsvWriter::new(&mut file).finish(&mut df.clone())?;

            println!(
                "Exported table '{}/{}' to {}",
                source,
                table,
                output.display()
            );
            println!("Rows: {}", df.height());
        },

        StoreCommands::Delete {
            source,
            table,
            path,
            force,
        } => {
            if !force {
                println!(
                    "Are you sure you want to delete table '{source}/{table}'? This cannot be undone."
                );
                println!("Run with --force to confirm.");
                return Ok(());
            }

            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            store.delete_table(&source, &table).await?;
            println!("Deleted table '{source}/{table}'");
        },
    }

    Ok(())
}

async fn handle_create_admin(email: &str, name: &str, database_url: &str) -> Result<()> {
    // Ensure data directory exists
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let db_path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let db = brightflow_api::auth::AuthDb::new(database_url).await?;

    // Check if user already exists
    if let Some(_existing) = db.get_user_by_email(email).await? {
        anyhow::bail!("User with email '{email}' already exists");
    }

    // Prompt for password
    let password = rpassword::read_password_from_tty(Some("Password: "))?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }
    let confirm = rpassword::read_password_from_tty(Some("Confirm password: "))?;
    if password != confirm {
        anyhow::bail!("Passwords do not match");
    }

    let hash = brightflow_api::auth::hash_password(&password)?;
    let user = db.create_user(email, name, &hash).await?;

    println!("Admin user created:");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Name:  {}", user.display_name);

    Ok(())
}

async fn handle_topics(action: TopicsAction) -> Result<()> {
    match action {
        TopicsAction::Fit {
            source,
            table,
            clusters,
        } => topics_fit(&source, &table, clusters).await,
        TopicsAction::List { source, table } => topics_list(&source, &table),
        TopicsAction::Eval {
            source,
            table,
            algorithms,
            k,
        } => topics_eval(&source, &table, &algorithms, k).await,
        TopicsAction::Embed { source, table } => topics_embed(&source, &table).await,
        TopicsAction::NearDup {
            source,
            table,
            threshold,
        } => topics_near_dup(&source, &table, threshold).await,
        TopicsAction::EvalClassifier { source, table } => {
            topics_eval_classifier(&source, &table).await
        },
    }
}

/// Read-only: does the trained head actually beat nearest-centroid on THIS
/// data? The go/no-go check — if the head doesn't win here, the whole
/// supervised-taxonomy thesis is wrong for this corpus.
///
/// Reads the embeddings already on disk rather than re-embedding, so it is
/// cheap and reflects exactly what a fit would train on.
async fn topics_eval_classifier(source: &str, table: &str) -> Result<()> {
    use brightflow_engine::embedding::get_backend;
    use brightflow_engine::enrichment::read_existing_embeddings;
    use brightflow_engine::nlp::linear::{min_examples_per_label, min_labelled_rows_to_train};
    use brightflow_engine::nlp::{fit_centroid_baseline, fit_multilabel_linear};

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;
    let df = store.read_table(source, table).await?;

    let backend = get_backend(&wp.root(), config.embedder)
        .map_err(|e| anyhow::anyhow!("embedder unavailable: {e}"))?;
    let dim = backend.dim();

    let embeddings = read_existing_embeddings(&df, dim).ok_or_else(|| {
        anyhow::anyhow!("no usable `embedding` column on {source}/{table} — run `topics fit` first")
    })?;

    let targets = brightflow_api::topics::labels::load_label_targets(&store, source, table, &df)
        .await
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no curated labels for {source}/{table} — run the propose_taxonomy and \
                 label_documents agents, then approve their proposals"
            )
        })?;

    // Train on rows having BOTH an embedding and >=1 label — the same rule
    // `fit_topics` step 7b applies.
    let mut features: Vec<Vec<f32>> = Vec::new();
    let mut row_targets: Vec<Vec<usize>> = Vec::new();
    for (i, ids) in targets.per_row.iter().enumerate() {
        if ids.is_empty() {
            continue;
        }
        if let Some(Some(v)) = embeddings.get(i) {
            if v.len() == dim {
                features.push(v.clone());
                row_targets.push(ids.clone());
            }
        }
    }

    println!("Classifier eval: {source}/{table}");
    println!(
        "  {} labelled rows over {} categories (dim {dim})",
        features.len(),
        targets.names.len()
    );
    if features.is_empty() {
        anyhow::bail!("no rows have both an embedding and a label");
    }

    let Some(outcome) = fit_multilabel_linear(&features, &row_targets, targets.names.len(), dim)
    else {
        println!(
            "\n  Not enough signal to train (need >= {} labelled rows and >= {} examples \
             for at least one category).",
            min_labelled_rows_to_train(),
            min_examples_per_label()
        );
        return Ok(());
    };

    let base = fit_centroid_baseline(&features, &row_targets, &outcome.retained_labels, dim);

    println!(
        "  train {} rows / {} retained categories\n",
        outcome.train_rows,
        outcome.retained_labels.len()
    );
    println!("  {:<28} {:>10}", "model", "macro-F1");
    println!("  {:-<28} {:->10}", "", "");
    match base {
        Some(b) => println!("  {:<28} {b:>10.3}", "nearest-centroid baseline"),
        None => println!("  {:<28} {:>10}", "nearest-centroid baseline", "n/a"),
    }
    println!("  {:<28} {:>10.3}", "trained head", outcome.val_macro_f1);

    if let Some(b) = base {
        let delta = outcome.val_macro_f1 - b;
        println!("\n  head - baseline = {delta:+.3}");
        if delta <= 0.0 {
            println!(
                "  ⚠ The head does NOT beat the baseline on this data. Either the seed \
                 labels are too noisy//thin, or intent is not linearly recoverable from \
                 these embeddings. Investigate before trusting predicted_labels."
            );
        }
    }

    // Per-category support: the labels that got dropped are the ones a curator
    // should spend the next hour on.
    println!("\n  per-category support (retained by the head):");
    for (i, &label) in outcome.retained_labels.iter().enumerate() {
        let name = targets.names.get(label).map_or("?", String::as_str);
        let n = outcome.support.get(i).copied().unwrap_or(0);
        println!("    {n:>6}  {name}");
    }
    let dropped: Vec<&String> = targets
        .names
        .iter()
        .enumerate()
        .filter(|(i, _)| !outcome.retained_labels.contains(i))
        .map(|(_, n)| n)
        .collect();
    if !dropped.is_empty() {
        println!(
            "\n  dropped (need ~{} labelled examples each — label more of these):",
            min_examples_per_label()
        );
        for name in dropped {
            println!("    {name}");
        }
    }
    Ok(())
}

/// Read-only near-duplicate report. Embeds the table's rows, groups
/// near-identical text, and prints the groups. Writes nothing.
async fn topics_near_dup(source: &str, table: &str, threshold: Option<f32>) -> Result<()> {
    use brightflow_engine::embedding::get_backend;
    use brightflow_engine::nlp::{
        clean_for_embedding, find_near_duplicates, DEFAULT_NEAR_DUP_THRESHOLD,
    };

    /// Characters of each member's text shown in the report.
    const SNIPPET_LEN: usize = 100;

    let threshold = threshold.unwrap_or(DEFAULT_NEAR_DUP_THRESHOLD);
    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;
    let df = store.read_table(source, table).await?;
    println!(
        "Near-dup scan on {source}/{table}: {} rows, threshold {threshold}",
        df.height()
    );

    // Clean the configured text columns into one string per row.
    let mut texts: Vec<Option<String>> = Vec::with_capacity(df.height());
    {
        let mut per_col: Vec<Vec<String>> = Vec::new();
        for col in &config.text_columns {
            let values = df
                .column(col)
                .map_err(|e| anyhow::anyhow!("column {col}: {e}"))?
                .as_materialized_series()
                .str()
                .map_err(|e| anyhow::anyhow!("column {col} not string: {e}"))?
                .into_iter()
                .map(|o| o.unwrap_or("").to_string())
                .collect::<Vec<_>>();
            per_col.push(values);
        }
        for row in 0..df.height() {
            let raw = per_col
                .iter()
                .filter_map(|c| c.get(row).map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            texts.push(clean_for_embedding(&raw, config.cleaning_profile));
        }
    }

    // Embed only the eligible rows, then scatter back so indices line up with
    // `texts` (and therefore with the DataFrame's rows).
    let eligible: Vec<usize> = texts
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_ref().map(|_| i))
        .collect();
    let clean_texts: Vec<String> = eligible
        .iter()
        .filter_map(|&i| texts.get(i).cloned().flatten())
        .collect();
    println!("  {} rows eligible after cleaning", eligible.len());

    let backend = get_backend(&wp.root(), config.embedder)
        .map_err(|e| anyhow::anyhow!("embedder unavailable: {e}"))?;
    let vectors = backend
        .embed(&clean_texts)
        .map_err(|e| anyhow::anyhow!("embed failed: {e}"))?;

    let mut embeddings: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    for (slot, vector) in eligible.iter().zip(vectors) {
        if let Some(cell) = embeddings.get_mut(*slot) {
            *cell = Some(vector);
        }
    }

    let groups = find_near_duplicates(&embeddings, &texts, threshold)
        .map_err(|e| anyhow::anyhow!("near-dup detection failed: {e}"))?;

    if groups.is_empty() {
        println!("\nNo near-duplicate groups found.");
        return Ok(());
    }

    let duplicated: usize = groups.iter().map(|g| g.rows.len()).sum();
    println!("\n{} group(s) covering {duplicated} rows:", groups.len());
    for (n, group) in groups.iter().enumerate() {
        let kind = if group.exact { "exact" } else { "near" };
        println!("\n  Group {} — {} rows ({kind})", n + 1, group.rows.len());
        for &row in &group.rows {
            let snippet = texts
                .get(row)
                .cloned()
                .flatten()
                .unwrap_or_default()
                .chars()
                .take(SNIPPET_LEN)
                .collect::<String>()
                .replace('\n', " ");
            println!("    row {row:>6}: {snippet}");
        }
    }

    Ok(())
}

async fn resolve_enrichment_config(
    store: &ParquetStore,
    source: &str,
    table: &str,
) -> Result<brightflow_engine::enrichment::EnrichmentConfig> {
    use brightflow_engine::enrichment::{EnrichmentConfig, EnrichmentOverrides};
    let overrides = match store.db().get_table(source, table).await {
        Ok(Some(row)) => store
            .db()
            .get_enrichment_settings(&row.id)
            .await
            .ok()
            .flatten()
            .map(|r| {
                EnrichmentOverrides::from_stored(
                    r.text_columns.as_deref(),
                    r.cleaning_profile,
                    r.language_column,
                    r.embedder,
                    r.min_cluster_size,
                    r.algorithm,
                )
            }),
        _ => None,
    };
    EnrichmentConfig::resolve(table, overrides.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Table '{table}' is not enrichable"))
}

async fn topics_fit(source: &str, table: &str, num_clusters: Option<usize>) -> Result<()> {
    use brightflow_engine::enrichment::{fit_topics, FitOptions, DEFAULT_K};
    let num_clusters = num_clusters.unwrap_or(DEFAULT_K);

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;

    println!(
        "Fitting topics: {source}/{table} (k={num_clusters}, embedder={})",
        config.embedder.name()
    );

    let df = store.read_table(source, table).await?;
    println!("  Loaded {} rows", df.height());

    // Curated row labels train the classifier head. Absent => fall back to the
    // table's own `label_names` column, as before taxonomies existed.
    let labels =
        brightflow_api::topics::labels::load_label_targets(&store, source, table, &df).await;
    match labels.as_ref() {
        Some(t) => println!(
            "  {} curated categories over {} labelled rows",
            t.names.len(),
            t.labelled_rows()
        ),
        None => println!("  No curated labels — falling back to label_names if present"),
    }

    let workspace_root = wp.root();
    let source_owned = source.to_string();
    let table_owned = table.to_string();
    let (enriched, outcome) = tokio::task::spawn_blocking(move || {
        fit_topics(
            &workspace_root,
            &source_owned,
            &table_owned,
            &df,
            &config,
            &FitOptions {
                num_clusters,
                language: None,
                algorithm: None,
                labels,
            },
        )
    })
    .await
    .map_err(|e| anyhow::anyhow!("topics fit join error: {e}"))?
    .map_err(|e| anyhow::anyhow!("topics fit failed: {e}"))?;

    println!(
        "  k = {} ({} of {} rows eligible, labels={}, language={})",
        outcome.k,
        outcome.eligible_rows,
        outcome.total_rows,
        outcome.has_labels,
        outcome.language.as_deref().unwrap_or("-")
    );
    match outcome.classifier_val_macro_f1 {
        Some(f1) => println!(
            "  classifier: {} labels, val macro-F1 = {:.3} ({})",
            outcome.classifier_labels.len(),
            f1,
            outcome.classifier_labels.join(", ")
        ),
        None => println!("  classifier: not trained (too little labelled signal)"),
    }
    for (i, name) in outcome.cluster_names.iter().enumerate() {
        let size = outcome.cluster_sizes.get(i).copied().unwrap_or(0);
        println!("    cluster {i}: {size:>6} docs · {name}");
    }

    let table_dir = wp.store().join(source).join(table);
    std::fs::create_dir_all(&table_dir)?;
    let output_path = table_dir.join("enriched.parquet");
    let file = std::fs::File::create(&output_path)?;
    polars::prelude::ParquetWriter::new(file).finish(&mut enriched.clone())?;

    let info = store
        .ingest_parquet(
            source,
            table,
            &output_path,
            Some(IngestOptions {
                mode: IngestMode::Overwrite,
                ..Default::default()
            }),
        )
        .await?;

    println!(
        "  Written: {} rows, {} columns",
        info.num_rows.unwrap_or(0),
        enriched.width()
    );

    if output_path.exists() {
        drop(std::fs::remove_file(&output_path));
    }

    println!("Done.");
    Ok(())
}

/// A/B eval: clustering algorithms × k on the table's real rows.
/// This is the decisive quality check for clustering choices.
#[allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
async fn topics_eval(source: &str, table: &str, algorithms: &str, k: usize) -> Result<()> {
    use std::collections::HashMap;

    use brightflow_engine::embedding::get_backend;
    use brightflow_engine::nlp::cluster_metrics::{davies_bouldin, npmi_coherence, silhouette};
    use brightflow_engine::nlp::{
        clean_for_embedding, default_min_cluster_size, hdbscan_dense, kmeans_dense,
    };

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;
    let df = store.read_table(source, table).await?;
    println!("Eval on {source}/{table}: {} rows", df.height());

    // Clean once (profile is embedder-independent)
    let mut texts: Vec<Option<String>> = Vec::with_capacity(df.height());
    {
        let columns: Vec<&str> = config.text_columns.iter().map(String::as_str).collect();
        let mut per_col: Vec<Vec<String>> = Vec::new();
        for col in &columns {
            let values = df
                .column(col)
                .map_err(|e| anyhow::anyhow!("column {col}: {e}"))?
                .as_materialized_series()
                .str()
                .map_err(|e| anyhow::anyhow!("column {col} not string: {e}"))?
                .into_iter()
                .map(|o| o.unwrap_or("").to_string())
                .collect::<Vec<_>>();
            per_col.push(values);
        }
        for row in 0..df.height() {
            let raw = per_col
                .iter()
                .filter_map(|c| c.get(row).map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            texts.push(clean_for_embedding(&raw, config.cleaning_profile));
        }
    }
    let eligible: Vec<usize> = texts
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_ref().map(|_| i))
        .collect();
    let clean_texts: Vec<String> = eligible.iter().filter_map(|&i| texts[i].clone()).collect();
    println!("  {} rows eligible after cleaning", eligible.len());

    println!(
        "\n{:<38} {:<9} {:>4} {:>10} {:>8} {:>7} {:>9} {:>9}",
        "embedder", "algo", "k", "silhouette", "dav-bou", "npmi", "unassign%", "wall"
    );

    let embedder_name = config.embedder.name();
    let backend = get_backend(&wp.root(), config.embedder)
        .map_err(|e| anyhow::anyhow!("embedder unavailable: {e}"))?;
    let t_embed = std::time::Instant::now();
    let vectors = backend
        .embed(&clean_texts)
        .map_err(|e| anyhow::anyhow!("embed failed: {e}"))?;
    let embed_time = t_embed.elapsed();

    for algo in algorithms.split(',').map(str::trim) {
        let t_cluster = std::time::Instant::now();
        let result = match algo {
            "hdbscan" => hdbscan_dense(&vectors, default_min_cluster_size(vectors.len())),
            _ => kmeans_dense(&vectors, k, 30),
        };
        let wall = t_cluster.elapsed() + embed_time;
        let unassigned = result.assignments.iter().filter(|a| a.is_none()).count() as f64
            / result.assignments.len().max(1) as f64
            * 100.0;

        let sil = silhouette(&vectors, &result.assignments);
        let db = davies_bouldin(&vectors, &result.assignments);

        // Top terms per cluster by in-cluster document frequency (quick,
        // eval-only naming — the real pipeline uses c-TF-IDF)
        let n_clusters = result.centroids.len();
        let mut term_df: Vec<HashMap<String, usize>> = vec![HashMap::new(); n_clusters];
        for (text, assigned) in clean_texts.iter().zip(result.assignments.iter()) {
            let Some(c) = assigned else { continue };
            let mut seen = std::collections::HashSet::new();
            for token in text.to_lowercase().split_whitespace() {
                if token.len() > 3 && seen.insert(token.to_string()) {
                    *term_df[*c].entry(token.to_string()).or_insert(0) += 1;
                }
            }
        }
        let cluster_terms: Vec<Vec<String>> = term_df
            .iter()
            .map(|counts| {
                let mut ranked: Vec<(&String, &usize)> = counts.iter().collect();
                ranked.sort_by(|a, b| b.1.cmp(a.1));
                ranked.into_iter().take(6).map(|(t, _)| t.clone()).collect()
            })
            .collect();
        let npmi = npmi_coherence(&clean_texts, &cluster_terms);

        println!(
            "{:<38} {:<9} {:>4} {:>10} {:>8} {:>7} {:>8.1}% {:>8.1}s",
            embedder_name,
            algo,
            n_clusters,
            sil.map_or("-".to_string(), |v| format!("{v:.3}")),
            db.map_or("-".to_string(), |v| format!("{v:.3}")),
            npmi.map_or("-".to_string(), |v| format!("{v:.3}")),
            unassigned,
            wall.as_secs_f64(),
        );
    }
    Ok(())
}

fn topics_list(source: &str, table: &str) -> Result<()> {
    use brightflow_engine::embedding::topics_artifact_dir;
    use brightflow_engine::enrichment::{ArtifactMeta, ClusteringArtifact};

    let wp = brightflow_core::WorkspacePaths::from_env();
    let dir = topics_artifact_dir(&wp.root(), source, table);

    if !ArtifactMeta::exists(&dir) {
        println!("No topics artifacts found at {}", dir.display());
        println!("Run: brightflow topics fit --source {source} --table {table}");
        return Ok(());
    }

    let meta = ArtifactMeta::load(&dir).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("Topics for {source}/{table}");
    println!("  embedding model : {}", meta.embedding_model_id);
    println!("  k               : {}", meta.k);
    println!("  rows            : {}", meta.total_rows);
    println!("  labels present  : {}", meta.has_labels);
    println!("  fitted_at       : {}", meta.fitted_at);

    if let Ok(c) = ClusteringArtifact::load(&dir) {
        println!();
        for i in 0..c.k {
            let terms = c.top_terms.get(i).cloned().unwrap_or_default();
            let samples = c.sample_titles.get(i).cloned().unwrap_or_default();
            println!("Cluster {i}");
            if !terms.is_empty() {
                println!("  distinctive terms : {}", terms.join(", "));
            }
            if !samples.is_empty() {
                println!("  representative titles:");
                for s in &samples {
                    println!("    · {s}");
                }
            }
            if terms.is_empty() && samples.is_empty() {
                if let Some(n) = c.names.get(i) {
                    println!("  {n}");
                }
            }
            println!();
        }
    }

    Ok(())
}

async fn topics_embed(source: &str, table: &str) -> Result<()> {
    use brightflow_engine::enrichment::enrich_with_topics;

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;

    let df = store.read_table(source, table).await?;
    println!("Embedding {} rows from {source}/{table}", df.height());

    let workspace_root = wp.root();
    let source_owned = source.to_string();
    let table_owned = table.to_string();
    let enriched = tokio::task::spawn_blocking(move || {
        enrich_with_topics(&workspace_root, &source_owned, &table_owned, &df, &config)
    })
    .await
    .map_err(|e| anyhow::anyhow!("embed join error: {e}"))?
    .map_err(|e| anyhow::anyhow!("embed failed: {e}"))?;

    let table_dir = wp.store().join(source).join(table);
    std::fs::create_dir_all(&table_dir)?;
    let output_path = table_dir.join("enriched.parquet");
    let file = std::fs::File::create(&output_path)?;
    polars::prelude::ParquetWriter::new(file).finish(&mut enriched.clone())?;

    store
        .ingest_parquet(
            source,
            table,
            &output_path,
            Some(IngestOptions {
                mode: IngestMode::Overwrite,
                ..Default::default()
            }),
        )
        .await?;

    if output_path.exists() {
        drop(std::fs::remove_file(&output_path));
    }

    println!("Done.");
    Ok(())
}
