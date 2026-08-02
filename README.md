# Brightflow

Analytics platform.

## Structure

```
crates/
├── brightflow-core/      # Shared types and errors
├── brightflow-connect/   # Data connectors (Longbow integration)
├── brightflow-store/     # SQLite-backed Parquet storage (Litehouse)
├── brightflow-engine/    # Analysis engine, NLP primitives, enrichment orchestration
├── brightflow-llm/       # Provider-agnostic OpenAI-compatible chat client
├── brightflow-scheduler/ # Background job runner for connector syncs + post-sync insights
├── brightflow-api/       # HTTP API server (Axum + Polars) with integrated event ingestion
└── brightflow-cli/       # Main binary (run-all, serve, schedule, insights, topics)

brightflow-app/       # Vue 3 frontend
```

## Run

```bash
# API + Scheduler together
cargo run -- run-all

# API server only (scheduler is integrated)
cargo run -- serve

# Standalone scheduler daemon (stub — use run-all or serve instead)
cargo run -- schedule

# Insights analysis
cargo run -- insights review --input data.csv

# Frontend
cd brightflow-app && npm run dev
```

## Config

Copy `.env.example` to `.env` for local settings.

> **Note:** `brightflow-connect` depends on the sibling `longbow` crate at
> `../longbow` (path dependency). Clone it alongside this repo to build.
