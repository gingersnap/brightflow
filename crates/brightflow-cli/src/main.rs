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
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use brightflow_api::system::log_layer::{LogBroadcastLayer, LogEntry};
use brightflow_api::ServeConfig;
use brightflow_connect::list_builtin_connectors;
use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::{ReportType, ReviewCadence};
use brightflow_insights::data::loader::load_csv;
use brightflow_insights::data::schema::detect_schema;
use brightflow_insights::debug::DebugLog;
use brightflow_insights::output::html::write_html;
use brightflow_insights::output::json::write_output;
use brightflow_insights::output::markdown::write_markdown;
// Scheduler is now integrated into the API server
use brightflow_store::{IngestMode, IngestOptions, ParquetStore};

#[derive(Parser, Debug)]
#[command(name = "brightflow")]
#[command(about = "Brightflow analytics platform")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

    /// Register existing event Parquet files in the Litehouse catalog
    MigrateEvents,

    /// Compact event files in a partition into a single file
    Compact {
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
enum StoreCommands {
    /// List all tables in the store
    List {
        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Show information about a table
    Info {
        /// Table name
        name: String,

        /// Store path (derived from BRIGHTFLOW_DATA_DIR)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    /// Ingest a Parquet file into a table
    Ingest {
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

    match cli.command {
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

        Commands::MigrateEvents => {
            init_tracing_simple("brightflow=info");
            let paths = brightflow_core::WorkspacePaths::from_env();
            let store = ParquetStore::new(paths.store(), &paths.litehouse_url()).await?;
            let events_path = paths.events_store();
            let count = store.register_existing_events(&events_path).await?;
            println!("Registered {count} event files in the catalog.");
        },

        Commands::Compact {
            table,
            date,
            all_dates,
        } => {
            init_tracing_simple("brightflow=info");
            let paths = brightflow_core::WorkspacePaths::from_env();
            let store = ParquetStore::new(paths.store(), &paths.litehouse_url()).await?;

            if let Some(date) = date {
                let merged = store.compact_partition(&table, "date", &date).await?;
                println!("Compacted {merged} files for {table}/date={date}");
            } else if all_dates {
                // List all distinct date partitions for this table
                let table_row = store
                    .table_info(&table)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

                // Query files and extract partition values
                let files = store
                    .get_table_parquet_paths(&table)
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
                    let merged = store.compact_partition(&table, "date", date_val).await?;
                    if merged > 0 {
                        println!("Compacted {merged} files for {table}/date={date_val}");
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

fn init_tracing(default_filter: &str) -> broadcast::Sender<LogEntry> {
    let (log_sender, _) = broadcast::channel(1000);
    let log_layer = LogBroadcastLayer::new(log_sender.clone());

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
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

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
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

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
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
    tree: &brightflow_insights::analysis::tree::AnalysisTree,
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
                    println!("  - {}", table.name);
                }
            }
        },

        StoreCommands::Info { name, path } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let info = store.table_info(&name).await?;

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

            tracing::info!("Ingesting {} into table '{}'", input.display(), table);
            let info = store.ingest_parquet(&table, &input, Some(options)).await?;

            println!("Ingested into table '{}'", info.name);
            println!("Version: {}", info.version);
            println!("Files: {}", info.num_files);
        },

        StoreCommands::Export {
            table,
            output,
            path,
        } => {
            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            let df = store.read_table(&table).await?;

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Write to CSV
            let mut file = std::fs::File::create(&output)?;
            polars::io::csv::write::CsvWriter::new(&mut file).finish(&mut df.clone())?;

            println!("Exported table '{}' to {}", table, output.display());
            println!("Rows: {}", df.height());
        },

        StoreCommands::Delete { table, path, force } => {
            if !force {
                println!("Are you sure you want to delete table '{table}'? This cannot be undone.");
                println!("Run with --force to confirm.");
                return Ok(());
            }

            let store_path = path.unwrap_or_else(|| wp.store());
            let store = ParquetStore::new(&store_path, &litehouse_url).await?;
            store.delete_table(&table).await?;
            println!("Deleted table '{table}'");
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
    let user = db.create_user(email, name, &hash, true).await?;

    println!("Admin user created:");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Name:  {}", user.display_name);

    Ok(())
}
