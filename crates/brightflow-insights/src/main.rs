use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

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
#[command(about = "Analyzes CSV data to find statistical insights")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Review { args, cadence } => {
            match cadence {
                CadenceArg::All => {
                    for c in ReviewCadence::all() {
                        run_review(&args, *c)?;
                    }
                    Ok(())
                }
                CadenceArg::Daily => run_review(&args, ReviewCadence::Daily),
                CadenceArg::Weekly => run_review(&args, ReviewCadence::Weekly),
                CadenceArg::Monthly => run_review(&args, ReviewCadence::Monthly),
            }
        }
        Commands::Trends { args } => run_report(&args, ReportType::Trends),
        Commands::Drivers { args } => run_report(&args, ReportType::Drivers),
        Commands::All { args } => {
            // Run all review cadences
            for c in ReviewCadence::all() {
                run_review(&args, *c)?;
            }
            run_report(&args, ReportType::Trends)?;
            run_report(&args, ReportType::Drivers)?;
            Ok(())
        }
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

    // Generate output file paths
    let input_stem = args.input.file_stem().unwrap().to_str().unwrap();
    let input_dir = args.input.parent().unwrap_or(std::path::Path::new("."));

    let json_path = input_dir.join(format!("{}_{}.json", input_stem, suffix));
    let md_path = input_dir.join(format!("{}_{}.md", input_stem, suffix));
    let html_path = input_dir.join(format!("{}_{}.html", input_stem, suffix));

    // Build title
    let title = match &schema_name {
        Some(name) => format!("{} - {}", name, cadence.title()),
        None => cadence.title().to_string(),
    };

    // Write outputs
    write_output(&json_path, &tree, args.pretty)?;
    write_markdown(&md_path, &tree, Some(&title))?;
    write_html(&html_path, &tree, Some(&title))?;

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

    // Generate output file paths
    let input_stem = args.input.file_stem().unwrap().to_str().unwrap();
    let input_dir = args.input.parent().unwrap_or(std::path::Path::new("."));

    let json_path = input_dir.join(format!("{}_{}.json", input_stem, suffix));
    let md_path = input_dir.join(format!("{}_{}.md", input_stem, suffix));
    let html_path = input_dir.join(format!("{}_{}.html", input_stem, suffix));

    // Build title
    let title = match &schema_name {
        Some(name) => format!("{} - {}", name, report_type.title()),
        None => report_type.title().to_string(),
    };

    // Write outputs
    write_output(&json_path, &tree, args.pretty)?;
    write_markdown(&md_path, &tree, Some(&title))?;
    write_html(&html_path, &tree, Some(&title))?;

    println!(
        "[{}] {} root findings, {} total nodes",
        suffix,
        tree.roots.len(),
        tree.nodes.len()
    );

    Ok(())
}
