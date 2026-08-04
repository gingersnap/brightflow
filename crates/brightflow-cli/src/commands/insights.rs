//! `insights *`: run review/trends/drivers reports over a CSV and write
//! the markdown/HTML/JSON outputs.

use anyhow::Result;

use brightflow_engine::analysis::engine::AnalysisEngine;
use brightflow_engine::analysis::scoring::ScoringContext;
use brightflow_engine::analysis::tree::{ReportType, ReviewCadence};
use brightflow_engine::data::loader::load_csv;
use brightflow_engine::data::schema::detect_schema;
use brightflow_engine::output::html::write_html;
use brightflow_engine::output::json::write_output;
use brightflow_engine::output::markdown::write_markdown;

use crate::AnalyzeArgs;

pub(crate) fn run_review(args: &AnalyzeArgs, cadence: ReviewCadence) -> Result<()> {
    let df = load_csv(&args.input)?;
    let data_schema = detect_schema(&df)?;

    let suffix = format!("review_{}", cadence.suffix());
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth)
        .with_scoring_ctx(
            ScoringContext::new().with_kpis(data_schema.kpi_columns.iter().cloned().collect()),
        );
    let result = engine.run_review_with_cadence(&df, &data_schema, cadence)?;
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

pub(crate) fn run_report(args: &AnalyzeArgs, report_type: ReportType) -> Result<()> {
    let df = load_csv(&args.input)?;
    let data_schema = detect_schema(&df)?;

    let suffix = report_type.suffix();
    tracing::info!("[{}] Running...", suffix);

    let engine = AnalysisEngine::new(args.z_threshold, args.p_threshold, args.max_depth)
        .with_scoring_ctx(
            ScoringContext::new().with_kpis(data_schema.kpi_columns.iter().cloned().collect()),
        );
    let result = engine.run_report(&df, &data_schema, report_type)?;
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
