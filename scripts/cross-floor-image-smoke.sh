#!/usr/bin/env bash
# Design 66 Z4: image compatibility must follow the declared authority
# vectors. Production images are authored by Rust Image/Codegen; rollback
# images are consumed by the complete Zig vector. The former R5 probe tried to
# make an individual Zig floor publish a Rust-generation image, which is a
# deliberately rejected mixed-authority operation.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd -P)
echo "cross-floor-image: running authority-correct production/rollback archive matrix"
bash "$repo/scripts/z3-archive-distribution-smoke.sh"
echo "cross-floor-image: authority-correct image compatibility passed"
