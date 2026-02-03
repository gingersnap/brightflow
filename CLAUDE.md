# Brightflow

Cargo workspace. Run `cargo check` from root to verify all crates compile.

## Crates

- **brightflow-core** - Shared types (TenantId, DatasetId, errors)
- **brightflow-connect** - Avon integration for data connectors
- **brightflow-store** - Delta Lake storage
- **brightflow-insights** - Statistical analysis CLI
- **brightflow-api** - HTTP API server (Axum + Polars)
- **brightflow** - Main binary entry point

## Frontend

- **brightflow-app** - Vue 3 analytics UI

See each directory's CLAUDE.md for details.
