#!/usr/bin/env bash
# test-env.sh — manage the persistent interactive test workspace.
#
# Two lifecycles, one template (see plans/2026-08-29_test-workspace-architecture.md):
#   - the committed template testdata/workspaces/test/ is the single source of truth,
#   - the *ephemeral* copy for the automated suite is created per-run by the
#     brightflow-test-support helper (never by this script),
#   - this script manages the *persistent* copy at data/workspaces/test/ that
#     interactive work (run-all, frontend dev, bug repro, demo) uses.
#
# The persistent env is wiped only deliberately, via `reset` — never on boot,
# so working state survives restarts. Schema drift is handled by the existing
# self-migrate-on-open machinery, so `setup`/`reset` are about content, not schema.
#
# Usage:
#   test-env.sh setup     copy template → data/workspaces/test (first / rebuild)
#   test-env.sh reset     rm the runtime copy, re-copy from template (discard state)
#   test-env.sh status    show where template and runtime live + schema drift

set -euo pipefail

# This script's parent dir is scripts/; one hop up is the repo root.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/testdata/workspaces/test"
RUNTIME="$ROOT/data/workspaces/test"

usage() {
    sed -n '2,14p' "${BASH_SOURCE[0]}"
}

copy_template() {
    if [[ ! -d "$TEMPLATE" ]]; then
        echo "error: template not found at $TEMPLATE" >&2
        echo "  Rebuild it per the plan §4, or set BRIGHTFLOW_TESTDATA_DIR." >&2
        exit 1
    fi
    rm -rf "$RUNTIME"
    mkdir -p "$(dirname "$RUNTIME")"
    cp -R "$TEMPLATE" "$RUNTIME"
    # Sanity: the committed DBs must be checklist-checkpointed (no WAL/shm
    # sidecars), or the copy would carry a live journal and `cp -R` would be
    # unsafe. The .gitignore guards make a stray sidecar un-committable, but a
    # local rebuild could still leave one — catch it before it propagates.
    if compgen -G "$RUNTIME/*.db-wal" >/dev/null || compgen -G "$RUNTIME/*.db-shm" >/dev/null; then
        echo "error: copied workspace has WAL/shm sidecars — template is not cleanly checkpointed" >&2
        exit 1
    fi
    echo "Copied template → $RUNTIME"
}

schema_version() {
    # Column `version` exists after any migration has run; an un-migrated DB
    # has no schema_migrations table at all.
    sqlite3 "$1" "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;" 2>/dev/null \
        || echo "?" 
}

status() {
    echo "Template: $TEMPLATE"
    if [[ -d "$TEMPLATE" ]]; then
        echo "  schema: $(schema_version "$TEMPLATE/litehouse.db")"
    else
        echo "  (missing)"
    fi
    echo "Runtime:  $RUNTIME"
    if [[ -d "$RUNTIME" ]]; then
        echo "  schema: $(schema_version "$RUNTIME/litehouse.db")"
    else
        echo "  (not set up)"
    fi
    if [[ -d "$TEMPLATE" && -d "$RUNTIME" ]]; then
        local tv rv
        tv="$(schema_version "$TEMPLATE/litehouse.db")"
        rv="$(schema_version "$RUNTIME/litehouse.db")"
        if [[ -n "$tv" && "$tv" != "?" && "$tv" -ne "$rv" ]]; then
            echo "  schema DIFFERS (template $tv vs runtime $rv) — the runtime copy will self-migrate on open, or run 'reset' for a clean baseline."
        else
            echo "  schema matches"
        fi
    fi
}

case "${1:-}" in
    setup) copy_template ;;
    reset) copy_template ;;
    status) status ;;
    -h|--help|help) usage ;;
    *) usage; exit 2 ;;
esac
