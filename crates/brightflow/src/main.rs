use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use brightflow_api::ServeConfig;
use brightflow_insights::analysis::engine::AnalysisEngine;
use brightflow_insights::analysis::tree::{ReportType, ReviewCadence};
use brightflow_insights::data::config::SchemaConfig;
use brightflow_insights::data::loader::load_csv;
use brightflow_insights::data::schema::{detect_schema, DataSchema};
use brightflow_insights::debug::DebugLog;
use brightflow_insights::output::html::write_html;
use brightflow_insights::output::json::write_output;
use brightflow_insights::output::markdown::write_markdown;

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
    },

    /// Start the scheduler daemon only (not yet implemented)
    Schedule,

    /// Run statistical analysis on data
    #[command(subcommand)]
    Insights(InsightsCommands),
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
    // Load .env file if present
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::RunAll {
            host,
            port,
            dataset,
        } => {
            init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(&host, port, dataset);

            // Run API and scheduler concurrently
            tokio::select! {
                result = brightflow_api::serve(config) => {
                    result?;
                }
                _ = run_scheduler() => {}
            }
        }

        Commands::Serve {
            host,
            port,
            dataset,
        } => {
            init_tracing("brightflow=info,brightflow_api=debug,tower_http=debug");

            let config = build_serve_config(&host, port, dataset);
            brightflow_api::serve(config).await?;
        }

        Commands::Schedule => {
            init_tracing("brightflow=info");
            run_scheduler().await;
        }

        Commands::Insights(insights_cmd) => {
            match insights_cmd {
                InsightsCommands::Review { args, cadence } => match cadence {
                    CadenceArg::All => {
                        for c in ReviewCadence::all() {
                            run_review(&args, *c)?;
                        }
                    }
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
                }
            }
        }
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

fn build_serve_config(host: &str, port: u16, dataset: Option<String>) -> ServeConfig {
    let host_parts: Vec<u8> = host.split('.').filter_map(|p| p.parse().ok()).collect();
    let host_array: [u8; 4] = host_parts.try_into().unwrap_or([127, 0, 0, 1]);

    ServeConfig {
        host: host_array,
        port,
        default_dataset: dataset.or_else(|| std::env::var("BRIGHTFLOW_DEFAULT_DATASET").ok()),
    }
}

async fn run_scheduler() {
    tracing::info!("Starting scheduler (not yet implemented)");
    // Placeholder: scheduler will loop here processing jobs
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        tracing::debug!("Scheduler tick");
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
    println!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
    let tree = engine.run_review_with_cadence(&df, &data_schema, cadence, &DebugLog::disabled())?;

    write_outputs(args, &tree, &suffix, &schema_name, cadence.title())?;

    println!(
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
    println!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth);
    let tree = engine.run_report(&df, &data_schema, report_type, &DebugLog::disabled())?;

    write_outputs(args, &tree, suffix, &schema_name, report_type.title())?;

    println!(
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

    let json_path = input_dir.join(format!("{}_{}.json", input_stem, suffix));
    let md_path = input_dir.join(format!("{}_{}.md", input_stem, suffix));
    let html_path = input_dir.join(format!("{}_{}.html", input_stem, suffix));

    let title = match schema_name {
        Some(name) => format!("{} - {}", name, type_title),
        None => type_title.to_string(),
    };

    write_output(&json_path, tree, args.pretty)?;
    write_markdown(&md_path, tree, Some(&title))?;
    write_html(&html_path, tree, Some(&title))?;

    Ok(())
}
