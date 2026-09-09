#!/usr/bin/env bash
# Verify one immutable candidate archive and emit its Design 60H evidence bundle.
set -euo pipefail

cd "$(dirname "$0")/.."
archive=${1:?usage: stackless-release-acceptance.sh ARCHIVE EVIDENCE_DIR BASELINE_JSON}
evidence_dir=${2:?usage: stackless-release-acceptance.sh ARCHIVE EVIDENCE_DIR BASELINE_JSON}
baseline=${3:?usage: stackless-release-acceptance.sh ARCHIVE EVIDENCE_DIR BASELINE_JSON}
archive=$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")
baseline=$(cd "$(dirname "$baseline")" && pwd)/$(basename "$baseline")
test -f "$archive"
test -f "$baseline"
git_status=$(git status --porcelain=v1 --untracked-files=all)
if [[ -n $git_status && ${BIRNAM_ACCEPT_DIRTY:-} != 1 ]]; then
  printf 'release acceptance requires a clean tree\n' >&2
  exit 1
fi
if [[ -e $evidence_dir ]]; then
  printf 'evidence destination already exists: %s\n' "$evidence_dir" >&2
  exit 2
fi
mkdir -p "$evidence_dir/logs" "$evidence_dir/longevity"
git_commit=$(git rev-parse HEAD)
archive_before=$(shasum -a 256 "$archive" | awk '{print $1}')
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-release-acceptance.XXXXXX")
trap 'rm -rf "$work"' EXIT
tar -C "$work" -xzf "$archive"
prefix=$(find "$work" -mindepth 1 -maxdepth 1 -type d -print -quit)
root_count=$(find "$work" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')
if [[ $root_count != 1 || -z $prefix ]]; then
  printf 'candidate archive must contain exactly one prefix directory\n' >&2
  exit 1
fi
test -x "$prefix/bin/birnam"
install_receipt="$prefix/share/birnam/rust-install.json"
test -f "$install_receipt"
ledger="$evidence_dir/gates.tsv"
: >"$ledger"

run_gate() {
  local name=$1
  shift
  local log="$evidence_dir/logs/${name}.log"
  local command
  printf -v command '%q ' "$@"
  set +e
  "$@" >"$log" 2>&1
  local status=$?
  set -e
  local digest
  digest=$(shasum -a 256 "$log" | awk '{print $1}')
  printf '%s\t%s\t%s\t%s\n' "$name" "$status" "$digest" "$command" >>"$ledger"
  if ((status != 0)); then
    printf 'release gate failed: %s (see %s)\n' "$name" "$log" >&2
    exit "$status"
  fi
}

validate_benchmark_pair() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    baseline = json.load(handle)["stamp"]
with open(sys.argv[2], encoding="utf-8") as handle:
    candidate = json.load(handle)["stamp"]
assert baseline["floor"] == "rust", baseline
assert candidate["floor"] == "rust", candidate
assert baseline["mode"] == "release", baseline
assert candidate["mode"] == "release", candidate
assert baseline["machine"] == candidate["machine"], (baseline, candidate)
assert baseline.get("quick") is False, baseline
assert candidate.get("quick") is False, candidate
assert candidate.get("dirty") is False, candidate
PY
}

run_gate archive_integrity tar -tzf "$archive"
run_gate install_layout env BIRNAM_PREFIX="$prefix" BIRNAM_DISTRIBUTION=production scripts/install-smoke.sh
run_gate exact_current_rejection scripts/exact-current-rejection-smoke.sh "$prefix"
run_gate image_resave_plateau scripts/image-resave-plateau-smoke.sh "$prefix"
run_gate stackless_image scripts/stackless-image-lifecycle-smoke.sh "$prefix"
run_gate portable_resumable scripts/portable-resumable-lifecycle-smoke.sh "$prefix"
run_gate portable_code_binding scripts/portable-code-binding-lifecycle-smoke.sh "$prefix"
run_gate portable_image_v8 scripts/portable-image-v8-smoke.sh "$prefix"
run_gate stackless_control scripts/stackless-control-smoke.sh "$prefix"
run_gate bounded_churn scripts/runtime-bounded-churn-smoke.sh "$prefix"

