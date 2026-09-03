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

# A wrapped native artifact declared two path-dependency levels below the root
# must be built in its declaring project, published into the root build's lib/
# directory, and available before a root wrapper recipe runs. This also proves
# that a validated, already-published build can load the transitive wrapper.
mkdir -p \
  "$tmp/transitive/root/native" "$tmp/transitive/root/src" \
  "$tmp/transitive/middle/src" \
  "$tmp/transitive/native/src" \
  "$tmp/transitive/native/native"
cat >"$tmp/transitive/root/birnam.toml" <<'EOF'
name = "transitive-root"
main = "main.birnam"
load = ["middle"]

[dependencies]
middle = { path = "../middle" }

[[wrapped]]
name = "root_marker"
build = [
  ["/bin/test", "-f", "{lib}/libtransitive_fixture.dylib"],
  ["cc", "-dynamiclib", "native/root_marker.c", "-o", "{output}"],
]
EOF
cat >"$tmp/transitive/root/main.birnam" <<'EOF'
[System println: [[TransitiveFixture answer] printString]];
EOF
cat >"$tmp/transitive/root/native/root_marker.c" <<'EOF'
int root_marker(void) { return 1; }
EOF
cat >"$tmp/transitive/middle/birnam.toml" <<'EOF'
name = "middle"
source_roots = ["src"]

[dependencies]
transitive_fixture = { path = "../native" }
EOF
cat >"$tmp/transitive/middle/src/middle.birnam" <<'EOF'
[Module named: #middle requires: #[#transitive_fixture]];
EOF
cat >"$tmp/transitive/native/birnam.toml" <<'EOF'
name = "transitive_fixture"
source_roots = ["src"]

[[wrapped]]
name = "transitive_fixture"
build = [["cc", "-dynamiclib", "native/fixture.c", "-o", "{output}"]]
EOF
cat >"$tmp/transitive/native/src/transitive_fixture.birnam" <<'EOF'
[Module named: #transitive_fixture requires: #[]];
[Alien dlopen: "libtransitive_fixture.dylib"];
[Globals at: #TransitiveFixture put: [Object clone]];
[Alien wrap: "transitive_answer" signature: "i" as: #answer on: TransitiveFixture]
EOF
cat >"$tmp/transitive/native/native/fixture.c" <<'EOF'
int transitive_answer(void) { return 42; }
EOF

transitive_check="$(run_from_clean_cwd check "$tmp/transitive/root")"
grep -Fq 'transitive_fixture -> lib/libtransitive_fixture.dylib' <<<"$transitive_check"
grep -Fq 'root_marker -> lib/libroot_marker.dylib' <<<"$transitive_check"
run_from_clean_cwd build "$tmp/transitive/root" >/dev/null
test -f "$tmp/transitive/root/.birnam/build/"*/debug/lib/libtransitive_fixture.dylib
test -f "$tmp/transitive/root/.birnam/build/"*/debug/lib/libroot_marker.dylib
transitive_run="$(run_from_clean_cwd run "$tmp/transitive/root" --no-repl)"
grep -Fxq '42' <<<"$transitive_run"
transitive_install_lib="$tmp/transitive-install/lib"
mkdir -p "$transitive_install_lib"
cp "$runtime_lib"/*.dylib "$transitive_install_lib/"
transitive_install="$(cd /tmp && BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$transitive_install_lib" "$bin" install "$tmp/transitive/root")"
grep -Fq " into $transitive_install_lib" <<<"$transitive_install"
test -f "$transitive_install_lib/libbirnam_middle.dylib"
test -f "$transitive_install_lib/libbirnam_transitive_fixture.dylib"
test -f "$transitive_install_lib/libtransitive_fixture.dylib"
test -f "$transitive_install_lib/libroot_marker.dylib"

echo "rust-project-check: relocated check human/JSON, build, run, install, test, and transitive wrapped dependency passed"
