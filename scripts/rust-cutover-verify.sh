#!/usr/bin/env bash
# Compatibility entrypoint for full release qualification; one plan owns gates.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/validate.py --suite release "$@"
