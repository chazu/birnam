#!/usr/bin/env bash
# Rust-host project command contract.
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
bin="$(cd "$(dirname "$bin")" && pwd -P)/$(basename "$bin")"
runtime_lib="$(cd "$(dirname "$bin")/../lib" && pwd -P)"
tmp="$(mktemp -d /tmp/birnam-rust-project-check.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

# The Rust host must observe an installed prefix from the executable location,
# not from the caller's cwd or inherited resource overrides.
unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB BIRNAM_RUST_WORKSPACE_DIR
run_from_clean_cwd() {
  (cd /tmp && BIRNAM_FORCE_SOURCE=1 "$bin" "$@")
}

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

human="$(run_from_clean_cwd check "$tmp/project")"
grep -Fq 'checked 1 module; no compile diagnostics' <<<"$human"

json="$(run_from_clean_cwd check "$tmp/project" --format=json)"
grep -Fq '"schema":"birnam-check-v1"' <<<"$json"
grep -Fq '"ok":true' <<<"$json"
grep -Fq '"checkedModules":1' <<<"$json"

build="$(run_from_clean_cwd build "$tmp/project")"
grep -Eq 'built 1 module(s)? at ' <<<"$build"

run="$(run_from_clean_cwd run "$tmp/project" --no-repl)"
grep -Fxq 'rust-host-project-ok' <<<"$run"

# Install from an unrelated cwd into a private runtime directory seeded with
# the executable's shipped support libraries. This proves the install command
# itself, rather than only the build/run/test paths, and leaves the prefix
# under test untouched.
install_lib="$tmp/install/lib"
mkdir -p "$install_lib"
cp "$runtime_lib"/*.dylib "$install_lib/"
install_output="$(cd /tmp && BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$install_lib" "$bin" install --build "$tmp/project")"
grep -Fq "installed " <<<"$install_output"
grep -Fq " into $install_lib" <<<"$install_output"
test -f "$install_lib/libbirnam_app.dylib"

test_output="$(run_from_clean_cwd test "$tmp/project")"
test -n "$test_output"

echo "rust-project-check: relocated check human/JSON, build, run, install, and test passed"
