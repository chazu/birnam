#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec bash scripts/runtime-services-test.sh "${1:-}" root-registry
