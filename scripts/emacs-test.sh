#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

emacs_bin="${BIRNAM_EMACS:-}"
if [[ -z "$emacs_bin" ]]; then
  emacs_bin="$(command -v emacs || true)"
fi
if [[ -z "$emacs_bin" || ! -x "$emacs_bin" ]]; then
  echo "emacs-test: set BIRNAM_EMACS to an Emacs executable" >&2
  exit 1
fi

exec "$emacs_bin" --batch -Q \
  -L "$repo_root/tools/emacs" \
  -l "$repo_root/tools/emacs/birnam-mode-test.el" \
  -f ert-run-tests-batch-and-exit
