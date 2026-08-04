//! Quality regression: the CLEANED pipeline must recover planted topic
//! clusters that format noise (URLs, mentions, emoji, code fences) obscures.
//!
//! Uses a deterministic bag-of-tokens hash embedder rather than the real
//! Model2Vec model so the test runs hermetically in CI — the property under
//! test is the pipeline (clean → embed → cluster), and a token-averaging
//! embedder has exactly the noise-sensitivity that makes cleaning matter.

#![expect(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "integration tests panic on failure by design"
)]

use brightflow_engine::nlp::cluster_metrics::adjusted_rand_index;
use brightflow_engine::nlp::{clean_for_embedding, kmeans_dense, CleaningProfile};

const EMBED_DIM: usize = 128;
const DOCS_PER_CLUSTER: usize = 150;

/// Deterministic hash-bucket bag-of-tokens embedding, L2-normalized —
/// a stand-in with Model2Vec's failure mode: every token pulls the vector.
fn hash_embed(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for token in text.to_lowercase().split_whitespace() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in token.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        v[(h % EMBED_DIM as u64) as usize] += 1.0;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    for x in &mut v {
        *x /= norm;
    }
    v
}

/// 4 planted vocab clusters × injected format noise.
fn build_corpus() -> (Vec<String>, Vec<usize>) {
    let vocab: [&[&str]; 4] = [
        &[
            "payment",
            "invoice",
            "billing",
            "refund",
            "subscription",
            "charge",
            "receipt",
        ],
        &[
            "login",
            "password",
            "authentication",
            "session",
            "oauth",
            "token",
            "credentials",
        ],
        &[
            "dashboard",
            "chart",
            "visualization",
            "widget",
            "graph",
            "metric",
            "panel",
        ],
        &[
            "database",
            "migration",
            "schema",
            "query",
            "index",
            "transaction",
            "replica",
        ],
    ];
    // Format noise: heavy, shared across ALL clusters — the trap that makes an
    // uncleaned bag-of-tokens embedder cluster by format instead of topic.
    let noise: [&str; 6] = [
        "https://example.com/a/very/long/tracking/link?utm_source=noise&utm_campaign=x1",
        "@some.handle.bsky.social 🚀🔥👍🎉😅",
        "```\nlet unrelated = code_snippet();\npanic!(\"boom\");\n```",
        "<!-- template instructions here --> ### Steps to reproduce",
        "https://cdn.example.org/image.png 📸 ![screenshot](https://x.io/s.png)",
        "@another_user 👋 www.tracking-pixel.net/xyz 💯💯💯",
    ];

    let mut docs = Vec::new();
    let mut truth = Vec::new();
    for (cluster, words) in vocab.iter().enumerate() {
        for i in 0..DOCS_PER_CLUSTER {
            // 5 topical words, deterministic rotation
            let mut parts: Vec<String> = (0..5)
                .map(|j| words[(i * 3 + j * 5 + cluster) % words.len()].to_string())
                .collect();
            // 2 noise fragments — identical across clusters
            parts.push(noise[i % noise.len()].to_string());
            parts.push(noise[(i * 7 + 1) % noise.len()].to_string());
            docs.push(parts.join(" "));
            truth.push(cluster);
        }
    }
    (docs, truth)
}

fn cluster_ari(texts: &[String], truth: &[usize]) -> f64 {
    let vectors: Vec<Vec<f32>> = texts.iter().map(|t| hash_embed(t)).collect();
    let result = kmeans_dense(&vectors, 4, 50);
    adjusted_rand_index(&result.assignments, truth).unwrap_or(0.0)
}

#[test]
fn cleaned_pipeline_recovers_planted_clusters() {
    let (docs, truth) = build_corpus();

    // Uncleaned baseline: raw text straight into the embedder
    let raw_ari = cluster_ari(&docs, &truth);

    // Cleaned pipeline: the 2A social/markdown cleaners strip the format noise
    let cleaned: Vec<String> = docs
        .iter()
        .map(|d| {
            clean_for_embedding(d, CleaningProfile::MarkdownIssue)
                .or_else(|| clean_for_embedding(d, CleaningProfile::Social))
                .unwrap_or_default()
        })
        .collect();
    assert!(
        cleaned.iter().all(|c| !c.is_empty()),
        "all planted docs must stay eligible after cleaning"
    );
    let cleaned_ari = cluster_ari(&cleaned, &truth);

    eprintln!("ARI raw={raw_ari:.3} cleaned={cleaned_ari:.3}");
    assert!(
        cleaned_ari >= 0.9,
        "cleaned pipeline must recover planted clusters: ARI={cleaned_ari:.3}"
    );
    assert!(
        cleaned_ari > raw_ari,
        "cleaning must beat the uncleaned baseline ({cleaned_ari:.3} vs {raw_ari:.3})"
    );
}
