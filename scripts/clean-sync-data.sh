#!/usr/bin/env bash
# Clean all synced data while preserving user accounts and settings.
#
# Removes:
#   - Parquet tables in data/store/
#   - Parquet connector output in data/github/
#   - Litehouse metadata (data/litehouse.db)
#   - SQLite: sync_runs, sync_state, scheduler_jobs, connector_configs
#
# Preserves:
#   - SQLite: users, user_settings, tower_sessions, _sqlx_migrations (auth.db)
#   - Connector config files in configs/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

SCHEDULER_DB="$ROOT_DIR/data/scheduler.db"
LITEHOUSE_DB="$ROOT_DIR/data/litehouse.db"
STORE_DIR="$ROOT_DIR/data/store"
GITHUB_DIR="$ROOT_DIR/data/github"

echo "=== Brightflow Sync Data Cleanup ==="
echo ""
echo "This will remove:"
echo "  - Parquet tables in $STORE_DIR"
echo "  - Parquet output in $GITHUB_DIR"
echo "  - Litehouse metadata ($LITEHOUSE_DB)"
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

# 1. Clean Parquet tables
if [ -d "$STORE_DIR" ]; then
    echo "Removing Parquet tables in $STORE_DIR..."
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

# 3. Clean Litehouse metadata database
if [ -f "$LITEHOUSE_DB" ]; then
    echo "Removing Litehouse metadata ($LITEHOUSE_DB)..."
    rm -f "$LITEHOUSE_DB" "$LITEHOUSE_DB-shm" "$LITEHOUSE_DB-wal"
    echo "  Done."
else
    echo "No Litehouse database found, skipping."
fi

# 4. Clean SQLite scheduler sync data
if [ -f "$SCHEDULER_DB" ]; then
    if command -v sqlite3 &>/dev/null; then
        echo "Cleaning scheduler sync data (preserving config)..."
        sqlite3 "$SCHEDULER_DB" <<'SQL'
DELETE FROM sync_runs;
DELETE FROM sync_state;
DELETE FROM scheduler_jobs;
DELETE FROM connector_configs;
SQL
    else
        echo "sqlite3 CLI not found — removing scheduler database file."
        echo "  (Backend will recreate it on startup.)"
        rm -f "$SCHEDULER_DB" "$SCHEDULER_DB-shm" "$SCHEDULER_DB-wal"
    fi
    echo "  Done."
else
    echo "No scheduler database found at $SCHEDULER_DB, skipping."
fi

echo ""
echo "Cleanup complete. Restart the backend to re-index tables."
