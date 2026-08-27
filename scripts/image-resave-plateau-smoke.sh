#!/usr/bin/env bash
# Regression gate: current-only load/compact/resave must not grow with image age.
set -euo pipefail

cd "$(dirname "$0")/.."
prefix=${1:?usage: image-resave-plateau-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"
root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-image-resave-plateau.XXXXXX")
trap 'rm -rf "$root"' EXIT

seed="$root/epoch-0.image"
printf '[db retainCurrentOnly]\n[db compact]\n[System gc]\n[System saveImage: "%s"]\n:quit\n' "$seed" \
  | "$bin" --repl >"$root/epoch-0.out" 2>"$root/epoch-0.err"
test ! -s "$root/epoch-0.err"
previous=$seed

for epoch in 1 2 3 4; do
  next="$root/epoch-$epoch.image"
  input="$root/epoch-$epoch.in"
  printf '[db retainCurrentOnly]\n[db compact]\n[System gc]\n"METRICS"\n[db retainedFactCount]\n[db retainedFactBytes]\n[Codegen liveHeapBytes]\n[Codegen liveSymbolCount]\n[Codegen liveSymbolNameBytes]\n[Codegen liveVtableCount]\n[Codegen currentCodeBindingCount]\n[Codegen retiringCodeBindingCount]\n[Codegen codeBindingGeneratedBytes]\n[System saveImage: "%s"]\n:quit\n' "$next" >"$input"
  if ((epoch % 2)); then
    "$bin" "$previous" <"$input" >"$root/epoch-$epoch.out" 2>"$root/epoch-$epoch.err"
  else
    "$bin" --rebuild "$previous" <"$input" >"$root/epoch-$epoch.out" 2>"$root/epoch-$epoch.err"
  fi
  test ! -s "$root/epoch-$epoch.err"
  test -s "$next"
  previous=$next
done

python3 - "$root" <<'PY'
import os
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
rows = []
for epoch in range(1, 5):
    output = (root / f"epoch-{epoch}.out").read_text(encoding="utf-8")
    prompt_values = [line.removeprefix("birnam> ").strip() for line in output.splitlines()]
    marker = prompt_values.index("METRICS")
    values = [int(value) for value in prompt_values[marker + 1:marker + 10]]
    assert len(values) == 9, (epoch, values)
    values.append(os.path.getsize(root / f"epoch-{epoch}.image"))
    rows.append(values)

# Epoch 1 establishes the recovered fixed live set. Mapped and rebuilt resaves
# after that must be byte-for-byte flat in every semantic counter and image size.
assert rows[1] == rows[2] == rows[3], rows
PY

printf 'image-resave-plateau-smoke: mapped/rebuilt current-only resaves plateaued\n'
