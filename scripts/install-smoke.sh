#!/usr/bin/env bash
# Verify the current published closure and all installed Host modes after relocation.
set -euo pipefail
cd "$(dirname "$0")/.."
exec bash scripts/host-cli-test.sh --prefix "${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}" --group all
