# Brightflow

Cargo workspace. Run `cargo check` from root to verify all crates compile.

## Crates

- **brightflow-cli** - Main binary (run-all, serve, schedule, insights subcommands)
- **brightflow-core** - Shared types (TenantId, DatasetId, errors)
- **brightflow-connect** - Avon integration for data connectors
- **brightflow-store** - Delta Lake storage
- **brightflow-insights** - Statistical analysis engine (library)
- **brightflow-api** - HTTP API server library (Axum + Polars)

## Frontend

- **brightflow-app** - Vue 3 analytics UI

See each directory's CLAUDE.md for details.