benchmark="$evidence_dir/benchmark.json"
run_gate benchmark env BENCH_PREFIX="$prefix" BENCH_FINAL_OUT="$benchmark" scripts/bench.sh --release --floor=rust
run_gate baseline_contract validate_benchmark_pair "$baseline" "$benchmark"
candidate_version=$(python3 - "$benchmark" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["stamp"]["version"])
PY
)
if [[ $candidate_version == 0.14.0 ]]; then
  run_gate performance_compare python3 scripts/v014-performance-acceptance.py "$baseline" "$benchmark"
elif [[ $candidate_version == 0.15.0 ]]; then
  run_gate performance_compare env BENCH_BIN="$prefix/bin/birnam" python3 scripts/v015-performance-acceptance.py "$baseline" "$benchmark"
else
  run_gate performance_compare env BENCH_BIN="$prefix/bin/birnam" scripts/bench-compare.sh "$baseline" "$benchmark"
fi
run_gate longevity scripts/stackless-longevity-smoke.sh "$prefix" "$evidence_dir/longevity"

archive_after=$(shasum -a 256 "$archive" | awk '{print $1}')
test "$archive_before" = "$archive_after"
python3 - "$archive" "$archive_before" "$baseline" "$benchmark" "$install_receipt" \
  "$ledger" "$evidence_dir" "$git_commit" "$([[ -z $git_status ]] && printf true || printf false)" <<'PY'
import hashlib
import json
import pathlib
import sys
import tarfile

archive, archive_digest, baseline, benchmark, install_path, ledger_path, evidence_dir, commit, clean = sys.argv[1:]
root = pathlib.Path(evidence_dir)

def digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()

with open(install_path, encoding="utf-8") as handle:
    install = json.load(handle)
assert install["schema"] == 3
assert install["admission_contract"]["image_version"] == 8
assert install["admission_contract"]["candidate_manifest_version"] == 4

gates = []
with open(ledger_path, encoding="utf-8") as handle:
    for line in handle:
        name, status, output_digest, command = line.rstrip("\n").split("\t", 3)
        gates.append({
            "name": name,
            "command": command.rstrip(),
            "exit_status": int(status),
            "complete_output_sha256": output_digest,
            "log": f"logs/{name}.log",
        })
assert gates and all(gate["exit_status"] == 0 for gate in gates)

members = {}
with tarfile.open(archive, "r:gz") as package:
    for member in package.getmembers():
        if member.isfile():
            stream = package.extractfile(member)
            assert stream is not None
            value = hashlib.sha256()
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                value.update(chunk)
            members[member.name] = value.hexdigest()

with open(root / "longevity/summary.json", encoding="utf-8") as handle:
    longevity = json.load(handle)
receipt = {
    "schema": 1,
    "scope": "birnam-stackless-production-acceptance-60h",
    "result": "passed",
    "promotable": clean == "true" and install["source_identity"]["git_clean"],
    "source": {"git_commit": commit, "git_clean": clean == "true"},
    "admission_contract": install["admission_contract"],
    "toolchain": install["toolchain"],
    "archive": {
        "path": pathlib.Path(archive).name,
        "sha256": archive_digest,
        "bytes": pathlib.Path(archive).stat().st_size,
        "members": members,
    },
    "install_receipt_sha256": digest(install_path),
    "source_manifest_sha256": install["source_manifest_sha256"],
    "staged_manifest_sha256": install["staged_manifest_sha256"],
    "baseline": {"path": pathlib.Path(baseline).name, "sha256": digest(baseline)},
    "benchmark": {"path": pathlib.Path(benchmark).name, "sha256": digest(benchmark)},
    "gates": gates,
    "longevity": longevity,
}
with open(root / "receipt.json", "w", encoding="utf-8") as handle:
    json.dump(receipt, handle, sort_keys=True, indent=2)
    handle.write("\n")
summary = [
    "# Birnam stackless production acceptance",
    "",
    f"- Result: {receipt['result']}",
    f"- Promotable: {str(receipt['promotable']).lower()}",
    f"- Commit: `{commit}`",
    f"- Archive SHA-256: `{archive_digest}`",
    f"- Gates: {len(gates)} passed",
    f"- Longevity: {longevity['epochs']} epochs passed",
    "",
    "This summary is derived from `receipt.json`; the receipt is authoritative.",
]
(root / "SUMMARY.md").write_text("\n".join(summary) + "\n", encoding="utf-8")
PY

printf 'stackless-release-acceptance: immutable archive passed; receipt %s/receipt.json\n' "$evidence_dir"
