#!/usr/bin/env bash
set -euo pipefail
exec bash "$(dirname "$0")/deferred-storage-test.sh" "$@"
