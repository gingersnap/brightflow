#!/usr/bin/env bash
# Clean all synced data while preserving user accounts.
#
# Removes:
#   - Parquet tables in {workspace}/store/
#   - Litehouse metadata ({workspace}/litehouse.db)
#   - SQLite: sync_runs, sync_state, scheduler_jobs, connector_configs
#
# Preserves:
#   - SQLite: users, tower_sessions, _sqlx_migrations (auth.db)
#   - Connector config files

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Load .env if present
if [ -f "$ROOT_DIR/.env" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$ROOT_DIR/.env"
    set +a
fi

DATA_DIR="${BRIGHTFLOW_DATA_DIR:-$ROOT_DIR/data}"
WS="${BRIGHTFLOW_WORKSPACE:-default}"
WS_DIR="$DATA_DIR/workspaces/$WS"

SCHEDULER_DB="$WS_DIR/scheduler.db"
LITEHOUSE_DB="$WS_DIR/litehouse.db"
STORE_DIR="$WS_DIR/store"

echo "=== Brightflow Sync Data Cleanup ==="
echo ""
echo "Workspace: $WS_DIR"
echo ""
echo "This will remove:"
echo "  - Parquet tables in $STORE_DIR"
echo "  - Litehouse metadata ($LITEHOUSE_DB)"
echo "  - Sync runs, sync state, scheduler jobs, connector configs from SQLite"
echo ""
echo "User accounts will be preserved."
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

# 2. Clean Litehouse metadata database
if [ -f "$LITEHOUSE_DB" ]; then
    echo "Removing Litehouse metadata ($LITEHOUSE_DB)..."
    rm -f "$LITEHOUSE_DB" "$LITEHOUSE_DB-shm" "$LITEHOUSE_DB-wal"
    echo "  Done."
else
    echo "No Litehouse database found, skipping."
fi

# 3. Clean SQLite scheduler sync data
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
