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
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use brightflow_api::ServeConfig;
use brightflow_connect::{
    get_builtin_connector_path, list_builtin_connectors, run_connector, RunOptions,
};
use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::{ReportType, ReviewCadence};
use brightflow_insights::data::config::SchemaConfig;
use brightflow_insights::data::loader::load_csv;
use brightflow_insights::data::schema::{detect_schema, DataSchema};
use brightflow_insights::debug::DebugLog;
use brightflow_insights::output::html::write_html;
use brightflow_insights::output::json::write_output;
use brightflow_insights::output::markdown::write_markdown;
// Scheduler is now integrated into the API server
use brightflow_store::{DeltaStore, IngestMode, IngestOptions};

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
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Default dataset to load on startup
        #[arg(long)]
        dataset: Option<String>,

        /// Path to Delta Lake store to auto-load tables from
        #[arg(long)]
        delta_store: Option<String>,

        /// Specific Delta tables to load (comma-separated, loads all if not specified)
        #[arg(long)]
        delta_tables: Option<String>,

        /// Path to connector config YAML directory
        #[arg(long, default_value = "./configs")]
        connector_configs: String,

        /// SQLite database URL for auth/sessions
        #[arg(long)]
        database_url: Option<String>,
    },

    /// Start the API server only
    Serve {
        /// Host address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Default dataset to load on startup
        #[arg(long)]
        dataset: Option<String>,

        /// Path to Delta Lake store to auto-load tables from
        #[arg(long)]
        delta_store: Option<String>,

        /// Specific Delta tables to load (comma-separated, loads all if not specified)
        #[arg(long)]
        delta_tables: Option<String>,

        /// Path to connector config YAML directory
        #[arg(long, default_value = "./configs")]
        connector_configs: String,

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

    /// Delta Lake store operations
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

        /// SQLite database URL
        #[arg(long, default_value = "sqlite:data/brightflow.db?mode=rwc")]
        database_url: String,
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
    /// Run a data connector to sync data from an API
    Run {
        /// Connector name (e.g., "github") or path to Lua file
        connector: String,

        /// Path to config YAML file
        #[arg(short, long)]
        config: PathBuf,

        /// Only sync specific endpoints (comma-separated)
        #[arg(long)]
        only: Option<String>,

        /// Dry run - show what would be synced without fetching
        #[arg(long)]
        dry_run: bool,

        /// Ingest output into Delta Lake store
        #[arg(long)]
        ingest: bool,

        /// Store path for Delta Lake ingestion (default: ./data/store)
        #[arg(long, default_value = "./data/store")]
        store_path: PathBuf,
    },

    /// List available built-in connectors
    List,
}

#[derive(Subcommand, Debug)]
enum StoreCommands {
    /// List all tables in the store
    List {
        /// Store path (default: ./data/store)
        #[arg(short, long, default_value = "./data/store")]
        path: PathBuf,
    },

    /// Show information about a table
    Info {
        /// Table name
        name: String,

        /// Store path (default: ./data/store)
        #[arg(short, long, default_value = "./data/store")]
        path: PathBuf,
    },

    /// Ingest a Parquet file into a Delta table
    Ingest {
        /// Table name to create/append to
        table: String,

        /// Path to Parquet file(s) to ingest
        #[arg(short, long)]
        input: PathBuf,

        /// Store path (default: ./data/store)
        #[arg(short, long, default_value = "./data/store")]
        path: PathBuf,

        /// Overwrite existing data instead of appending
        #[arg(long)]
        overwrite: bool,
    },

    /// Export a Delta table to CSV
    Export {
        /// Table name to export
        table: String,

        /// Output CSV file path
        #[arg(short, long)]
        output: PathBuf,

        /// Store path (default: ./data/store)
        #[arg(short, long, default_value = "./data/store")]
        path: PathBuf,
    },

