#!/usr/bin/env bash
#
# Install git hooks for Brightflow
#

set -e

ROOT_DIR="$(git rev-parse --show-toplevel)"
HOOKS_DIR="$ROOT_DIR/.git/hooks"

echo "Installing git hooks..."

# Create symlink for pre-commit hook
ln -sf "../../scripts/pre-commit" "$HOOKS_DIR/pre-commit"

echo "✓ Installed pre-commit hook"
echo ""
echo "Hooks installed! They will run automatically on git commit."
echo "To bypass (not recommended): git commit --no-verify"
