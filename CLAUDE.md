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

## Development Setup

Install git hooks for local CI (runs on every commit):
```bash
./scripts/install-hooks.sh
```

This runs:
- **Rust**: `cargo fmt --check`, `cargo clippy`, `cargo audit`
- **Frontend**: `npm run type-check`, `npm run lint`, `npm run format:check`
