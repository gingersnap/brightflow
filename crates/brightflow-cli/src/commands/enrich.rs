//! `enrich eval`: score Call A (`ticket_classify`) or Call B
//! (`ticket_extract`) against a hand-labelled per-language set.
//!
//! This is a CLI and not a test on purpose: it costs money and needs a live
//! provider. Its numbers decide whether the mention table launches for a
//! language — extraction degrades across languages far faster than
//! classification, so the two calls are scored separately. Cells are
//! computed directly (never cached) against the target table's current
//! vocabulary, so an eval never pollutes the workspace.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use polars::prelude::*;

use brightflow_api::enrichment::runner::{compute_cell, prepare_run_inputs, RunSpec};
use brightflow_api::enrichment::vocab;
use brightflow_engine::enrichment::mentions::ExtractCell;
use brightflow_engine::enrichment::ticket_classify::{ClassifyCell, VocabNames};
use brightflow_engine::enrichment::{FunctionSpec, TicketClassifySpec, TicketExtractSpec};
use brightflow_llm::ChatClient;
use brightflow_store::ParquetStore;

/// Which call to score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalCall {
    A,
    B,
}

impl EvalCall {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "a" | "classify" => Ok(Self::A),
            "b" | "extract" => Ok(Self::B),
            other => Err(anyhow!("unknown call '{other}' (a | b)")),
        }
    }
}

/// One labelled row from the eval CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRow {
    pub id: String,
    pub title: String,
    pub body: String,
    pub expected_category: String,
    pub expected_sentiment: String,
    /// `(type, subject)` pairs; subject empty when not applicable.
    pub expected_mentions: Vec<(String, String)>,
}

/// Parse `type:subject|type:subject`.
pub fn parse_expected_mentions(raw: &str) -> Vec<(String, String)> {
    raw.split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (t, s) = pair.split_once(':').unwrap_or((pair, ""));
            (t.trim().to_lowercase(), s.trim().to_lowercase())
        })
        .collect()
}

fn read_eval_csv(path: &Path) -> Result<Vec<EvalRow>> {
    let df = CsvReadOptions::default()
        .with_has_header(true)
        .try_into_reader_with_file_path(Some(path.to_path_buf()))?
        .finish()
        .with_context(|| format!("reading {}", path.display()))?;
    let col = |name: &str| -> Result<Vec<String>> {
        let s = df
            .column(name)
            .with_context(|| format!("eval CSV needs a '{name}' column"))?
            .as_materialized_series()
            .cast(&DataType::String)?;
        Ok(s.str()?
            .into_iter()
            .map(|o| o.unwrap_or("").to_string())
            .collect())
    };
    let ids = col("id")?;
    let titles = col("title")?;
    let bodies = col("body")?;
    let cats = col("expected_category")?;
    let sentiments = col("expected_sentiment")?;
    let mentions = col("expected_mentions")?;
    let at = |v: &[String], i: usize| v.get(i).cloned().unwrap_or_default();
    Ok((0..df.height())
        .map(|i| EvalRow {
            id: at(&ids, i),
            title: at(&titles, i),
            body: at(&bodies, i),
            expected_category: at(&cats, i).to_lowercase(),
            expected_sentiment: at(&sentiments, i).to_lowercase(),
            expected_mentions: parse_expected_mentions(&at(&mentions, i)),
        })
        .collect())
}

/// Precision / recall of predicted `(type, subject)` pairs against expected.
#[allow(clippy::cast_precision_loss)] // eval sets are a few hundred rows at most
pub fn pair_precision_recall(
    expected: &[(String, String)],
    predicted: &[(String, String)],
) -> (f64, f64) {
    let exp: HashSet<&(String, String)> = expected.iter().collect();
    let pred: HashSet<&(String, String)> = predicted.iter().collect();
    let hits = exp.intersection(&pred).count() as f64;
    let precision = if pred.is_empty() {
        if exp.is_empty() {
            1.0
        } else {
            0.0
        }
    } else {
        hits / pred.len() as f64
    };
    let recall = if exp.is_empty() {
        1.0
    } else {
        hits / exp.len() as f64
    };
    (precision, recall)
}

/// Scoreboard for one run of the eval.
#[derive(Debug, Default)]
pub struct Scores {
    pub rows: usize,
    pub errors: usize,
    pub category_hits: usize,
    pub sentiment_hits: usize,
    pub precision_sum: f64,
    pub recall_sum: f64,
}

impl Scores {
    #[allow(clippy::cast_precision_loss)] // eval sets are a few hundred rows at most
    pub fn report(&self, call: EvalCall) -> String {
        let scored = self.rows.saturating_sub(self.errors).max(1) as f64;
        match call {
            EvalCall::A => format!(
                "rows={} errors={} category_accuracy={:.3} sentiment_accuracy={:.3}",
                self.rows,
                self.errors,
                self.category_hits as f64 / scored,
                self.sentiment_hits as f64 / scored
            ),
            EvalCall::B => format!(
                "rows={} errors={} mention_precision={:.3} mention_recall={:.3}",
                self.rows,
                self.errors,
                self.precision_sum / scored,
                self.recall_sum / scored
            ),
        }
    }
}

