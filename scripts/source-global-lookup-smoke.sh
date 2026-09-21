#!/usr/bin/env bash
# Nested source loading must publish globals visible to already compiled code.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: source-global-lookup-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-source-globals.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cat >"$scratch/runner.birnam" <<'BIRNAM'
[System load: "modules/os.birnam"];
[System load: "modules/portable-code-test.birnam"];
[Test runAllSuitesWithRunner: ConsoleRunner];
[[ConsoleRunner allPassed] ifTrue: [| [OS exit: 0]] ifFalse: [| [OS exit: 1]]]
BIRNAM
for mode in installed source compiled; do
  case $mode in
    installed) "$bin" "$scratch/runner.birnam" >"$scratch/$mode.out" ;;
    source) BIRNAM_FORCE_SOURCE=1 "$bin" "$scratch/runner.birnam" >"$scratch/$mode.out" ;;
    compiled)
      "$bin" --compile-module "$scratch/runner.birnam" "$scratch/runner.dylib"
      "$bin" "$scratch/runner.dylib" >"$scratch/$mode.out" ;;
  esac
  # Exit status alone used to accept a run that silently executed no assertions.
  python3 - "$scratch/$mode.out" <<'PY'
import pathlib, re, sys
output = pathlib.Path(sys.argv[1]).read_text()
totals = re.findall(r'^TOTAL: (\d+) passed, (\d+) failed$', output, re.M)
assert len(totals) == 1, output
passed, failed = map(int, totals[0])
assert passed >= 114 and failed == 0, output
assert len(re.findall(r'^  ok   ', output, re.M)) == passed, output
PY
  printf 'source-global-lookup-smoke: %s executed Portable Code assertions\n' "$mode"
done
