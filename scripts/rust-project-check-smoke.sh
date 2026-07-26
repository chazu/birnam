#!/usr/bin/env bash
# Rust-host project command contract. The Zig-host project smoke deliberately
# checks its richer human build report; Rust owns a summary/JSON report while
# the project policy, compilation, run, and test boundaries remain identical.
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
tmp="$(mktemp -d /tmp/birnam-rust-project-check.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/project/src" "$tmp/project/tests"
cat >"$tmp/project/birnam.toml" <<'EOF'
name = "rust-host-project"
main = "main.birnam"
load = ["app"]
source_roots = ["src"]

[test]
sources = ["tests/app-test.birnam"]
EOF
cat >"$tmp/project/src/app.birnam" <<'EOF'
[Module named: #app requires: #[]];
EOF
cat >"$tmp/project/main.birnam" <<'EOF'
[System println: "rust-host-project-ok"];
EOF
cat >"$tmp/project/tests/app-test.birnam" <<'EOF'
let suite := [TestSuite named: "Rust host project" module: #app];
[suite test: "module active" do: [:t |
  [t expect: [Module isRegistered: #app] equals: true describes: "module active"]]];
EOF

human="$(BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/project")"
grep -Fq 'checked 1 module; no compile diagnostics' <<<"$human"

json="$(BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/project" --format=json)"
grep -Fq '"schema":"birnam-check-v1"' <<<"$json"
grep -Fq '"ok":true' <<<"$json"
grep -Fq '"checkedModules":1' <<<"$json"

build="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/project")"
grep -Eq 'built 1 module(s)? at ' <<<"$build"

run="$(BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/project" --no-repl)"
grep -Fxq 'rust-host-project-ok' <<<"$run"

test_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" test "$tmp/project")"
test -n "$test_output"

echo "rust-project-check: check human/JSON, build, run, and test passed"
