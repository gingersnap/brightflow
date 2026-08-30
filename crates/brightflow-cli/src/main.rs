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
use std::path::PathBuf;

use brightflow_connect::list_builtin_connectors;
use brightflow_engine::analysis::tree::{ReportType, ReviewCadence};
use brightflow_store::ParquetStore;

mod commands;
mod logging;

use commands::admin::handle_create_admin;
use commands::insights::{run_report, run_review};
use commands::serve::build_serve_config;
use commands::store::handle_store_command;
use commands::topics::handle_topics;
use logging::{init_tracing, init_tracing_simple};

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

    /// Ticket enrichment (Call A classification / Call B extraction)
    Enrich {
        #[command(subcommand)]
        action: EnrichAction,
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

/// `enrich` subcommands.
#[derive(Subcommand, Debug)]
pub enum EnrichAction {
    /// Score Call A or Call B against testdata/eval/<lang>/issues.csv using
    /// the table's current vocabulary and the BRIGHTFLOW_LLM_* provider.
    /// Costs money; never touches the cache.
    Eval {
        /// Source id owning the table whose vocabulary is used
        #[arg(long)]
        source: String,
        /// Table name (e.g. issues)
        #[arg(long, default_value = "issues")]
        table: String,
        /// Language folder under testdata/eval (en | sv | fi | ...)
        #[arg(long)]
        lang: String,
        /// Which call: a (classify) | b (extract)
        #[arg(long, default_value = "a")]
        call: String,
        /// Score only the first N rows
        #[arg(long)]
        limit: Option<usize>,
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

    // Every subcommand opens a workspace database, and SQLite will not create
    // one under a directory that does not exist — a fresh `BRIGHTFLOW_DATA_DIR`
    // otherwise fails with a bare "unable to open database file". `serve` has
    // always done this inside the API; doing it here covers the other
    // subcommands (`store`, `topics`, `create-admin`, …) too.
    brightflow_core::WorkspacePaths::from_env().ensure_dirs()?;

    match cli.command.unwrap_or(Commands::RunAll {
        host: None,
        port: None,
        dataset: None,
        store: None,
        tables: None,
        connector_configs: None,
        database_url: None,
    }) {
        // `run-all` and `serve` start the same server; one body, two names.
        Commands::RunAll {
            host,
            port,
            dataset,
            store,
            tables,
            connector_configs,
            database_url,
        }
        | Commands::Serve {
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

        Commands::Enrich { action } => {
            init_tracing_simple("brightflow=info");
            match action {
                EnrichAction::Eval {
                    source,
                    table,
                    lang,
                    call,
                    limit,
                } => {
                    let call = commands::enrich::EvalCall::parse(&call)?;
                    commands::enrich::run_eval(&source, &table, &lang, call, limit).await?;
                },
            }
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
                // Partition values come from the store catalog, which is
                // authoritative — never from parsing the on-disk layout here.
                let dates = store
                    .list_partition_values(&source, &table, "date")
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

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
                    "Done. Compacted {total} files across {} date partitions of {source}/{table}",
                    dates.len(),
                );
            } else {
                println!("Specify --date <YYYY-MM-DD> or --all-dates");
            }
        },
    }

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
