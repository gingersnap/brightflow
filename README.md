# Brightflow

Analytics platform.

## Structure

```
crates/
├── brightflow-core/      # Shared types and errors
├── brightflow-connect/   # Data connectors (Avon integration)
├── brightflow-store/     # Delta Lake storage
├── brightflow-insights/  # Statistical analysis engine
├── brightflow-api/       # HTTP API server
└── brightflow/           # Main binary

brightflow-explore/       # Vue frontend
```

## Run

```bash
# API server
cargo run --bin brightflow-api

# Insights CLI
cargo run --bin brightflow-insights -- --help

# Frontend
cd brightflow-explore && npm run dev
```

## Config

Copy `.env.example` to `.env` for local settings.
