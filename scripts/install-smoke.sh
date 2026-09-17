#!/usr/bin/env bash
# Verify the installed tree, isolated from checkout-relative module fallbacks.
set -euo pipefail
cd "$(dirname "$0")/.."

prefix=${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}
distribution=${BIRNAM_DISTRIBUTION:-production}
case "$distribution" in
  production|test) ;;
  *) echo "install-smoke: BIRNAM_DISTRIBUTION must be production or test" >&2; exit 2 ;;
esac
tmp=$(mktemp -d)
tmp=$(cd "$tmp" && pwd -P)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/birnam"
cp -R "$prefix/bin" "$prefix/lib" "$prefix/share" "$tmp/birnam/"

test ! -e "$tmp/birnam/share/birnam/runtime.toml"

# The IdApi JSON is repository-side compatibility evidence, not a runtime
# manifest or distributed artifact.
if find "$tmp/birnam" -type f -name 'idapi-v1.json' -print -quit | grep -q .; then
  echo "install-smoke: build/test-only IdApi manifest evidence was installed" >&2
  exit 1
fi

# Production Birnam source lives in compiled-unit metadata, not a parallel
# installed source tree.
test ! -e "$tmp/birnam/share/birnam/modules"
if find "$tmp/birnam/share/birnam" -type f -name '*.birnam' -print -quit | grep -q .; then
  echo "install-smoke: loose Birnam module source was installed" >&2
  exit 1
fi

find rust -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' \) -print | sort >"$tmp/expected-rust-workspace"
find "$tmp/birnam/share/birnam/src/rust" -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' \) -print |
  sed "s#^$tmp/birnam/share/birnam/src/##" | sort >"$tmp/installed-rust-workspace"
diff -u "$tmp/expected-rust-workspace" "$tmp/installed-rust-workspace"
cmp Cargo.toml "$tmp/birnam/share/birnam/src/Cargo.toml"
cmp Cargo.lock "$tmp/birnam/share/birnam/src/Cargo.lock"
test ! -e "$tmp/birnam/share/birnam/src/rust/Cargo.toml"

mkdir -p "$tmp/cargo-home"
CARGO_HOME="$tmp/cargo-home" cargo metadata \
  --offline \
  --no-deps \
  --format-version=1 \
  --manifest-path "$tmp/birnam/share/birnam/src/Cargo.toml" >"$tmp/cargo-metadata.json"
test "$(jq -r '.workspace_root' "$tmp/cargo-metadata.json")" = "$tmp/birnam/share/birnam/src"
test "$(jq -r '[.packages[].name] | sort | join(" ")' "$tmp/cargo-metadata.json")" = "birnam-abi birnam-alien birnam-artifact birnam-artifact-codec birnam-codegen birnam-codegen-runtime birnam-collection birnam-error birnam-host birnam-id birnam-image birnam-isolate birnam-lang birnam-peg birnam-portable-code birnam-prelude birnam-relational birnam-store birnam-tcc"

find docs/guide -type f -name '*.md' -print | sed 's#^docs/guide/##' | sort >"$tmp/expected-guide"
find "$tmp/birnam/share/birnam/docs/guide" -type f -name '*.md' -print | sed "s#^$tmp/birnam/share/birnam/docs/guide/##" | sort >"$tmp/installed-guide"
diff -u "$tmp/expected-guide" "$tmp/installed-guide"

required_dylibs=(
  libbirnam_id.dylib libbirnam_store.dylib libbirnam_id_artifact.dylib
  libbirnam_id_peg.dylib libbirnam_id_lang.dylib libbirnam_id_tcc.dylib
  libbirnam_id_codegen.dylib libbirnam_id_prelude.dylib
  libbirnam_id_collection.dylib libbirnam_id_error.dylib
  libbirnam_portable_code.dylib
  libbirnam_id_isolate.dylib libbirnam_id_relational.dylib
  libbirnam_id_alien.dylib libbirnam_id_image.dylib
  libbirnam_modules.dylib libbirnam_stdlib.dylib libbirnam_compiledUnit.dylib
  libbirnam_toml.dylib libbirnam_config.dylib libbirnam_appConfig.dylib
  libbirnam_projectPolicy.dylib libbirnam_isolate.dylib libbirnam_html.dylib
  libbirnam_file.dylib libbirnam_json.dylib libbirnam_socket.dylib
  libbirnam_time.dylib libbirnam_doc.dylib libbirnam_nativedoc.dylib
  libbirnam_lsp.dylib libbirnam_repl.dylib libbirnam_cli.dylib
  libbirnam_http.dylib libbirnam_log.dylib libbirnam_markdown.dylib
  libbirnam_os.dylib libbirnam_process.dylib libbirnam_random.dylib
  libbirnam_test.dylib libbirnam_compiledUnitDocs.dylib libbirnam_concurrencyDocs.dylib
)
for dylib in "${required_dylibs[@]}"; do
  test -f "$tmp/birnam/lib/$dylib"
