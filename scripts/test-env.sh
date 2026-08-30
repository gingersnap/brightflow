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
#   test-env.sh setup     copy template → data/workspaces/test (first run only)
#   test-env.sh reset     rm the runtime copy, re-copy from template (discard state)
#   test-env.sh status    show where template and runtime live + schema drift

set -euo pipefail

# This script's parent dir is scripts/; one hop up is the repo root.
command -v sqlite3 >/dev/null 2>&1 || {
    echo "error: sqlite3 is required (schema-version reporting)" >&2
    exit 1
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/testdata/workspaces/test"
RUNTIME="$ROOT/data/workspaces/test"

usage() {
    sed -n '2,14p' "${BASH_SOURCE[0]}"
}

copy_template() {
    if [[ ! -d "$TEMPLATE" ]]; then
        echo "error: template not found at $TEMPLATE" >&2
        echo "  Rebuild it with scripts/build-test-template.sh." >&2
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
        # Both sides must be numeric: `[[ "?" -ne 5 ]]` is an arithmetic
        # comparison on a non-number, which evaluates to 0 and silently lies.
        if [[ "$tv" =~ ^[0-9]+$ && "$rv" =~ ^[0-9]+$ && "$tv" -ne "$rv" ]]; then
            echo "  schema DIFFERS (template $tv vs runtime $rv) — the runtime copy will self-migrate on open, or run 'reset' for a clean baseline."
        else
            echo "  schema matches"
        fi
    fi
}

case "${1:-}" in
    # `setup` is the first-run path and refuses to destroy an existing env:
    # both words used to run the same destructive copy, so a second `setup`
    # silently discarded whatever the persistent env had accumulated. `reset`
    # is the one that throws work away, and it says so.
    setup)
        if [[ -d "$RUNTIME" ]]; then
            echo "error: $RUNTIME already exists — 'setup' will not overwrite it." >&2
            echo "  Use 'test-env.sh reset' to discard it and re-copy the template." >&2
            exit 1
        fi
        copy_template
        ;;
    reset) copy_template ;;
    status) status ;;
    -h|--help|help) usage ;;
    *) usage; exit 2 ;;
esac
