#!/usr/bin/env bash
#
# Mechanical half of the documentation conventions (root CLAUDE.md, ## Conventions).
#
# DIFF-SCOPED BY DESIGN: this only ever inspects *staged* files, never the whole tree.
# That is what lets it land green while a module-doc backlog is still outstanding — it
# cannot flag what you didn't touch, and the backlog cannot grow while it's burned down.
# Do not "improve" this into a tree-wide scan; a check that fails on unrelated files gets
# bypassed, and a bypassed check enforces nothing.
#
# Checks:
#   1. Staged .rs under crates/*/src/  -> must have `//!` within the first 3 lines
#   2. Staged .ts under brightflow-app/src/ (excluding types/generated/)
#                                      -> must have `/**` within the first 3 lines
#   3. New .md anywhere outside docs/, plans/, reports/ -> rejected
#
# Usage: ./scripts/check-conventions.sh   (also run from scripts/pre-commit)

# No `pipefail` here, deliberately. Every check below pipes `git show` into something
# that stops reading early (grep -q, head), which SIGPIPEs the producer — under pipefail
# that reads as a failed check, so a file WITH a valid header gets reported as missing
# one, nondeterministically by file size. Content is captured into a variable first for
# the same reason.
set -u

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"

FAILED=0

# Added/Copied/Modified/Renamed only — deleted files have nothing to check.
STAGED=$(git diff --cached --name-only --diff-filter=ACMR)

if [ -z "$STAGED" ]; then
    echo -e "${GREEN}✓ Conventions (nothing staged)${NC}"
    exit 0
fi

# --- 1. Rust module docs -----------------------------------------------------
# Read from the index, not the worktree: the commit is what's being checked.
MISSING_RS=()
while IFS= read -r f; do
    [ -z "$f" ] && continue
    head3=$(git show ":$f" 2>/dev/null | sed -n '1,3p')
    if ! grep -q '^//!' <<< "$head3"; then
        MISSING_RS+=("$f")
    fi
done <<< "$(echo "$STAGED" | grep -E '^crates/[^/]+/src/.*\.rs$' || true)"

if [ ${#MISSING_RS[@]} -gt 0 ]; then
    echo -e "${RED}✗ Missing '//!' module doc (must be within the first 3 lines):${NC}"
    printf '    %s\n' "${MISSING_RS[@]}"
    echo "  Say why the module exists, not what it contains."
    FAILED=1
fi

# --- 2. Frontend module headers ----------------------------------------------
MISSING_TS=()
while IFS= read -r f; do
    [ -z "$f" ] && continue
    # Header-scoped, like the Rust check: a JSDoc block anywhere in the file is not a
    # module header, and accepting one lets a file with a documented function but no
    # module doc pass.
    head3=$(git show ":$f" 2>/dev/null | sed -n '1,3p')
    if ! grep -q '/\*\*' <<< "$head3"; then
        MISSING_TS+=("$f")
    fi
done <<< "$(echo "$STAGED" \
    | grep -E '^brightflow-app/src/.*\.ts$' \
    | grep -v '^brightflow-app/src/types/generated/' || true)"

if [ ${#MISSING_TS[@]} -gt 0 ]; then
    echo -e "${RED}✗ Missing '/** ... */' module header:${NC}"
    printf '    %s\n' "${MISSING_TS[@]}"
    FAILED=1
fi

# --- 3. Prose files must be normative / dated / generated ---------------------
# Only NEW markdown (filter=A) — existing files outside the allowlist stay editable, so
# this constrains what gets added rather than forcing a migration.
STRAY_MD=()
while IFS= read -r f; do
    [ -z "$f" ] && continue
    case "$f" in
        docs/*|plans/*|reports/*) ;;
        # CLAUDE.md and README.md are the entry points tooling and humans expect by name.
        CLAUDE.md|README.md|*/CLAUDE.md|*/README.md) ;;
        *) STRAY_MD+=("$f") ;;
    esac
done <<< "$(git diff --cached --name-only --diff-filter=A | grep -E '\.md$' || true)"

if [ ${#STRAY_MD[@]} -gt 0 ]; then
    echo -e "${RED}✗ New prose file outside docs/, plans/, reports/:${NC}"
    printf '    %s\n' "${STRAY_MD[@]}"
    echo "  A prose file must be unable to go stale — normative (docs/), dated"
    echo "  (plans/, reports/), or generated. Prose describing current state by hand"
    echo "  belongs inline, next to the code that makes it true."
    FAILED=1
fi

if [ $FAILED -eq 1 ]; then
    echo -e "  See ${BLUE}## Conventions${NC} in CLAUDE.md."
    exit 1
fi

echo -e "${GREEN}✓ Conventions${NC}"
exit 0