    /// Delete a table from the store
    Delete {
        /// Table name to delete
        table: String,

        /// Store path (default: ./data/store)
        #[arg(short, long, default_value = "./data/store")]
        path: PathBuf,

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

    /// Schema config file (YAML) defining column roles
    #[arg(short, long)]
    schema: Option<PathBuf>,

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
            delta_store,
            delta_tables,
            connector_configs,
            database_url,
        } => {
            init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(
                &host,
                port,
                dataset,
                delta_store,
                delta_tables,
                &connector_configs,
                database_url,
            );

            // Scheduler is now integrated into the API server (started automatically)
            brightflow_api::serve(config).await?;
        },

        Commands::Serve {
            host,
            port,
            dataset,
            delta_store,
            delta_tables,
            connector_configs,
            database_url,
        } => {
            init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(
                &host,
                port,
                dataset,
                delta_store,
                delta_tables,
                &connector_configs,
                database_url,
            );
            brightflow_api::serve(config).await?;
        },

        Commands::Schedule => {
            init_tracing("brightflow=info");
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
            init_tracing("brightflow=info");
            handle_connect_command(connect_cmd).await?;
        },

        Commands::Store(store_cmd) => {
            init_tracing("brightflow=info");
            handle_store_command(store_cmd).await?;
        },

        Commands::CreateAdmin {
            email,
            name,
            database_url,
        } => {
            handle_create_admin(&email, &name, &database_url).await?;
        },
    }

    Ok(())
}

fn init_tracing(default_filter: &str) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

fn build_serve_config(
    host: &str,
    port: u16,
    dataset: Option<String>,
    delta_store: Option<String>,
    delta_tables: Option<String>,
    connector_configs: &str,
    database_url: Option<String>,
) -> ServeConfig {
    let host_parts: Vec<u8> = host.split('.').filter_map(|p| p.parse().ok()).collect();
    let host_array: [u8; 4] = host_parts.try_into().unwrap_or([127, 0, 0, 1]);

    let delta_store_path = delta_store.or_else(|| std::env::var("BRIGHTFLOW_DELTA_STORE").ok());

    let resolved_tables = delta_tables
        .or_else(|| std::env::var("BRIGHTFLOW_DELTA_TABLES").ok())
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());

    let schema_dir = std::env::var("BRIGHTFLOW_SCHEMA_DIR").ok();

    let connector_config_dir = std::env::var("BRIGHTFLOW_CONNECTOR_CONFIGS")
        .ok()
        .unwrap_or_else(|| connector_configs.to_string());

    let database_url = database_url.or_else(|| std::env::var("BRIGHTFLOW_DATABASE_URL").ok());

    ServeConfig {
        host: host_array,
        port,
        default_dataset: dataset.or_else(|| std::env::var("BRIGHTFLOW_DEFAULT_DATASET").ok()),
        delta_store_path,
        delta_tables: resolved_tables,
        schema_dir,
        connector_config_dir: Some(connector_config_dir),
        database_url,
    }
}

fn run_review(args: &AnalyzeArgs, cadence: ReviewCadence) -> Result<()> {
    let df = load_csv(&args.input)?;

    let (data_schema, schema_name) = if let Some(schema_path) = &args.schema {
        let config = SchemaConfig::load(schema_path)?;
        let name = config.name.clone();
        (DataSchema::from_config(&config), name)
    } else {
        (detect_schema(&df)?, None)
    };

    let suffix = format!("review_{}", cadence.suffix());
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
    let tree = engine.run_review_with_cadence(&df, &data_schema, cadence, &DebugLog::disabled())?;

    write_outputs(args, &tree, &suffix, &schema_name, cadence.title())?;

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

    let (data_schema, schema_name) = if let Some(schema_path) = &args.schema {
        let config = SchemaConfig::load(schema_path)?;
        let name = config.name.clone();
        (DataSchema::from_config(&config), name)
    } else {
        (detect_schema(&df)?, None)
    };

    let suffix = report_type.suffix();
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
    let tree = engine.run_report(&df, &data_schema, report_type, &DebugLog::disabled())?;

    write_outputs(args, &tree, suffix, &schema_name, report_type.title())?;

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
    schema_name: &Option<String>,
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

    let title = match schema_name {
        Some(name) => format!("{name} - {type_title}"),
        None => type_title.to_string(),
    };

    write_output(&json_path, tree, args.pretty)?;
    write_markdown(&md_path, tree, Some(&title))?;
    write_html(&html_path, tree, Some(&title))?;

    Ok(())
}

