# Brightflow

Analytics platform.

## Structure

```
crates/
├── brightflow-core/      # Shared types and errors
├── brightflow-connect/   # Data connectors (Longbow integration)
├── brightflow-store/     # SQLite-backed Parquet storage (Litehouse)
├── brightflow-insights/  # Statistical analysis engine
├── brightflow-api/       # HTTP API server
└── brightflow-cli/       # Main binary

brightflow-app/       # Vue frontend
```

## Run

```bash
# API + Scheduler together
cargo run -- run-all

# API server only
cargo run -- serve

# Scheduler only (not yet implemented)
cargo run -- schedule

# Insights analysis
cargo run -- insights review --input data.csv

# Frontend
cd brightflow-app && npm run dev
```

## Config

Copy `.env.example` to `.env` for local settings.
