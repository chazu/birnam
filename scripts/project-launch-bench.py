#!/usr/bin/env python3
"""Compare warm project launch across installed prefixes, with Git/native deps.

python3 scripts/project-launch-bench.py --binary before=/path/to/old/bin/birnam \
    --binary after=/path/to/new/bin/birnam --runs 5

Build/fetch and one warmup are excluded. Results are diagnostic wall-clock
measurements, not release acceptance evidence. No existing project is modified.
"""
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time


def run(args, cwd, env):
    return subprocess.run(args, cwd=cwd, env=env, check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", action="append", required=True, metavar="LABEL=PATH")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--modules", type=int, default=8,
                        help="authored modules above the pinned native dependency")
    args = parser.parse_args()
    if args.runs < 3:
        parser.error("use at least three measured launches")
    if not 1 <= args.modules <= 100:
        parser.error("--modules must be between 1 and 100")
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    results = []
    with tempfile.TemporaryDirectory(prefix="birnam-project-launch-") as temporary:
        root = Path(temporary)
        dep = root / "native"
        (dep / "src").mkdir(parents=True)
        (dep / "birnam.toml").write_text('''name = "launch-native"
source_roots = ["src"]
[[wrapped]]
name = "launch_native"
build = [["cc", "-dynamiclib", "fixture.c", "-o", "{output}"]]
''')
        (dep / "fixture.c").write_text("int launch_answer(void) { return 42; }\n")
        (dep / "src/launch_native.birnam").write_text('''[Module named: #launch_native requires: #[]]
[Alien dlopen: "liblaunch_native.dylib"]
[Globals at: #LaunchNative put: [Object clone]]
[Alien wrap: "launch_answer" signature: "i" as: #answer on: LaunchNative]
''')
        run(["git", "init", "--quiet"], dep, env)
        run(["git", "add", "birnam.toml", "fixture.c", "src"], dep, env)
        run(["git", "-c", "user.name=Birnam", "-c", "user.email=birnam@example.invalid",
             "commit", "--quiet", "-m", "fixture"], dep, env)
        revision = run(["git", "rev-parse", "HEAD"], dep, env).stdout.strip()
        for index, selection in enumerate(args.binary):
            label, separator, executable = selection.partition("=")
            if not separator or not label:
                parser.error("--binary requires LABEL=PATH")
            binary = Path(executable).resolve(strict=True)
            project = root / f"project-{index}"
            (project / "src").mkdir(parents=True)
            (project / "birnam.toml").write_text(f'''name = "launch-bench"
main = "main.birnam"
load = ["launch_app_{args.modules}"]
[dependencies]
launch_native = {{ git = "{dep}", rev = "{revision}" }}
''')
            for number in range(1, args.modules + 1):
                required = "launch_native" if number == 1 else f"launch_app_{number - 1}"
                (project / f"src/launch_app_{number}.birnam").write_text(
                    f"[Module named: #launch_app_{number} requires: #[#{required}]]\n")
            (project / "main.birnam").write_text(
                '[System println: [[LaunchNative answer] printString]]\n')
            run([str(binary), "fetch", str(project)], root, env)
            run([str(binary), "build", str(project)], root, env)
            samples = []
            for iteration in range(args.runs + 1):
                started = time.perf_counter()
                output = run([str(binary), "run", str(project), "--no-repl"], root, env)
                elapsed = time.perf_counter() - started
                if output.stdout.strip() != "42":
                    raise RuntimeError(f"{label}: unexpected application output: {output.stdout!r}")
                if iteration:
                    samples.append(elapsed)
            receipt_path = binary.parent.parent / "share/birnam/rust-install.json"
            receipt = load_json(receipt_path)
            results.append({"label": label, "binary": str(binary),
                            "binary_sha256": hashlib.sha256(read_bytes(binary)).hexdigest(),
                            "source_manifest_sha256": receipt["source_manifest_sha256"],
                            "seconds": samples, "median_seconds": statistics.median(samples)})
    print(json.dumps({"schema": "birnam-project-launch-bench-v1",
                      "machine": platform.platform(), "profile": "release runtime, debug project",
                      "warmups": 1, "runs": args.runs,
                      "authored_modules": args.modules + 1, "results": results}, indent=2))


if __name__ == "__main__":
    main()
