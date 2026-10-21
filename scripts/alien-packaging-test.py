#!/usr/bin/env python3
"""Standard prepared Alien builds, adjacent and single-file delivery.

python3 scripts/alien-packaging-test.py PREFIX
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import struct
import sys
import tempfile

BIN = Path(sys.argv[1]).resolve() / "bin/birnam"
ENV = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}


def run(*args, cwd, ok=True):
    p = subprocess.run([str(BIN), *map(str, args)], cwd=cwd, env=ENV, capture_output=True, timeout=120)
    assert (p.returncode == 0) == ok, (args, p.returncode, p.stdout, p.stderr)
    return p


def probe(root, library, marker, expected=43):
    driver = root / "probe.birnam"
    driver.write_text('''
[Alien libraryDir: DIRECTORY];
// Module inspection and supplying bytes must not run native constructors.
[Artifact inspect: ARTIFACT];
[Reflect defineMethod: #parse: on: Lang as: [:self :source | [Error kind: #forbiddenParse]]];
[Reflect defineMethod: #compile:signature: on: AlienCompiler as: [:self :name :signature | [Error kind: #forbiddenWrapperCompiler]]];
[Reflect defineMethod: #compile: on: TCC as: [:self :source | [Error kind: #forbiddenCCompiler]]];
[Reflect defineMethod: #compileObject: on: TCC as: [:self :source | [Error kind: #forbiddenCCompiler]]];
let loaded := [Module loadArtifact: ARTIFACT];
[[loaded isError] ifTrue: [| loaded] ifFalse: [|
  [System gc];
  [System println: [PackageFixture answer]];
  [System println: [PackageFixture bump]];
  [System println: [PackageFixture bump]];
  [System println: [PackageFixture path]];
  [System println: [[AlienRecipes snapshotRegistrations] anySatisfy: [:row | [[row at: 0] = "package_answer"]]]]
]]
'''.replace("DIRECTORY", json.dumps(str(library))).replace("ARTIFACT", json.dumps(str(library / "libbirnam_packageFixture.dylib"))))
    assert not marker.exists()
    p = run(driver, cwd=root)
    lines = p.stdout.decode().splitlines()
    assert lines[:3] == [str(expected), "1", "2"] and lines[-1] == "true", (p.stdout, p.stderr)
    assert marker.read_text() == "constructed\n"
    return Path(lines[3])


with tempfile.TemporaryDirectory(prefix="birnam-alien-packaging-test-") as temp:
    work = Path(temp)
    for mode in ("adjacent", "embedded"):
        project = work / mode; (project / "src").mkdir(parents=True); (project / "native").mkdir()
        marker = work / f"{mode}.constructed"
        manifest = f'''name = "package-fixture"
source_roots = ["src"]
load = ["packageFixture"]
main = "main.birnam"
[[wrapped]]
name = "package_fixture"
packaging = "{mode}"
build = [["cc", "-dynamiclib", "native/fixture.c", "-o", "{{output}}"]]
'''
        (project / "birnam.toml").write_text(manifest)
        native = project / "native/fixture.c"
        field = lambda value: struct.pack("<I", len(value)) + value
        payload = b"".join(map(field, [b"module", b"nested", b"birnam-artifact-generation-v1", b"aarch64-macos", b"a" * 64, b"b" * 64])) + struct.pack("<III", 0, 0, 0)
        collision = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1" + struct.pack("<I", len(payload)) + payload
        native.write_text('''#include <stdio.h>
#include <dlfcn.h>
static int counter;
__attribute__((used)) const unsigned char descriptor_collision_probe[] = {COLLISION};
int package_answer(void) { return 43; }
int package_bump(void) { return ++counter; }
const char *package_path(void) { Dl_info info; dladdr((void *)package_answer, &info); return info.dli_fname; }
__attribute__((constructor)) static void constructed(void) {
  FILE *f = fopen(MARKER, "w"); if (f) { fputs("constructed\\n", f); fclose(f); }
}
'''.replace("MARKER", json.dumps(str(marker))).replace("COLLISION", ",".join(map(str, collision))))
        source = project / "src/fixture.birnam"
        source.write_text('''[Module named: #packageFixture requires: #[]];
[Alien dlopen: "libpackage_fixture.dylib"];
let PackageFixture := [Object clone];
[Alien bind: "package_answer" signature: "i" as: #answer on: PackageFixture];
[Alien bind: "package_bump" signature: "i" as: #bump on: PackageFixture];
[Alien bind: "package_path" signature: "s" as: #path on: PackageFixture];
[PackageFixture answer]
''')
        (project / "main.birnam").write_text('[System println: [PackageFixture answer]]')
        run("check", project, cwd=work)
        assert not marker.exists() and not (project / ".birnam").exists(), "check executes no native build or module body"
        run("build", project, cwd=work)
        assert not marker.exists(), "build must not execute the native constructor"
        lib = project / ".birnam/build/aarch64-macos/debug/lib"
        assert (lib / "libpackage_fixture.dylib").exists() == (mode == "adjacent")
        # Inspect in an independent process, with the constructor sentinel absent.
        inspect = work / "inspect.birnam"
        inspect.write_text(f'[System println: [[Artifact inspect: {json.dumps(str(lib / "libbirnam_packageFixture.dylib"))}] nativeRequires]]')
        inspected = run(inspect, cwd=work)
        assert not marker.exists(), "inspection must stay inert"
        assert inspected.stdout.strip() == (b"#[libpackage_fixture.dylib]" if mode == "adjacent" else b"#[]"), inspected.stdout
        assert run("run", project, "--no-repl", cwd=work).stdout == b"43\n"
        marker.unlink()
        native.write_text(native.read_text().replace("return 43;", "return 44;"))
        run("build", project, cwd=work)
        assert not marker.exists()
        assert run("run", project, "--no-repl", cwd=work).stdout == b"44\n", "native-only edits must replace embedded payloads too"
        marker.unlink()
        previous = (lib / "libbirnam_packageFixture.dylib").read_bytes()
        valid_native = native.read_text()
        native.write_text("invalid C source\n")
        run("build", project, cwd=work, ok=False)
        assert (lib / "libbirnam_packageFixture.dylib").read_bytes() == previous
        assert run("run", project, "--no-repl", cwd=work).stdout == b"44\n", "failed native builds preserve the prior publication"
        marker.unlink()
        if mode == "embedded":
            native.write_text(valid_native)
            (project / "birnam.toml").write_text(manifest.replace('packaging = "embedded"', 'packaging = "adjacent"'))
            run("build", project, cwd=work)
            assert (lib / "libpackage_fixture.dylib").exists()
            assert run(inspect, cwd=work).stdout.strip() == b"#[libpackage_fixture.dylib]", "embedded-to-adjacent must not reuse an embedded module"
            (project / "birnam.toml").write_text(manifest)
            run("build", project, cwd=work)
            assert not (lib / "libpackage_fixture.dylib").exists()
            assert run(inspect, cwd=work).stdout.strip() == b"#[]"
        relocated = work / f"{mode}-relocated"; relocated.mkdir()
        shutil.copy2(lib / "libbirnam_packageFixture.dylib", relocated)
        if mode == "adjacent": shutil.copy2(lib / "libpackage_fixture.dylib", relocated)
        assert len(list(relocated.iterdir())) == (2 if mode == "adjacent" else 1)
        shutil.rmtree(project / "src"); shutil.rmtree(project / "native"); shutil.rmtree(project / ".birnam")
        materialized = probe(work, relocated, marker, expected=44)
        marker.unlink()
        run(relocated / "libbirnam_packageFixture.dylib", cwd=work)
        assert marker.exists(), "direct artifact entry loads its declared native closure"
        if mode == "embedded":
            assert not materialized.exists(), "normal process exit removes private native materialization"
        else:
            assert materialized.exists()
            (relocated / "libpackage_fixture.dylib").unlink()
            inspect.write_text(f'[System println: [Artifact inspect: {json.dumps(str(relocated / "libbirnam_packageFixture.dylib"))}]]; [System println: [Artifact lastFailure]]')
            assert b"nativeRequirementUnavailable" in run(inspect, cwd=work).stdout
        print(f"{mode}: inert check/build/inspect, project run, relocation, compiler-free binding, GC, registration and cleanup passed", flush=True)

    # An ordinary local object named Alien must not trigger declaration lowering.
    shadow = work / "shadow.birnam"
    shadow.write_text('''[Module named: #shadow requires: #[]];
let Alien := [Object clone];
[Reflect defineMethod: #bind:signature:as:on: on: Alien as: [:self :fn :sig :sel :target | 79]];
[System println: [Alien bind: "not_a_real_function" signature: "not a signature" as: #x on: nil]]
''')
    artifact = work / "libbirnam_shadow.dylib"
    run("--compile-module", shadow, artifact, cwd=work)
    assert run(artifact, cwd=work).stdout == b"79\n"
    shadow.write_text('[Module named: #bad requires: #[]]; [Alien bind: "labs" signature: "nonsense" as: #x on: Integer]')
    run("--compile-module", shadow, work / "bad.dylib", cwd=work, ok=False)
    assert not (work / "bad.dylib").exists()
    print("declarations: lexical shadowing respected; malformed signature rejected at build", flush=True)
