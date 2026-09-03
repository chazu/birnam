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

# Git dependencies are immutable full-revision origins. Fetch two local Git
# repositories recursively, then prove ordinary resolution, wrapped artifact
# propagation, and execution all consume the exact cached closure offline.
mkdir -p "$tmp/git/native/src" "$tmp/git/native/native"
cat >"$tmp/git/native/birnam.toml" <<'EOF'
name = "git-native"
source_roots = ["src"]

[[wrapped]]
name = "git_native"
build = [["cc", "-dynamiclib", "native/git_native.c", "-o", "{output}"]]
EOF
cat >"$tmp/git/native/src/git_native.birnam" <<'EOF'
[Module named: #git_native requires: #[]];
[Alien dlopen: "libgit_native.dylib"];
[Globals at: #GitNative put: [Object clone]];
[Alien wrap: "git_native_answer" signature: "i" as: #answer on: GitNative]
EOF
cat >"$tmp/git/native/native/git_native.c" <<'EOF'
int git_native_answer(void) { return 84; }
EOF
git -C "$tmp/git/native" init --quiet
git -C "$tmp/git/native" add birnam.toml src native
git -C "$tmp/git/native" -c user.name=Birnam -c user.email=birnam@example.invalid commit --quiet -m fixture
native_revision="$(git -C "$tmp/git/native" rev-parse HEAD)"

mkdir -p "$tmp/git/middle/src"
cat >"$tmp/git/middle/birnam.toml" <<EOF
name = "git-middle"
source_roots = ["src"]

[dependencies]
git_native = { git = "$tmp/git/native", rev = "$native_revision" }
EOF
cat >"$tmp/git/middle/src/git_middle.birnam" <<'EOF'
[Module named: #git_middle requires: #[#git_native]];
EOF
git -C "$tmp/git/middle" init --quiet
git -C "$tmp/git/middle" add birnam.toml src
git -C "$tmp/git/middle" -c user.name=Birnam -c user.email=birnam@example.invalid commit --quiet -m fixture
middle_revision="$(git -C "$tmp/git/middle" rev-parse HEAD)"

mkdir -p "$tmp/git/root/src"
cat >"$tmp/git/root/birnam.toml" <<EOF
name = "git-root"
main = "main.birnam"
load = ["git_middle"]

[dependencies]
git_middle = { git = "$tmp/git/middle", rev = "$middle_revision" }
EOF
cat >"$tmp/git/root/main.birnam" <<'EOF'
[System println: [[GitNative answer] printString]];
EOF

if run_from_clean_cwd check "$tmp/git/root" >"$tmp/git/missing.out" 2>&1; then
  echo "check unexpectedly fetched a missing Git dependency" >&2
  exit 1
fi
grep -Fq 'GitCacheMissing' "$tmp/git/missing.out"
test ! -e "$tmp/git/root/.birnam"

git_fetch="$(run_from_clean_cwd fetch "$tmp/git/root")"
grep -Fq 'fetched 2 Git dependencies; reused 0 cached checkouts' <<<"$git_fetch"
git_refetch="$(run_from_clean_cwd fetch "$tmp/git/root")"
grep -Fq 'fetched 0 Git dependencies; reused 2 cached checkouts' <<<"$git_refetch"
git_check="$(run_from_clean_cwd check "$tmp/git/root")"
grep -Fq 'git_native -> lib/libgit_native.dylib' <<<"$git_check"
run_from_clean_cwd build "$tmp/git/root" >/dev/null
git_run="$(run_from_clean_cwd run "$tmp/git/root" --no-repl)"
grep -Fxq '84' <<<"$git_run"

cached_native="$(find "$tmp/git/root/.birnam/dependencies/git" -path '*/src/git_native.birnam' -print -quit)"
test -n "$cached_native"
printf '\n// local mutation\n' >>"$cached_native"
if run_from_clean_cwd check "$tmp/git/root" >"$tmp/git/dirty.out" 2>&1; then
  echo "check accepted a modified Git dependency checkout" >&2
  exit 1
fi
grep -Fq 'checkout has tracked modifications' "$tmp/git/dirty.out"

echo "rust-project-check: relocated check human/JSON, build, run, install, test, path dependencies, and pinned Git dependencies passed"