done
mv "$tmp/birnam" "$tmp/relocated"

bin="$tmp/relocated/bin/birnam"
work="$tmp/work"
mkdir -p "$work"
cd "$work"
unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB

"$bin" doc time nowMillis >"$tmp/doc-time.out"
grep -q 'nowMillis' "$tmp/doc-time.out"
"$bin" doc image saveImage: >"$tmp/doc-image.out"
grep -q 'saveImage:' "$tmp/doc-image.out"
"$bin" docgen reference.html >/dev/null
grep -q 'id="guide-01-language"' reference.html
grep -q 'id="guide-04-images"' reference.html
grep -q 'System saveImage:' reference.html
grep -q 'Blocking process isolates' reference.html
grep -q 'id="html"' reference.html
grep -q 'tag:body:' reference.html
grep -q 'id="rust-abi"' reference.html
grep -q 'id="rust-lib"' reference.html
grep -q '>Native Rust<' reference.html
grep -q '>Rust floor<' reference.html
grep -q 'Append-only C ABI table returned by' reference.html
"$bin" --lsp </dev/null >/dev/null
printf ':quit\n' | env BIRNAM_ID_FLOOR_IDENTITY=rust-id-v1 "$bin" >/dev/null
# A compiled unit is itself a runnable root. Its identity and requirements are
# recovered from inert artifact bytes; no source manifest or build receipt is
# involved.
"$bin" "$tmp/relocated/lib/libbirnam_time.dylib"

