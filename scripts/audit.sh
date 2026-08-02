#!/usr/bin/env bash
#
# Supply-chain audit gate for Brightflow.
#
# This is the real gate — it fetches a fresh advisory database and exits non-zero on any
# advisory not explicitly ignored in .cargo/audit.toml. The pre-commit hook deliberately
# does NOT do this: it runs --no-fetch and never blocks, so it can report a stale picture
# and must not be trusted as a clean bill of health. Run this before a release, after a
# dependency bump, and periodically on main.
#
# Ignores live in .cargo/audit.toml, each with a reason and a recheck trigger. If an
# advisory here can't be fixed by upgrading, add it there with that justification rather
# than lowering this gate.
#
# Usage: ./scripts/audit.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"

if ! command -v cargo-audit &> /dev/null; then
    echo -e "${RED}✗ cargo-audit is not installed${NC}"
    echo "  Install with: cargo install cargo-audit"
    exit 1
fi

echo -e "${BLUE}Fetching advisory database and auditing dependencies...${NC}"
echo ""

# No --no-fetch: a fresh DB is the whole point of this script over the hook's check.
if cargo audit; then
    echo ""
    echo -e "${GREEN}✓ No un-ignored advisories.${NC}"
    echo -e "  Ignored entries and their recheck triggers: ${BLUE}.cargo/audit.toml${NC}"
    exit 0
else
    echo ""
    echo -e "${RED}✗ Un-ignored advisories found.${NC}"
    echo "  Fix by upgrading (try 'cargo update' first, then bump direct deps in"
    echo "  crates/*/Cargo.toml). If an advisory is genuinely unreachable, add it to"
    echo "  .cargo/audit.toml with a reason AND a recheck trigger."
    exit 1
fi
