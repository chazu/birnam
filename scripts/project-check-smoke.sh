#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
runtime_lib="$(cd "$(dirname "$bin")/../lib" && pwd -P)"
tmp="$(mktemp -d /tmp/birnam-project-check.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/app/src/nested" "$tmp/app/native" "$tmp/dep/src"

cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
main = "main.birnam"
load = ["app"]
source_roots = ["src"]

[[wrapped]]
name = "foo"
build = [["cc", "-dynamiclib", "-fPIC", "native/foo.c", "-o", "{output}"]]

[dependencies]
dep = { path = "../dep" }
EOF

cat >"$tmp/app/main.birnam" <<'EOF'
// The entry source is not itself a project module.
[System println: "project-main"];
[System println: [[Foo version] printString]];
[System println: [[Globals at: #ProjectArgs] printString]];
EOF

cat >"$tmp/app/src/app.birnam" <<'EOF'
[Module named: #app requires: #[#util #dep #foo]];
EOF

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
test -f "$build_root/lib/libfoo.dylib"
grep -q 'foo \[compiled\].*lib/libfoo.dylib' <<<"$build_output"
grep -q 'artifact_abi = "birnam-artifact-generation-v1"' "$build_root/artifacts.toml"
old_leaf_checksum="$(shasum -a 256 "$build_root/lib/libbirnam_leaf.dylib" | awk '{print $1}')"

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

bare_output="$(cd "$tmp/app" && printf ':quit\n' | BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$install_lib" "$bin")"
printf '%s\n' "$bare_output"
test "$(grep -c 'project-main' <<<"$bare_output")" = 1
grep -q '^7$' <<<"$bare_output"
grep -q 'interactive image' <<<"$bare_output"

reused_output="$(BIRNAM_FORCE_SOURCE=1 "$bin" build "$tmp/app")"
printf '%s\n' "$reused_output"
for module in leaf dep util foo app; do
    grep -q "#$module \[reused\]" <<<"$reused_output"
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

tty_output="$(cd /tmp && printf ':quit\n' | BIRNAM_FORCE_SOURCE=1 "$bin" run "$tmp/app" --repl=tty -- first second)"
printf '%s\n' "$tty_output"
test "$(grep -c 'project-main' <<<"$tty_output")" = 1
test "$(grep -c 'interactive image' <<<"$tty_output")" = 1

cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
main = "main.birnam"
load = ["app"]
source_roots = ["src"]

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
printf ':quit\n' | nc -w 1 127.0.0.1 45678 >"$tmp/socket-client.out" || true
grep -q 'socket REPL' "$tmp/socket-client.out"
kill "$socket_pid" 2>/dev/null || true
wait "$socket_pid" 2>/dev/null || true

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
