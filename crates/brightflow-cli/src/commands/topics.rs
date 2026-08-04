//! `topics *`: fit/list/embed/eval the topic model against the store.
//!
//! Config resolution and label alignment live in the engine and store; this
//! module is argument handling plus human-readable output.

use anyhow::Result;

use brightflow_store::ParquetStore;

use crate::TopicsAction;

pub(crate) async fn handle_topics(action: TopicsAction) -> Result<()> {
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
    use brightflow_engine::nlp::{classifier_eval, labelled_feature_rows};

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

    let targets = load_label_targets(&store, source, table, &df)
        .await
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no curated labels for {source}/{table} — run the propose_taxonomy and \
                 label_documents agents, then approve their proposals"
            )
        })?;

    let (features, row_targets) = labelled_feature_rows(&embeddings, &targets.per_row, dim);

    println!("Classifier eval: {source}/{table}");
    println!(
        "  {} labelled rows over {} categories (dim {dim})",
        features.len(),
        targets.names.len()
    );
    if features.is_empty() {
        anyhow::bail!("no rows have both an embedding and a label");
    }

    let Some(eval) = classifier_eval(&features, &row_targets, targets.names.len(), dim) else {
        println!(
            "\n  Not enough signal to train (need >= {} labelled rows and >= {} examples \
             for at least one category).",
            min_labelled_rows_to_train(),
            min_examples_per_label()
        );
        return Ok(());
    };

    let (outcome, base) = (eval.outcome, eval.baseline);

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
    use brightflow_engine::nlp::{find_near_duplicates, DEFAULT_NEAR_DUP_THRESHOLD};

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
    let columns: Vec<&str> = config.text_columns.iter().map(String::as_str).collect();
    let texts: Vec<Option<String>> =
        brightflow_engine::enrichment::build_clean_texts(&df, &columns, config.cleaning_profile)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

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

/// Curated labels for `table`, aligned with `df` (same rule as the API's
/// topics module — the alignment itself lives in the engine).
async fn load_label_targets(
    store: &ParquetStore,
    source: &str,
    table: &str,
    df: &polars::prelude::DataFrame,
) -> Option<brightflow_engine::enrichment::LabelTargets> {
    let table_row = store.db().get_table(source, table).await.ok()??;
    let labels = store.db().get_document_labels(&table_row.id).await.ok()?;
    let pairs: Vec<(String, String)> = labels.into_iter().map(|l| (l.row_id, l.name)).collect();
    brightflow_engine::enrichment::align_label_targets(df, table, &pairs)
}

async fn resolve_enrichment_config(
    store: &ParquetStore,
    source: &str,
    table: &str,
) -> Result<brightflow_engine::enrichment::EnrichmentConfig> {
    let stored = match store.db().get_table(source, table).await {
        Ok(Some(row)) => store
            .db()
            .get_promoted_function_config(&row.id, "topic_model")
            .await
            .ok()
            .flatten(),
        _ => None,
    };
    brightflow_engine::enrichment::resolve_topic_config(table, stored.as_deref(), None)
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
    let labels = load_label_targets(&store, source, table, &df).await;
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

    let (rows, cols) = (enriched.height(), enriched.width());
    store
        .replace_table_data(source, table, enriched, None)
        .await?;

    println!("  Written: {rows} rows, {cols} columns");

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
    use brightflow_engine::embedding::get_backend;
    use brightflow_engine::nlp::{
        default_min_cluster_size, eval_clustering, hdbscan_dense, kmeans_dense,
    };

    let wp = brightflow_core::WorkspacePaths::from_env();
    let store = ParquetStore::new(wp.store(), &wp.litehouse_url()).await?;
    let config = resolve_enrichment_config(&store, source, table).await?;
    let df = store.read_table(source, table).await?;
    println!("Eval on {source}/{table}: {} rows", df.height());

    // Clean once (profile is embedder-independent)
    let columns: Vec<&str> = config.text_columns.iter().map(String::as_str).collect();
    let texts: Vec<Option<String>> =
        brightflow_engine::enrichment::build_clean_texts(&df, &columns, config.cleaning_profile)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
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
        let n_clusters = result.centroids.len();
        let eval = eval_clustering(&vectors, &result.assignments, n_clusters, &clean_texts);

        println!(
            "{:<38} {:<9} {:>4} {:>10} {:>8} {:>7} {:>8.1}% {:>8.1}s",
            embedder_name,
            algo,
            n_clusters,
            eval.silhouette
                .map_or("-".to_string(), |v| format!("{v:.3}")),
            eval.davies_bouldin
                .map_or("-".to_string(), |v| format!("{v:.3}")),
            eval.npmi.map_or("-".to_string(), |v| format!("{v:.3}")),
            eval.unassigned_pct,
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

    store
        .replace_table_data(source, table, enriched, None)
        .await?;

    println!("Done.");
    Ok(())
}
