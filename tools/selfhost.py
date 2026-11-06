#!/usr/bin/env python3
"""Build two native compiler generations and check their generated-C fixed point."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import build
import compile as driver

ROOT = Path(__file__).resolve().parents[1]
MODULES = driver.MODULES


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export_name(module: str) -> str:
    return "Native_" + module.replace("-", "_")


def build_pipeline(directory: Path) -> dict:
    directory = directory.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    # A failed new run must not leave an old success receipt at this location.
    receipt = directory / "receipt.json"
    receipt.write_text(json.dumps({"status": "building"}) + "\n")
    guard = directory / "guard.dylib"
    driver.compile_c(ROOT / "tests/selfhost_guard.c", guard)
    report = {"status": "building", "toolchain": build.configuration()[2],
              "siphon_sha256": digest(ROOT / "build/siphon"),
              "seed_sha256": digest(ROOT / "build/seed.dylib"),
              "request_adapter_sha256": digest(ROOT / "build/compile-request.dylib"),
              "modules": {}, "stages": {}}
    start = time.monotonic()
    for module in MODULES:
        report["modules"][module] = {"source_sha256": digest(ROOT / f"modules/{module}.birnam")}
    previous = None
    for stage in ("stage1", "stage2"):
        target = directory / stage
        target.mkdir(exist_ok=True)
        report["stages"][stage] = {}
        for module in MODULES:
            print(f"{stage}: {module}", flush=True)
            then = time.monotonic()
            c, library = target / f"{module}.c", target / f"{module}.dylib"
            driver.emit(ROOT / f"modules/{module}.birnam", c, export_name(module),
                        native_dir=previous, guard=guard if previous else None)
            if previous and c.read_bytes() != (previous / c.name).read_bytes():
                raise RuntimeError(f"native compiler output differs from seed output: {module}")
            driver.compile_c(c, library)
            report["stages"][stage][module] = {"c_sha256": digest(c), "library_sha256": digest(library),
                                                  "seconds": round(time.monotonic() - then, 3)}
        previous = target
    # Exercise the compiler produced by the compiler, not only its emitted text.
    final = directory / "stage3-c"
    final.mkdir(exist_ok=True)
    report["stages"]["stage3-c"] = {}
    for module in MODULES:
        print(f"stage3-c: {module}", flush=True)
        c = final / f"{module}.c"
        driver.emit(ROOT / f"modules/{module}.birnam", c, export_name(module), native_dir=previous, guard=guard)
        if c.read_bytes() != (previous / c.name).read_bytes():
            raise RuntimeError(f"second native generation output changed: {module}")
        report["stages"]["stage3-c"][module] = {"c_sha256": digest(c)}
    c, application = directory / "answer.c", directory / "answer.dylib"
    driver.emit(ROOT / "examples/compiler/answer.birnam", c, "NativeProgram", native_dir=previous, guard=guard)
    driver.compile_c(c, application)
    result = subprocess.run([str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib"),
                             str(guard), str(application), str(guard)],
                            cwd="/private/tmp", text=True, capture_output=True, timeout=60)
    if result.returncode or result.stdout != "42\n":
        raise RuntimeError(f"self-compiled application failed: {result.stdout} {result.stderr}")
    report.update(status="passed", seconds=round(time.monotonic()-start, 3),
                  generated_c_fixed_point=True, interpreter_guard=True, example_stdout=result.stdout)
    receipt.write_text(json.dumps(report, indent=2) + "\n")
    print(f"PASS native bootstrap: two compiled generations; all five modules stable across three C emissions; {report['seconds']}s", flush=True)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "build/selfhost")
    args = parser.parse_args()
    build_pipeline(args.directory)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"selfhost: {error}")
