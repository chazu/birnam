#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
runtime_lib="$(cd "$(dirname "$bin")/../lib" && pwd -P)"
tmp="$(mktemp -d /tmp/birnam-project-check.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/app/src/nested" "$tmp/app/native" "$tmp/app/assets" "$tmp/app/tests" "$tmp/dep/src"

cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
main = "main.birnam"
load = ["app"]
source_roots = ["src"]

[[module]]
entry = "src/app.birnam"
files = ["src/app-extra.birnam"]

[[wrapped]]
name = "foo"
build = [["cc", "-dynamiclib", "-fPIC", "native/foo.c", "-o", "{output}"]]

[dependencies]
dep = { path = "../dep" }

[test]
sources = ["tests/app-test.birnam", "tests/isolation-test.birnam"]
EOF

cat >"$tmp/app/main.birnam" <<'EOF'
// The entry source is not itself a project module.
[System println: "project-main"];
[System println: [[Foo version] printString]];
[System println: [[AppPart marker] printString]];
[System println: [[AppPart embedded] printString]];
[System println: [[Globals at: #ProjectArgs] printString]];
EOF

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#util #dep #foo]];
EOF

cat >"$tmp/app/src/app-extra.birnam" <<'EOF'
[Globals at: #AppPart put: [Object clone]];
[Reflect defineMethod: #marker on: AppPart as: [:self | "part-loaded"]];
[Reflect defineMethod: #embedded on: AppPart as: [:self | [System embedFile: "assets/message.txt"]]];
EOF

printf 'embedded-from-project-root' >"$tmp/app/assets/message.txt"

cat >"$tmp/app/src/foo.birnam" <<'EOF'
[Module named: #foo requires: #[]];
[Alien dlopen: "libfoo.dylib"];
[Globals at: #Foo put: [Object clone]];
[Alien wrap: "foo_version" signature: "i" as: #version on: Foo];
EOF

cat >"$tmp/app/native/foo.c" <<'EOF'
int foo_version(void) { return 7; }
EOF

cat >"$tmp/app/src/util.birnam" <<'EOF'
[Module named: #util requires: #[]];
EOF

cat >"$tmp/app/src/legacy.birnam" <<'EOF'
[Globals at: #LegacyFixture put: [Object clone]];
EOF

cat >"$tmp/app/tests/app-test.birnam" <<'EOF'
[Globals at: #LeakedProjectTestState put: true];
let suite := [TestSuite named: "Project App" module: #app];
[suite test: "compiled project modules are active" do: [:t |
  [t expect: [AppPart marker] equals: "part-loaded" describes: "project module loaded"]]];
EOF

cat >"$tmp/app/tests/isolation-test.birnam" <<'EOF'
let suite := [TestSuite named: "Project Isolation" module: #app];
[suite test: "test sources use fresh processes" do: [:t |
  [t expect: [Globals includes: #LeakedProjectTestState] equals: false describes: "prior test state absent"]]];
EOF

cat >"$tmp/dep/birnam.toml" <<'EOF'
name = "dep"
source_roots = ["src"]
EOF

cat >"$tmp/dep/src/dep.birnam" <<'EOF'
[Module named: #dep requires: #[#leaf]];
EOF

cat >"$tmp/dep/src/leaf.birnam" <<'EOF'
[Module named: #leaf requires: #[]];
EOF

output="$(BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app/src/nested")"
printf '%s\n' "$output"

grep -q "project: $tmp/app" <<<"$output"
grep -q "main: $tmp/app/main.birnam" <<<"$output"
order="$(grep '^  #' <<<"$output" | sed 's/^  #\([^ ]*\).*/\1/' | paste -sd ' ' -)"
test "$order" = "leaf dep foo util app"
grep -q "#app <- $tmp/app/src/app.birnam + $tmp/app/src/app-extra.birnam" <<<"$output"
grep -q "  $tmp/app/tests/app-test.birnam" <<<"$output"
grep -q "  $tmp/app/tests/isolation-test.birnam" <<<"$output"

json_check="$(BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" --format=json)"
grep -q '"schema":"birnam-check-v1"' <<<"$json_check"
grep -q '"ok":true' <<<"$json_check"
grep -q '"checkedModules":5' <<<"$json_check"
grep -q '"diagnostics":\[\]' <<<"$json_check"

build_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app/src/nested")"
printf '%s\n' "$build_output"
build_root="$(sed -n 's/^build: //p' <<<"$build_output")"
test -d "$build_root/lib"
test -f "$build_root/resolution.toml"
test -f "$build_root/artifacts.toml"
for module in leaf dep util foo app; do
    test -f "$build_root/lib/libbirnam_${module}.dylib"
    grep -q "name = \"$module\"" "$build_root/artifacts.toml"
done
grep -q "sources = \[\"$tmp/app/src/app.birnam\", \"$tmp/app/src/app-extra.birnam\"\]" "$build_root/artifacts.toml"
test -f "$build_root/lib/libfoo.dylib"
grep -q 'foo \[compiled\].*lib/libfoo.dylib' <<<"$build_output"
grep -q 'artifact_abi = "birnam-artifact-generation-v1"' "$build_root/artifacts.toml"
old_leaf_checksum="$(shasum -a 256 "$build_root/lib/libbirnam_leaf.dylib" | awk '{print $1}')"

test_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" test "$tmp/app")"
printf '%s\n' "$test_output"
grep -q '== Project App ==' <<<"$test_output"
grep -q 'ok   project module loaded' <<<"$test_output"
grep -q "ok   $tmp/app/tests/app-test.birnam" <<<"$test_output"
grep -q '== Project Isolation ==' <<<"$test_output"
grep -q 'ok   prior test state absent' <<<"$test_output"
grep -q "ok   $tmp/app/tests/isolation-test.birnam" <<<"$test_output"

cp "$tmp/app/tests/isolation-test.birnam" "$tmp/app/tests/isolation-test.birnam.valid"
cat >"$tmp/app/tests/isolation-test.birnam" <<'EOF'
let suite := [TestSuite named: "Project Failure" module: #app];
[suite test: "failure reaches the parent" do: [:t |
  [t expect: true equals: false describes: "intentional project test failure"]]];
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" test "$tmp/app" >"$tmp/test-failure.out" 2>&1; then
    echo "expected project test failure" >&2
    exit 1
fi
grep -q 'FAIL intentional project test failure' "$tmp/test-failure.out"
grep -q "FAIL $tmp/app/tests/isolation-test.birnam" "$tmp/test-failure.out"
grep -q '2 sources, 1 failure' "$tmp/test-failure.out"
mv "$tmp/app/tests/isolation-test.birnam.valid" "$tmp/app/tests/isolation-test.birnam"

release_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app" --profile=release)"
printf '%s\n' "$release_output"
release_root="$(sed -n 's/^build: //p' <<<"$release_output")"
test "$release_root" = "$tmp/app/.birnam/build/aarch64-macos/release"
test -f "$release_root/artifacts.toml"
grep -q 'profile = "release"' "$release_root/artifacts.toml"
test -f "$build_root/artifacts.toml"
release_run="$(BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/app" --profile release)"
grep -q '^project-main$' <<<"$release_run"

cp "$tmp/app/src/util.birnam" "$tmp/app/src/util.birnam.valid"
cat >"$tmp/app/src/util.birnam" <<'EOF'
[Module named: #util requires: #[]];
let assetPath := "assets/message.txt";
[System embedFile: assetPath];
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" --format=json >"$tmp/compile-check.json"; then
    echo "expected compile-check failure" >&2
    exit 1
fi
grep -q '"schema":"birnam-check-v1"' "$tmp/compile-check.json"
grep -q '"kind":"compileFailed"' "$tmp/compile-check.json"
grep -q "\"path\":\"$tmp/app/src/util.birnam\"" "$tmp/compile-check.json"
grep -q '"line":3' "$tmp/compile-check.json"
mv "$tmp/app/src/util.birnam.valid" "$tmp/app/src/util.birnam"

install_lib="$tmp/install/lib"
mkdir -p "$install_lib"
cp "$runtime_lib"/*.dylib "$install_lib/"
install_output="$(BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$install_lib" "$bin" install "$tmp/app")"
printf '%s\n' "$install_output"
grep -q "installed: $install_lib" <<<"$install_output"
for module in leaf dep util foo app; do
    test -f "$install_lib/libbirnam_${module}.dylib"
done
test -f "$install_lib/libfoo.dylib"
test ! -e "$install_lib/artifacts.toml"
test ! -e "$install_lib/resolution.toml"
test ! -e "$install_lib/birnam.toml"

# The installed project root is executable without its source tree or build
# receipts. Its embedded descriptor supplies the identity and complete sibling
# module/native closure.
(cd /tmp && BIRNAM_LIB_DIR="$install_lib" "$bin" "$install_lib/libbirnam_app.dylib")

bare_output="$(cd "$tmp/app" && printf ':quit\n' | BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$install_lib" "$bin")"
printf '%s\n' "$bare_output"
test "$(grep -c 'project-main' <<<"$bare_output")" = 1
grep -q '^7$' <<<"$bare_output"
grep -q 'part-loaded' <<<"$bare_output"
grep -q 'embedded-from-project-root' <<<"$bare_output"
grep -q 'interactive image' <<<"$bare_output"

mv "$install_lib/libfoo.dylib" "$install_lib/libfoo.dylib.unavailable"
# Remove Codegen too: the siphon's ID-only preflight must still report the
# transitive wrapper requirement, proving it walked the closure before loading
# the activation stack.
mv "$install_lib/libbirnam_id_codegen.dylib" "$install_lib/libbirnam_id_codegen.dylib.unavailable"
(cd /tmp && BIRNAM_LIB_DIR="$install_lib" "$bin" "$install_lib/libbirnam_app.dylib") >"$tmp/missing-native-artifact.out" 2>&1 || true
if ! grep -q 'nativeRequirementUnavailable' "$tmp/missing-native-artifact.out"; then
    cat "$tmp/missing-native-artifact.out" >&2
    echo "project-check-smoke: ID-only preflight missed the native requirement" >&2
    exit 1
fi
mv "$install_lib/libbirnam_id_codegen.dylib.unavailable" "$install_lib/libbirnam_id_codegen.dylib"

# A source-project run selects its validated local publication. Remove that
# publication's native sibling as well as the installed fallback to prove the
# same descriptor preflight happens before main.
mv "$build_root/lib/libfoo.dylib" "$build_root/lib/libfoo.dylib.unavailable"
(cd "$tmp/app" && printf ':quit\n' | BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$install_lib" "$bin") >"$tmp/missing-native.out" 2>&1 || true
if ! grep -q 'nativeRequirementUnavailable' "$tmp/missing-native.out"; then
    cat "$tmp/missing-native.out" >&2
    echo "project-check-smoke: missing native diagnostic was not preserved" >&2
    exit 1
fi
if grep -q '^project-main$' "$tmp/missing-native.out"; then
    echo "project-check-smoke: project main ran despite an unavailable native requirement" >&2
    exit 1
fi
mv "$build_root/lib/libfoo.dylib.unavailable" "$build_root/lib/libfoo.dylib"
mv "$install_lib/libfoo.dylib.unavailable" "$install_lib/libfoo.dylib"

reused_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$reused_output"
for module in leaf dep util foo app; do
    grep -q "#$module \[reused\]" <<<"$reused_output"
done

printf 'changed-embedded-bytes' >"$tmp/app/assets/message.txt"
asset_changed_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$asset_changed_output"
grep -q '#app \[compiled\]' <<<"$asset_changed_output"
for module in leaf dep util foo; do
    grep -q "#$module \[reused\]" <<<"$asset_changed_output"
done

printf '// changing a source part must invalidate its composed module\n' >>"$tmp/app/src/app-extra.birnam"
part_changed_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$part_changed_output"
grep -q '#app \[compiled\]' <<<"$part_changed_output"
for module in leaf dep util foo; do
    grep -q "#$module \[reused\]" <<<"$part_changed_output"
done

cat >"$tmp/dep/src/leaf.birnam" <<'EOF'
[Module named: #leaf requires: #[]];
1;
EOF
changed_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$changed_output"
for module in leaf dep app; do
    grep -q "#$module \[compiled\]" <<<"$changed_output"
done
grep -q '#util \[reused\]' <<<"$changed_output"

printf '\nartifact tamper\n' >>"$build_root/lib/libbirnam_leaf.dylib"
tampered_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$tampered_output"
for module in leaf dep app; do
    grep -q "#$module \[compiled\]" <<<"$tampered_output"
done
grep -q '#util \[reused\]' <<<"$tampered_output"
old_leaf_checksum="$(shasum -a 256 "$build_root/lib/libbirnam_leaf.dylib" | awk '{print $1}')"

run_output="$(cd /tmp && BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/app" -- first second)"
printf '%s\n' "$run_output"
test "$(grep -c 'project-main' <<<"$run_output")" = 1
grep -q '^7$' <<<"$run_output"
grep -q 'first' <<<"$run_output"
grep -q 'second' <<<"$run_output"

# An empty URI inside a project application means the already-resolved active
# project, not the caller's process cwd (/tmp in this probe).
project_probe='[System println: [[Project snapshotForUri: ""] printString]]'
invalid_project_probe='[System println: [[Project snapshotForUri: 7] printString]]'
tty_output="$(cd /tmp && printf '%s\n%s\n:quit\n' "$project_probe" "$invalid_project_probe" | BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/app" --repl=tty -- first second)"
printf '%s\n' "$tty_output"
test "$(grep -c 'project-main' <<<"$tty_output")" = 1
test "$(grep -c 'interactive image' <<<"$tty_output")" = 1
grep -q "$tmp/app" <<<"$tty_output"
grep -q 'profile -> debug' <<<"$tty_output"
grep -q "buildRoot -> $build_root" <<<"$tty_output"
grep -q 'projectUriInvalid' <<<"$tty_output"

cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
main = "main.birnam"
load = ["app"]
source_roots = ["src"]

[[module]]
entry = "src/app.birnam"
files = ["src/app-extra.birnam"]

[[wrapped]]
name = "foo"
build = [["cc", "-dynamiclib", "-fPIC", "native/foo.c", "-o", "{output}"]]

[dependencies]
dep = { path = "../dep" }

[run]
repl = "socket"
socket_bind = "127.0.0.1"
socket_port = 45678
EOF
socket_output="$tmp/socket.out"
BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/app" --build >"$socket_output" 2>&1 &
socket_pid=$!
for _ in $(seq 1 50); do
    if grep -q 'socket REPL listening on 127.0.0.1:45678' "$socket_output" 2>/dev/null; then
        break
    fi
    sleep 0.1
done
grep -q 'socket REPL listening on 127.0.0.1:45678' "$socket_output"
test "$(grep -c 'project-main' "$socket_output")" = 1
grep -q 'part-loaded' "$socket_output"
printf ':quit\n' | nc -w 1 127.0.0.1 45678 >"$tmp/socket-client.out" || true
grep -q 'socket REPL' "$tmp/socket-client.out"
kill "$socket_pid" 2>/dev/null || true
wait "$socket_pid" 2>/dev/null || true

cp "$tmp/app/src/app-extra.birnam" "$tmp/app/src/app-extra.birnam.valid"
cat >"$tmp/app/src/app-extra.birnam" <<'EOF'
[Module named: #unexpectedPart requires: #[]];
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" >"$tmp/part-declaration.out" 2>&1; then
    echo "expected module part declaration failure" >&2
    exit 1
fi
grep -q "modulePartDeclaration" "$tmp/part-declaration.out"
mv "$tmp/app/src/app-extra.birnam.valid" "$tmp/app/src/app-extra.birnam"

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#util #dep]];
this is not valid birnam;
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app" >"$tmp/build-failure.out" 2>&1; then
    echo "expected build failure" >&2
    exit 1
fi
grep -q "sourceMalformed" "$tmp/build-failure.out"
new_leaf_checksum="$(shasum -a 256 "$build_root/lib/libbirnam_leaf.dylib" | awk '{print $1}')"
test "$old_leaf_checksum" = "$new_leaf_checksum"
test -f "$build_root/artifacts.toml"

help_output="$(BIRNAM_FORCE_SOURCE=1 BIRNAM_ID_DYLIB=/tmp/does-not-exist "$bin" check --help)"
grep -q "Usage: birnam check \[directory\]" <<<"$help_output"
grep -q "read-only" <<<"$help_output"

if BIRNAM_FORCE_SOURCE=1 BIRNAM_ID_DYLIB=/tmp/does-not-exist "$bin" check --unknown >"$tmp/usage-error.out" 2>&1; then
    echo "expected check usage failure" >&2
    exit 1
fi
grep -q "birnam check: unknown option '--unknown'" "$tmp/usage-error.out"
grep -q "Usage: birnam check \[directory\]" "$tmp/usage-error.out"
if grep -q "stack trace" "$tmp/usage-error.out"; then
    echo "check usage error unexpectedly printed a stack trace" >&2
    exit 1
fi

if (cd /tmp && BIRNAM_FORCE_SOURCE=1 "$bin" check) >"$tmp/no-project.out" 2>&1; then
    echo "expected check without a project to fail" >&2
    exit 1
fi
grep -q "birnam check: projectNotFound:" "$tmp/no-project.out"
grep -q "usage: birnam check \[directory\]" "$tmp/no-project.out"
if grep -q "stack trace" "$tmp/no-project.out"; then
    echo "check failure unexpectedly printed a stack trace" >&2
    exit 1
fi

build_help_output="$(BIRNAM_FORCE_SOURCE=1 BIRNAM_ID_DYLIB=/tmp/does-not-exist "$bin" build --help)"
grep -q "Usage: birnam build \[directory\]" <<<"$build_help_output"
grep -q "staged" <<<"$build_help_output"

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#missing]];
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" >"$tmp/missing.out" 2>&1; then
    echo "expected missing dependency failure" >&2
    exit 1
fi
grep -q "moduleNotFound" "$tmp/missing.out"
grep -q "#app -> #missing" "$tmp/missing.out"

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#dep]];
EOF
cat >"$tmp/dep/src/dep.birnam" <<'EOF'
[Module named: #wrong requires: #[]];
EOF
rm "$tmp/dep/src/leaf.birnam"
if BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" >"$tmp/mismatch.out" 2>&1; then
    echo "expected module identity failure" >&2
    exit 1
fi
grep -q "moduleNameMismatch" "$tmp/mismatch.out"

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#util]];
EOF
cat >"$tmp/app/src/util.birnam" <<'EOF'
[Module named: #util requires: #[#app]];
EOF
if BIRNAM_FORCE_SOURCE=1 "$bin" check "$tmp/app" >"$tmp/cycle.out" 2>&1; then
    echo "expected dependency cycle failure" >&2
    exit 1
fi
grep -q "dependencyCycle" "$tmp/cycle.out"
grep -q "#app -> #util -> #app" "$tmp/cycle.out"

echo "project-check smoke: ok"
