# Brightflow

## Project Overview

Analytics platform with a Rust backend (Axum + Polars) and Vue 3 frontend.

## Tech Stack

- **Backend**: Rust, Axum, Polars, Delta Lake
- **Frontend**: Vite 7, Vue 3, TypeScript, Nuxt UI 4, Tailwind CSS 4, Pinia

## Project Structure

- `crates/brightflow-cli` - Binary (run-all, serve, schedule, insights)
- `crates/brightflow-core` - Shared types (TenantId, DatasetId, errors)
- `crates/brightflow-connect` - Data connectors
- `crates/brightflow-store` - Delta Lake storage
- `crates/brightflow-insights` - Statistical analysis engine
- `crates/brightflow-api` - HTTP API server (Axum + Polars)
- `brightflow-app/` - Vue 3 frontend (see its CLAUDE.md for detailed style rules and conventions)

## Code Style

- **Rust**: `cargo fmt`, `cargo clippy`, `cargo audit`
- **Frontend**: Biome (single quotes, semicolons, trailing commas), oxlint, strict TypeScript

## Workflow

- When debugging or testing changes, run both backend and frontend as background tasks to monitor output
- Frontend has `vite-console-forward-plugin` — browser console output forwards to the Vite terminal
- After Rust work is complete and debug compilation succeeds, always finish with `cargo build --release`

## Commands

```bash
# Rust
cargo check                       # verify compilation
cargo build --release             # release build (always run after debug succeeds)
cargo fmt --check                 # check formatting
cargo clippy                      # lint
cargo audit                       # check dependencies for vulnerabilities

# Frontend (from brightflow-app/)
npm run dev                       # vite dev server
npm run check                     # type-check + lint + format
npm run format                    # auto-fix formatting

# Backend dev server
cargo run -- run-all              # API + WebSocket server

# Git hooks
./scripts/install-hooks.sh        # install pre-commit hooks
```