async fn handle_connect_command(cmd: ConnectCommands) -> Result<()> {
    match cmd {
        ConnectCommands::Run {
            connector,
            config,
            only,
            dry_run,
            ingest,
            store_path,
        } => {
            // Resolve connector path - check if it's a built-in name or a file path
            let connector_path = if connector.ends_with(".lua") {
                PathBuf::from(&connector)
            } else {
                get_builtin_connector_path(&connector).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Connector '{connector}' not found. Use 'brightflow connect list' to see available connectors."
                    )
                })?
            };

            if !connector_path.exists() {
                anyhow::bail!("Connector file not found: {}", connector_path.display());
            }

            if !config.exists() {
                anyhow::bail!("Config file not found: {}", config.display());
            }

            tracing::info!("Running connector: {}", connector);
            tracing::info!("Config: {}", config.display());

            let options = RunOptions {
                only,
                dry_run,
                cursor_values: std::collections::HashMap::new(),
            };
            let result = run_connector(&connector_path, &config, &options).await?;

            if result.dry_run {
                tracing::info!("Dry run completed. Endpoints that would be synced:");
                for endpoint in &result.endpoints_synced {
                    tracing::info!("  - {}", endpoint);
                }
            } else {
                tracing::info!("Sync completed successfully!");
                tracing::info!("Endpoints synced: {:?}", result.endpoints_synced);
                tracing::info!("Output path: {}", result.output_path);

                // Ingest into Delta Lake if requested
                if ingest {
                    tracing::info!("Ingesting output into Delta Lake store...");
                    let store = DeltaStore::new(&store_path);

                    // Create store directory if it doesn't exist
                    std::fs::create_dir_all(&store_path)?;

                    // Find and ingest all parquet files from output
                    let output_dir = PathBuf::from(&result.output_path);
                    if output_dir.exists() && output_dir.is_dir() {
                        for dir_entry in std::fs::read_dir(&output_dir)? {
                            let path = dir_entry?.path();
                            if path.extension().is_some_and(|ext| ext == "parquet") {
                                let table_name = path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown");

                                tracing::info!(
                                    "Ingesting {} into table '{}'",
                                    path.display(),
                                    table_name
                                );
                                let info = store
                                    .ingest_parquet(
                                        table_name,
                                        &path,
                                        Some(IngestOptions::default()),
                                    )
                                    .await?;
                                tracing::info!(
                                    "  Table '{}' now at version {}, {} files",
                                    info.name,
                                    info.version,
                                    info.num_files
                                );
                            }
                        }
                    }
                }
            }
        },

        ConnectCommands::List => {
            let connectors = list_builtin_connectors()?;
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

    Ok(())
}

async fn handle_store_command(cmd: StoreCommands) -> Result<()> {
    match cmd {
        StoreCommands::List { path } => {
            let store = DeltaStore::new(&path);
            let tables = store.list_tables().await?;

            if tables.is_empty() {
                println!("No tables found in store at {}", path.display());
            } else {
                println!("Tables in {}:", path.display());
                for table in tables {
                    println!("  - {}", table.name);
                }
            }
        },

        StoreCommands::Info { name, path } => {
            let store = DeltaStore::new(&path);
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

            let store = DeltaStore::new(&path);
            std::fs::create_dir_all(&path)?;

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
            let store = DeltaStore::new(&path);
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

            let store = DeltaStore::new(&path);
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

    let db = brightflow_auth::AuthDb::new(database_url).await?;

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

    let hash = brightflow_auth::hash_password(&password)?;
    let user = db.create_user(email, name, &hash, true).await?;

    println!("Admin user created:");
    println!("  ID:    {}", user.id);
    println!("  Email: {}", user.email);
    println!("  Name:  {}", user.display_name);

    Ok(())
}