/// Run the eval. Provider comes from `BRIGHTFLOW_LLM_BASE_URL` /
/// `_API_KEY` / `_MODEL`, the same seed the server uses.
pub async fn run_eval(
    source: &str,
    table: &str,
    lang: &str,
    call: EvalCall,
    limit: Option<usize>,
) -> Result<()> {
    let base_url = std::env::var("BRIGHTFLOW_LLM_BASE_URL")
        .map_err(|_| anyhow!("BRIGHTFLOW_LLM_BASE_URL is not set"))?;
    let api_key = std::env::var("BRIGHTFLOW_LLM_API_KEY").ok();
    let model = std::env::var("BRIGHTFLOW_LLM_MODEL").unwrap_or_else(|_| "default".to_string());
    let client = ChatClient::new(base_url, api_key, model);

    let path = PathBuf::from("testdata/eval").join(lang).join("issues.csv");
    let mut rows = read_eval_csv(&path)?;
    if let Some(n) = limit {
        rows.truncate(n);
    }
    if rows.is_empty() {
        return Err(anyhow!("no rows in {}", path.display()));
    }

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let table_row = store
        .db()
        .get_table(source, table)
        .await?
        .ok_or_else(|| anyhow!("table '{table}' not found in source '{source}'"))?;

    let text_columns = vec!["title".to_string(), "body".to_string()];
    let mut spec = match call {
        EvalCall::A => FunctionSpec::TicketClassify(TicketClassifySpec {
            text_columns,
            language_column: None,
            provider_id: "eval".to_string(),
            model: None,
            categories: vec![],
            subcategories: vec![],
        }),
        EvalCall::B => FunctionSpec::TicketExtract(TicketExtractSpec {
            text_columns,
            language_column: None,
            provider_id: "eval".to_string(),
            model: None,
            products: vec![],
            competitors: vec![],
            feedback_categories: vec![],
        }),
    };
    vocab::inject(&store, &table_row.id, &mut spec)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let run = vocab::run_spec_for(&store, &table_row.id, spec)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let names: VocabNames = match &run {
        RunSpec::TicketClassify { names, .. } | RunSpec::TicketExtract { names, .. } => {
            (**names).clone()
        },
    };
    let lower_names: HashMap<i64, String> =
        names.iter().map(|(k, v)| (*k, v.to_lowercase())).collect();

    let df = df!(
        "title" => rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>(),
        "body" => rows.iter().map(|r| r.body.clone()).collect::<Vec<_>>(),
    )?;
    let inputs = prepare_run_inputs(&df, &run).map_err(|e| anyhow!("{e}"))?;

    let mut scores = Scores {
        rows: rows.len(),
        ..Scores::default()
    };
    for (row, input) in rows.iter().zip(inputs.iter()) {
        let cell = compute_cell(&client, &run, input).await;
        let Some(value) = cell.value else {
            scores.errors += 1;
            println!("{}\tERROR\t{}", row.id, cell.error.unwrap_or_default());
            continue;
        };
        match call {
            EvalCall::A => {
                let c: ClassifyCell = serde_json::from_value(value)?;
                let got_cat = if c.category_id == 0 {
                    "other".to_string()
                } else {
                    lower_names.get(&c.category_id).cloned().unwrap_or_default()
                };
                let cat_ok = got_cat == row.expected_category;
                let sentiment_ok = c.sentiment == row.expected_sentiment;
                scores.category_hits += usize::from(cat_ok);
                scores.sentiment_hits += usize::from(sentiment_ok);
                println!(
                    "{}\tcategory={}{}\tsentiment={}{}\tsummary={:?}",
                    row.id,
                    got_cat,
                    if cat_ok { "" } else { "✗" },
                    c.sentiment,
                    if sentiment_ok { "" } else { "✗" },
                    c.summary
                );
            },
            EvalCall::B => {
                let c: ExtractCell = serde_json::from_value(value)?;
                let predicted: Vec<(String, String)> = c
                    .mentions
                    .iter()
                    .map(|m| {
                        let subject = m
                            .subject_id
                            .and_then(|id| lower_names.get(&id).cloned())
                            .or_else(|| m.subject_surface.as_ref().map(|s| s.to_lowercase()))
                            .unwrap_or_default();
                        (m.mention_type.clone(), subject)
                    })
                    .collect();
                let (p, r) = pair_precision_recall(&row.expected_mentions, &predicted);
                scores.precision_sum += p;
                scores.recall_sum += r;
                println!(
                    "{}\tprecision={p:.2}\trecall={r:.2}\tpredicted={predicted:?}",
                    row.id
                );
            },
        }
    }
    println!("\n[{lang} / call {call:?}] {}", scores.report(call));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_mentions_parse_pairs_and_lowercase() {
        let pairs = parse_expected_mentions("product:Invoice screen|feedback:| service ");
        assert_eq!(
            pairs,
            vec![
                ("product".to_string(), "invoice screen".to_string()),
                ("feedback".to_string(), String::new()),
                ("service".to_string(), String::new()),
            ]
        );
        assert!(parse_expected_mentions("").is_empty());
    }

    #[test]
    fn precision_recall_handle_empty_sides() {
        let a = vec![("product".to_string(), "x".to_string())];
        assert_eq!(pair_precision_recall(&[], &[]), (1.0, 1.0));
        assert_eq!(pair_precision_recall(&a, &[]), (0.0, 0.0));
        assert_eq!(pair_precision_recall(&[], &a), (0.0, 1.0));
        let b = vec![
            ("product".to_string(), "x".to_string()),
            ("feedback".to_string(), String::new()),
        ];
        let (p, r) = pair_precision_recall(&a, &b);
        assert!((p - 0.5).abs() < 1e-9);
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn scores_report_per_call() {
        let s = Scores {
            rows: 4,
            errors: 0,
            category_hits: 3,
            sentiment_hits: 2,
            precision_sum: 2.0,
            recall_sum: 4.0,
        };
        assert!(s.report(EvalCall::A).contains("category_accuracy=0.750"));
        assert!(s.report(EvalCall::B).contains("mention_recall=1.000"));
        assert!(EvalCall::parse("B").is_ok());
        assert!(EvalCall::parse("c").is_err());
    }
}
