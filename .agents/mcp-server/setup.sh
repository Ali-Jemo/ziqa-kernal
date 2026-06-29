#!/usr/bin/env bash
# Install kernel-engineering MCP server dependencies
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

python3 -m venv "$SCRIPT_DIR/.venv"
"$SCRIPT_DIR/.venv/bin/pip" install mcp
echo "Done. Activate with: source $SCRIPT_DIR/.venv/bin/activate"
