#!/usr/bin/env bash
# build-test-template.sh — produce the committed test workspace template.
#
# This script is the authority on how testdata/workspaces/test/ is made. §4 of
# plans/2026-08-29_test-workspace-architecture.md is the record of the decision
# to build it this way; the procedure itself lives here, executable, because a
# 300 KB binary artifact whose provenance is prose can be rebuilt by nobody and
# reviewed by no one.
#
# The template is *built through the real paths* — the real CLI and the real
# HTTP API against a real server — so it is fixture data produced the way the
# product produces data, never hand-written schema. Every database self-creates
# and self-migrates on open, so this script never knows a schema.
#
# Usage:
#   build-test-template.sh            rebuild and replace the committed template
#   build-test-template.sh --check    rebuild into a temp dir and report drift
#                                     against the committed template (no writes)
#
# --check compares *logical* content (a sqlite3 .dump minus volatile columns,
# plus table row counts), never bytes: ids are uuids and timestamps are wall
# clock, so two correct builds never match byte for byte.
#
# POSIX/Linux/macOS only: the store lays out source directories as `source:name`
# and a colon is not a legal path character on Windows.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/testdata/workspaces/test"
SEED="$ROOT/testdata/seed"

# The committed demo account. Mirrored by brightflow-app/src/testing/withBackend.ts;
# changing it here means changing it there.
DEMO_EMAIL="test@brightflow.local"
DEMO_NAME="Test User"
DEMO_PASSWORD='brightflow-test-pass!'

# The one connector-style table the fixture ships, and the ingest source that
# the event fixtures are attributed to.
CONNECTOR_SOURCE="connector:sample"
EVENT_DOMAIN="fixture.brightflow.local"

CHECK_ONLY=0
[[ "${1:-}" == "--check" ]] && CHECK_ONLY=1

log() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
die() { printf '\033[31merror: %s\033[0m\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }
need sqlite3
need curl

BUILD_DIR="$(mktemp -d)"
SERVER_PID=""
cleanup() {
    if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        # SIGTERM, then wait: the clean shutdown is what checkpoints the WAL and
        # removes the -wal/-shm sidecars, which is what makes the folder copyable.
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$BUILD_DIR"
}
trap cleanup EXIT

WS="$BUILD_DIR/workspaces/test"
export BRIGHTFLOW_DATA_DIR="$BUILD_DIR"
export BRIGHTFLOW_WORKSPACE="test"

log "Building the CLI"
cargo build -q --bin brightflow

BF="$ROOT/target/debug/brightflow"

# A free port for the server phase. `run-all` binds a port it is told, not an
# ephemeral one, so the port is chosen here: a failed TCP connect means nothing
# is listening. Racy in principle, fine for a single-user build script.
find_free_port() {
    local p
    for p in $(seq 45000 45200); do
        if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then
            echo "$p"
            return 0
        fi
        exec 3>&- 2>/dev/null || true
    done
    die "no free port in 45000-45200"
}

# ---------------------------------------------------------------------------
# 1. Schema. Every database self-migrates on open; touching one is enough to
#    create the workspace and bring it to the current schema version.
# ---------------------------------------------------------------------------
log "Creating and migrating a fresh workspace"
"$BF" store list >/dev/null

# ---------------------------------------------------------------------------
# 2. The demo user, hashed exactly the way the server verifies.
# ---------------------------------------------------------------------------
log "Seeding the demo user"
BRIGHTFLOW_ADMIN_PASSWORD="$DEMO_PASSWORD" \
    "$BF" create-admin --email "$DEMO_EMAIL" --name "$DEMO_NAME" >/dev/null

# ---------------------------------------------------------------------------
# 3. Connector-style tables, ingested from the reviewable CSV seeds.
# ---------------------------------------------------------------------------
log "Ingesting seed tables"
# issues: short text, no dates — the Text Explorer / topics fixture.
"$BF" store ingest --source "$CONNECTOR_SOURCE" issues -i "$SEED/issues.csv" >/dev/null
# orders: a real daily time series with dimensions and a planted anomaly, so
# the insight analyses have something to find. `issues` cannot serve that role
# (no date column, no measure), which is why the fixture carries both.
"$BF" store ingest --source "$CONNECTOR_SOURCE" orders -i "$SEED/orders.csv" >/dev/null

# ---------------------------------------------------------------------------
# 4. Everything that only exists behind the HTTP API: boot the real router and
#    drive it as a client would.
# ---------------------------------------------------------------------------
# Production `run-all`, not the test harness's `test-server`: run-all is what
# §4 prescribes and what a real deployment runs, so the template is built by the
# same boot path that will later open it. It also starts the scheduler, which is
# what creates and migrates scheduler.db — test-server never touches it, so a
# template built with test-server would be missing that database entirely.
log "Booting run-all to seed over HTTP"
PORT="$(find_free_port)"
API="http://127.0.0.1:$PORT"
"$BF" run-all --host 127.0.0.1 --port "$PORT" >"$BUILD_DIR/server.out" 2>"$BUILD_DIR/server.err" &
SERVER_PID=$!

ready=0
for _ in $(seq 1 300); do
    if curl -sS -o /dev/null "$API/api/health" 2>/dev/null; then
        ready=1
        break
    fi
    kill -0 "$SERVER_PID" 2>/dev/null || die "server exited early: $(tail -20 "$BUILD_DIR/server.err")"
    sleep 0.1
done
[[ "$ready" == "1" ]] || die "server never answered /api/health: $(tail -20 "$BUILD_DIR/server.err")"

JAR="$BUILD_DIR/cookies"

# Every seeding call goes through here, and every one of them must succeed: a
# silently-ignored 400 is a fixture that is quietly missing from the template,
# which is far worse than a failed build. So the status is checked explicitly
# (curl -f would hide the response body that says what was wrong).
api() {
    local method="$1" path="$2" body="${3:-}" out status
    out="$BUILD_DIR/api.out"
    if [[ -n "$body" ]]; then
        status=$(curl -sS -b "$JAR" -c "$JAR" -X "$method" "$API$path" \
            -H 'Content-Type: application/json' -d "$body" -o "$out" -w '%{http_code}')
    else
        status=$(curl -sS -b "$JAR" -c "$JAR" -X "$method" "$API$path" \
            -o "$out" -w '%{http_code}')
    fi
    if [[ "$status" -lt 200 || "$status" -ge 300 ]]; then
        die "$method $path -> HTTP $status: $(head -c 400 "$out")"
    fi
    cat "$out"
}

log "Logging in as the demo user"
api POST /api/auth/login "{\"email\":\"$DEMO_EMAIL\",\"password\":\"$DEMO_PASSWORD\"}" >/dev/null

log "Seeding the ingest source"
api POST /api/sources "{\"domain\":\"$EVENT_DOMAIN\",\"name\":\"Fixture Site\"}" >/dev/null

# ---------------------------------------------------------------------------
# 4b. Events, posted exactly as the tracking script posts them.
#
# What the HTTP path does and does not let a fixture control: visitor ids are
# derived server-side from IP + user-agent under a rotating salt, and event
# timestamps are server-now. So distinct user-agents are how the fixture gets
# distinct visitors, `userId` is how it gets an identified one — and every
# event necessarily lands in the current session on the current day. A fixture
# spanning days or session gaps is not reachable from here; that would need
# writing rows behind the API's back, which is the thing this script exists to
# avoid.
# ---------------------------------------------------------------------------
log "Seeding events"
UA_A='Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36'
UA_B='Mozilla/5.0 (iPhone; CPU iPhone OS 18_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Mobile/15E148 Safari/604.1'
UA_C='Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36'

event() {
    local ua="$1" body="$2" status
    status=$(curl -sS -X POST "$API/api/event" -H 'Content-Type: application/json' \
        -A "$ua" -d "$body" -o "$BUILD_DIR/event.out" -w '%{http_code}')
    if [[ "$status" -lt 200 || "$status" -ge 300 ]]; then
        die "POST /api/event -> HTTP $status: $(head -c 300 "$BUILD_DIR/event.out")"
    fi
}

# Anonymous desktop visitor: a small pageview path plus one custom event, so
# the fixture has both a pageview series and a non-pageview event.
event "$UA_A" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/\",\"domain\":\"$EVENT_DOMAIN\",\"referrer\":\"https://www.google.com/\",\"screenWidth\":1920}"
event "$UA_A" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/pricing\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":1920}"
event "$UA_A" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/docs?utm_source=newsletter&utm_medium=email\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":1920}"
event "$UA_A" "{\"name\":\"signup_clicked\",\"url\":\"https://$EVENT_DOMAIN/pricing\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":1920,\"props\":{\"plan\":\"pro\"}}"

# Mobile visitor: one page, so device classification has more than one value.
event "$UA_B" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":390}"

# Identified visitor: userId set, so session continuity by user is represented.
event "$UA_C" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/app\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":1440,\"userId\":\"user-fixture-1\"}"
event "$UA_C" "{\"name\":\"pageview\",\"url\":\"https://$EVENT_DOMAIN/app/settings\",\"domain\":\"$EVENT_DOMAIN\",\"screenWidth\":1440,\"userId\":\"user-fixture-1\"}"

# The flush loop batches on a 60s interval, so the events only reach Parquet
# after one tick. Waiting is what makes the committed fixture contain a real
# events table rather than just buffered rows.
log "Waiting for the event flush (60s interval)"
flushed=0
for _ in $(seq 1 90); do
    if "$BF" store list 2>/dev/null | grep -q "events"; then
        flushed=1
        break
    fi
    sleep 1
done
[[ "$flushed" == "1" ]] || die "events never flushed to the store"

# ---------------------------------------------------------------------------
# 4c. A ticket-classification function with two versions, so the version
#     table and the promoted-config resolution path are represented. Creating
#     it never calls an LLM; only a run does, and the builder never runs one.
# ---------------------------------------------------------------------------
log "Seeding a ticket-classification function"
FN_PATH="/api/sources/$CONNECTOR_SOURCE/tables/issues/functions"
api POST "$FN_PATH" '{"name":"classify","kind":"ticket_classify","config":{"text_columns":["title","body"],"provider_id":"default"}}' \
    >"$BUILD_DIR/enrich.json"
FN_ID="$(sed -E 's/.*"id":"([^"]+)".*/\1/' "$BUILD_DIR/enrich.json")"
[[ -n "$FN_ID" && "$FN_ID" != "$(cat "$BUILD_DIR/enrich.json")" ]] \
    || die "could not create enrichment function: $(cat "$BUILD_DIR/enrich.json")"

api PUT "/api/functions/$FN_ID" '{"config":{"text_columns":["title"],"provider_id":"default"}}' >/dev/null

# ---------------------------------------------------------------------------
# 4d. Insight runs and novelty history. Only the API writes `insight_runs` —
#     the `insights` CLI subcommand analyses a CSV and prints JSON without
#     touching the store — so these have to be driven over HTTP. Manual runs
#     (not auto) are used deliberately: only a manual run records novelty, so
#     this is what puts rows in both insight_runs and the history table.
# ---------------------------------------------------------------------------
log "Seeding insight runs"
INSIGHT_TARGET="{\"sourceId\":\"$CONNECTOR_SOURCE\",\"datasetId\":\"orders\"}"
api POST /api/insights/review "{\"sourceId\":\"$CONNECTOR_SOURCE\",\"datasetId\":\"orders\",\"cadence\":\"weekly\"}" >"$BUILD_DIR/review.json"
api POST /api/insights/trends "$INSIGHT_TARGET" >"$BUILD_DIR/trends.json"
api POST /api/insights/drivers "$INSIGHT_TARGET" >"$BUILD_DIR/drivers.json"

# ---------------------------------------------------------------------------
# 5. Stop the server, then checkpoint. SQLite drops the -wal/-shm sidecars when
#    the *last* connection closes cleanly, and `test-server` has no graceful
#    shutdown — it is killed with its pools open, which leaves them behind. So
#    the checkpoint is done here, explicitly: open each database once more as
#    the sole connection and let that connection close properly. A sidecar
#    surviving that is a real problem (something still holds the file), so it
#    stays a hard failure rather than a cleanup.
# ---------------------------------------------------------------------------
log "Stopping the server"
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

# Every database in the tree, not just the workspace root: the per-source event
# buffers live under events-buffer/ and carry WALs of their own.
log "Checkpointing the WAL"
while IFS= read -r db; do
    sqlite3 "$db" "PRAGMA wal_checkpoint(TRUNCATE); VACUUM;" >/dev/null
done < <(find "$WS" -type f -name '*.db')

leftover="$(find "$WS" -type f \( -name '*.db-wal' -o -name '*.db-shm' \) | head -5)"
if [[ -n "$leftover" ]]; then
    die "WAL/shm sidecars survived the checkpoint (something still holds a connection):
$leftover"
fi

# Directories git cannot carry (it stores no empty trees) get a .gitkeep, so a
# fresh clone has the same shape a built workspace does.
for d in events events-buffer connector-configs; do
    mkdir -p "$WS/$d"
    touch "$WS/$d/.gitkeep"
done

# ---------------------------------------------------------------------------
# 6. Publish, or report drift.
# ---------------------------------------------------------------------------

# What "drifted" means here is structural, not byte-level. Two correct builds
# never produce identical bytes — argon2 salts, session ids, uuids, wall-clock
# timings and per-build absolute paths all differ — so comparing dumps would cry
# wolf on every run and promptly be ignored. What must match is the shape: the
# schema of every database, the set of tables, how many rows each holds, and the
# identifying values a test would actually assert on.
summarise() {
    local ws="$1" db name
    while IFS= read -r db; do
        name="${db#"$ws"/}"
        echo "### $name"
        # Schema, normalized: autoincrement counters and sqlite internals vary.
        sqlite3 "$db" .schema | grep -v "sqlite_sequence" | sort
        # Row counts per table: catches a fixture that silently stopped landing.
        while IFS= read -r t; do
            echo "count $t = $(sqlite3 "$db" "SELECT count(*) FROM \"$t\";")"
        done < <(sqlite3 "$db" "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name;")
    done < <(find "$ws" -type f -name '*.db' | sort)

    # The identifying values specs assert on. If one of these moves, a test
    # somewhere is about to fail, which is exactly what --check should catch.
    local lh="$ws/litehouse.db"
    if [[ -f "$lh" ]]; then
        echo "### identities"
        sqlite3 "$lh" "SELECT 'table: '||source_id||'/'||name FROM tables ORDER BY 1;"
        sqlite3 "$lh" "SELECT 'run: '||source_id||'/'||table_name||'/'||report_type FROM insight_runs ORDER BY 1;"
        sqlite3 "$lh" "SELECT 'function: '||name||'/'||kind||'/'||status||'/v'||current_version FROM enrichment_functions ORDER BY 1;"
        sqlite3 "$lh" "SELECT 'migration: '||version||' '||name FROM schema_migrations ORDER BY version;"
    fi
    if [[ -f "$ws/auth.db" ]]; then
        sqlite3 "$ws/auth.db" "SELECT 'user: '||email FROM users ORDER BY 1;"
    fi
    if [[ -f "$ws/ingest.db" ]]; then
        sqlite3 "$ws/ingest.db" "SELECT 'ingest source: '||domain FROM sources ORDER BY 1;"
    fi

    # Stored file paths must stay workspace-relative, or the catalog stops
    # pointing at its data the moment the template is copied.
    if [[ -f "$lh" ]]; then
        echo "### stored paths"
        sqlite3 "$lh" "SELECT CASE WHEN path LIKE '/%' THEN 'ABSOLUTE(!): '||path ELSE 'relative' END FROM table_files ORDER BY 1;" \
            | sort | uniq -c | sed -E 's/^ +//'
    fi

    echo "### files"
    # Parquet files are named by uuid and event partitions by build date, so
    # compare the shape rather than the names.
    (cd "$ws" && find . -type f ! -name '*.db' \
        | sed -E "s/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/<uuid>/g" \
        | sed -E "s;/[0-9]{4}-[0-9]{2}-[0-9]{2}/;/<date>/;g" \
        | sort)
}

if [[ "$CHECK_ONLY" == "1" ]]; then
    log "Comparing against the committed template"
    [[ -d "$TEMPLATE" ]] || die "no committed template at $TEMPLATE"
    # Every id in the fixture is a uuid minted at build time, so normalize them
    # everywhere before comparing: what matters is that the same *shape* of
    # source, table and buffer exists, not that it got the same id twice.
    normalize() {
        sed -E "s/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/<uuid>/g"
    }
    if diff -u <(summarise "$TEMPLATE" | normalize) <(summarise "$WS" | normalize); then
        printf '\033[32m✓ committed template matches what this script produces\033[0m\n'
    else
        die "committed template has drifted from this script — rebuild it (run without --check)"
    fi
    exit 0
fi

log "Publishing to $TEMPLATE"
rm -rf "$TEMPLATE"
mkdir -p "$(dirname "$TEMPLATE")"
cp -R "$WS" "$TEMPLATE"
printf '\033[32m✓ template rebuilt at %s\033[0m\n' "$TEMPLATE"
sqlite3 "$TEMPLATE/litehouse.db" "SELECT 'litehouse schema version: ' || MAX(version) FROM schema_migrations;"
