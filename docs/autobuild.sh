#!/usr/bin/env bash
# Sphinx Autobuild Script
# Watches for changes and automatically rebuilds the documentation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Activate virtual environment
if [ -f "$PROJECT_DIR/.venv/bin/activate" ]; then
    source "$PROJECT_DIR/.venv/bin/activate"
fi

cd "$SCRIPT_DIR"

echo "🔄 Starting sphinx-autobuild..."
echo "   Source: $SCRIPT_DIR/source"
echo "   Output: $SCRIPT_DIR/_build/html"
echo "   URL:    http://localhost:8000"
echo ""

sphinx-autobuild source _build/html --host 0.0.0.0 --port 8000
