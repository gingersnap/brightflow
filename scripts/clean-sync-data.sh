#!/usr/bin/env bash
# Clean all synced data while preserving user accounts and settings.
#
# Removes:
#   - Delta Lake tables in data/store/
#   - Parquet connector output in data/github/
#   - SQLite: sync_runs, sync_state, scheduler_jobs, connector_configs
#
# Preserves:
#   - SQLite: users, user_settings, tower_sessions, _sqlx_migrations
#   - Connector config files in configs/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

DB_PATH="$ROOT_DIR/data/brightflow.db"
STORE_DIR="$ROOT_DIR/data/store"
GITHUB_DIR="$ROOT_DIR/data/github"

echo "=== Brightflow Sync Data Cleanup ==="
echo ""
echo "This will remove:"
echo "  - Delta tables in $STORE_DIR"
echo "  - Parquet output in $GITHUB_DIR"
echo "  - Sync runs, sync state, scheduler jobs, connector configs from SQLite"
echo ""
echo "User accounts and settings will be preserved."
echo ""
read -rp "Continue? [y/N] " confirm
if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
    echo "Aborted."
    exit 0
fi

echo ""

# 1. Clean Delta Lake tables
if [ -d "$STORE_DIR" ]; then
    echo "Removing Delta tables in $STORE_DIR..."
    rm -rf "$STORE_DIR"
    mkdir -p "$STORE_DIR"
    echo "  Done."
else
    echo "No store directory found, skipping."
fi

# 2. Clean connector parquet output
if [ -d "$GITHUB_DIR" ]; then
    echo "Removing parquet output in $GITHUB_DIR..."
    rm -rf "$GITHUB_DIR"
    mkdir -p "$GITHUB_DIR"
    echo "  Done."
else
    echo "No github output directory found, skipping."
fi

# 3. Clean SQLite sync data
if [ -f "$DB_PATH" ]; then
    if command -v sqlite3 &>/dev/null; then
        echo "Cleaning SQLite sync data (preserving users)..."
        sqlite3 "$DB_PATH" <<'SQL'
DELETE FROM sync_runs;
DELETE FROM sync_state;
DELETE FROM scheduler_jobs;
DELETE FROM connector_configs;
SQL
    else
        echo "sqlite3 CLI not found — removing database file."
        echo "  (Backend will recreate it on startup; you'll need to log in again.)"
        rm -f "$DB_PATH" "$DB_PATH-shm" "$DB_PATH-wal"
    fi
    echo "  Done."
else
    echo "No database found at $DB_PATH, skipping."
fi

echo ""
echo "Cleanup complete. Restart the backend to re-index tables."
