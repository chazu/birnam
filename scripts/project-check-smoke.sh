#!/usr/bin/env bash
# Rust-host project command contract.
set -euo pipefail

exec bash "$(dirname "$0")/rust-project-check-smoke.sh"
