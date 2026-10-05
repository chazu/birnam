#!/usr/bin/env bash
# Common Id descriptor loading through startup, compiled code and saved/baked images.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: native-loader-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-loader-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'native loader image smoke root: %s\n' "$scratch"
fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib \
  scripts/fixtures/native-loader-target.c -o "$scratch/target.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib \
  -DNATIVE_LOADER_NO_INIT scripts/fixtures/native-loader-target.c -o "$scratch/not-module.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System println: [[[System loadModule: "$scratch/target.dylib" arguments: #[System]] == System] printString]];
[System println: [System nativeLoaderCount]];
[System loadModule: "$scratch/target.dylib" arguments: #[System]];
[System println: [System nativeLoaderCount]];
[System println: [[System loadModule: "$scratch/missing.dylib" arguments: #[System]] kind]];
[System println: [[System loadModule: "$scratch/not-module.dylib" arguments: #[System]] kind]];
[System println: "native-loader-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\n#native-load-1\n#native-load-2\n#ioError\n#notAModule\nnative-loader-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
if grep -Fq '/libbirnam_native_loader.dylib' "$scratch/startup.err"; then
  echo 'removed native loader was loaded' >&2; exit 1
fi
test -f "$prefix/lib/libbirnam_nativeLoader.dylib"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
[Reflect defineMethod: #savedReceiver on: receiver as: [:self | self]];
let path := "$scratch/target.dylib";
let pathAlias := path;
let main := [|
  [System gc];
  [System println: [receiver == alias]];
  [System println: [[receiver savedReceiver] == alias]];
  [System println: [path == pathAlias]];
  [System println: [[receiver loadModule: path arguments: #[System]] == alias]];
  [System println: [System nativeLoaderCount]];
  [receiver loadModule: path arguments: #[System]];
  [System println: [System nativeLoaderCount]];
  [System println: [[receiver loadModule: "$scratch/missing.dylib" arguments: #[System]] kind]];
  [System println: [[receiver loadModule: "$scratch/not-module.dylib" arguments: #[System]] kind]];
  // Image main runs under the Host's running-phase guard. Live replacement is
  // tested at the native owner boundary; it requires an idle intent here.
  [System println: [[Reflect defineMethod: #loadModule:arguments: on: receiver as: [:self :name :arguments | nil]] kind]];
  [System gc];
  [System println: [[receiver loadModule: path arguments: #[System]] == alias]];
  [System println: [System nativeLoaderCount]];
  [System println: "native-loader-image-ok"]
];
[System saveImage: "$scratch/app.image"];
[System println: [[System imageSaveReport] status]]
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
grep -Fxq '#saved' "$scratch/save.out"
test -s "$scratch/app.image"
rm "$scratch/save.birnam"
printf 'true\ntrue\ntrue\ntrue\n#native-load-1\n#native-load-2\n#ioError\n#notAModule\n#generationNotQuiescent\ntrue\n#native-load-3\nnative-loader-image-ok\n' >"$scratch/expected.out"
for mode in mapped rebuilt; do
  if [[ $mode == rebuilt ]]; then
    "$bin" --rebuild "$scratch/app.image" >"$scratch/$mode.out" 2>"$scratch/$mode.err"
  else
    "$bin" "$scratch/app.image" >"$scratch/$mode.out" 2>"$scratch/$mode.err"
  fi
  cmp "$scratch/expected.out" "$scratch/$mode.out"
  test ! -s "$scratch/$mode.err"
done
# A baked executable uses the same install-relative native closure as the host.
cp -R "$prefix" "$scratch/bundle"
"$bin" --bake "$scratch/app.image" "$scratch/bundle/bin/app"
"$scratch/bundle/bin/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
test ! -s "$scratch/baked.err"
printf 'native loader: descriptor loading, source-free artifact, mapped/rebuilt Image and baked entry passed\n'
