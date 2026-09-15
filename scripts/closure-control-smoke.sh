#!/usr/bin/env bash
# Compatibility entrypoint; the same explicit matrix plan is used by validate.py.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/closure-checks.py --prefix "${1:?installed prefix required}" --suite "${2:-exhaustive}"