mkdir -p "$tmp/project/src" "$tmp/project/tests"
cat >"$tmp/project/birnam.toml" <<'EOF'
name = "installed-smoke"
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
[System println: "installed-project-ok"];
EOF
cat >"$tmp/project/tests/app-test.birnam" <<'EOF'
let suite := [TestSuite named: "Installed Project" module: #app];
[suite test: "relocated test runtime loads" do: [:t |
  [t expect: [Module isRegistered: #app] equals: true describes: "project module active"]]];
EOF
"$bin" check "$tmp/project" >/dev/null
"$bin" build "$tmp/project" >/dev/null
"$bin" run "$tmp/project" >"$tmp/project-run.out"
grep -q '^installed-project-ok$' "$tmp/project-run.out"
"$bin" test "$tmp/project" >"$tmp/project-test.out"
grep -q 'ok   project module active' "$tmp/project-test.out"

image="$tmp/relocated.image"
printf 'let RelocatedImageValue := 41\nlet RelocatedWorker := [Object clone]\n[Reflect defineMethod: #handle: on: RelocatedWorker as: [:self :value | value]]\n[System saveImage: "%s"]\n:quit\n' "$image" | "$bin" --repl >/dev/null
test -s "$image"
printf '[RelocatedImageValue + 1]\n:quit\n' | "$bin" "$image" >"$tmp/image-run.out"
grep -q 'birnam> 42' "$tmp/image-run.out"
printf '[RelocatedImageValue + 1]\n:quit\n' | "$bin" --rebuild "$image" >"$tmp/image-rebuild.out"
grep -q 'birnam> 42' "$tmp/image-rebuild.out"

# Process-local execution capabilities fail during Store/image preflight, before
# code capture or publication. Each probe needs a fresh process because Store
# history deliberately retains retracted facts.
not_imageable_probe() {
  local label=$1
  local expression=$2
  local expected=$3
  local script="$tmp/not-imageable-$label.birnam"
  local rejected_image="$tmp/not-imageable-$label.image"
  cat >"$script" <<EOF
let value := $expression;
[db assert: #notImageableProbe attr: #value value: value];
let result := [System saveImage: "$rejected_image"];
[System println: [[result kind] printString]]
EOF
  test "$("$bin" "$script")" = "#$expected"
  test ! -e "$rejected_image"
}

not_imageable_probe machine '[Machine new]' machineNotImageable
not_imageable_probe continue-action '[ContinueAction clone]' stepActionNotImageable
not_imageable_probe call-action '[CallAction clone]' stepActionNotImageable
not_imageable_probe tail-call-action '[TailCallAction clone]' stepActionNotImageable
not_imageable_probe return-action '[ReturnAction clone]' stepActionNotImageable
not_imageable_probe frame-view '[FrameView clone]' frameViewNotImageable
not_imageable_probe discard-receipt '[DiscardReceipt clone]' discardReceiptNotImageable
not_imageable_probe foreign-address '[Alien alloc: 8]' foreignAddressNotImageable

cat >"$tmp/foreign-address-contract.birnam" <<'EOF'
let owned := [Alien alloc: 8];
[owned free];
let twice := [owned free];
[System println: [[twice kind] printString]];
let unit := [Tcc new];
[unit compile: "void* borrowed_symbol(void* value) { return value; }"];
[unit relocate];
let borrowed := [unit symbol: "borrowed_symbol"];
let borrowedFree := [borrowed free];
[System println: [[borrowedFree kind] printString]];
let fresh := [Tcc new];
[System println: [[[fresh addSymbol: "numeric" value: 1] isNil] printString]]
EOF
"$bin" "$tmp/foreign-address-contract.birnam" >"$tmp/foreign-address-contract.out"
grep -Fxq '#alreadyFreed' "$tmp/foreign-address-contract.out"
grep -Fxq '#notOwned' "$tmp/foreign-address-contract.out"
grep -Fxq 'true' "$tmp/foreign-address-contract.out"

# An image saved by a non-REPL script does not contain Repl. Its boot fallback
# must install the process-local REPL from the relocated compiled unit.
cat >"$tmp/save-no-repl.birnam" <<EOF
[System saveImage: "$tmp/no-repl.image"]
EOF
"$bin" "$tmp/save-no-repl.birnam" >/dev/null
printf ':quit\n' | "$bin" "$tmp/no-repl.image" >/dev/null

cat >"$tmp/isolate.birnam" <<EOF
let worker := [Isolate start: #RelocatedWorker selector: #handle: image: "$image"];
[System println: [worker request: 42 timeout: 5000]];
[worker stop]
EOF
test "$("$bin" "$tmp/isolate.birnam")" = "42"

# Required-resource failures must name the exact resolved path rather than
# degrading into a parser/DNU failure or silently reaching back into checkout.
cli="$tmp/relocated/lib/libbirnam_cli.dylib"
mv "$cli" "$cli.unavailable"
if "$bin" doc time nowMillis >/dev/null 2>"$tmp/missing-cli.err"; then
  echo "install-smoke: missing CLI compiled unit unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq "$cli" "$tmp/missing-cli.err"
mv "$cli.unavailable" "$cli"

# Descriptor identity, target, and ABI are checked before activation. The target
# probe also removes Codegen: the tiny IdApi-only inspector must diagnose the
# artifact before the compiler/runtime loading surface is even available.
time_unit="$tmp/relocated/lib/libbirnam_time.dylib"
cp "$time_unit" "$tmp/wrong-target.dylib"
perl -0pi -e 's/aarch64-macos/badarch-macos/g' "$tmp/wrong-target.dylib"
codegen_unit="$tmp/relocated/lib/libbirnam_id_codegen.dylib"
mv "$codegen_unit" "$codegen_unit.unavailable"
if "$bin" "$tmp/wrong-target.dylib" >/dev/null 2>"$tmp/wrong-target.err"; then
  echo "install-smoke: wrong-target artifact unexpectedly succeeded" >&2
  exit 1
fi
grep -q 'artifactTargetMismatch' "$tmp/wrong-target.err"
mv "$codegen_unit.unavailable" "$codegen_unit"

cp "$time_unit" "$tmp/wrong-abi.dylib"
perl -0pi -e 's/birnam-artifact-generation-v1/birnam-artifact-generation-v2/g' "$tmp/wrong-abi.dylib"
if "$bin" "$tmp/wrong-abi.dylib" >/dev/null 2>"$tmp/wrong-abi.err"; then
  echo "install-smoke: wrong-ABI artifact unexpectedly succeeded" >&2
  exit 1
fi
grep -q 'artifactAbiMismatch' "$tmp/wrong-abi.err"

cp "$time_unit" "$time_unit.correct"
cp "$tmp/relocated/lib/libbirnam_json.dylib" "$time_unit"
mkdir -p "$tmp/wrong-identity-project"
cat >"$tmp/wrong-identity-project/birnam.toml" <<'EOF'
load = ["time"]
EOF
if "$bin" run "$tmp/wrong-identity-project" >"$tmp/wrong-identity.err" 2>&1; then
  echo "install-smoke: wrong-identity artifact unexpectedly succeeded" >&2
  exit 1
fi
mv "$time_unit.correct" "$time_unit"

id_dylib="$tmp/relocated/lib/libbirnam_id.dylib"
mv "$id_dylib" "$id_dylib.unavailable"
if env -u BIRNAM_ID_FLOOR_IDENTITY -u BIRNAM_ID_FLOOR_SHA256 \
  "$bin" --repl </dev/null >/dev/null 2>"$tmp/missing-id.err"; then
  echo "install-smoke: missing id dylib unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq "required runtime dylib is unavailable: $id_dylib" "$tmp/missing-id.err"
mv "$id_dylib.unavailable" "$id_dylib"

echo "install-smoke: relocated compiled closure, tools, projects, images, isolates, and diagnostics passed"
