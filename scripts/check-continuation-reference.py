#!/usr/bin/env python3
"""Run the executable reference and compare two completed TLA+ witness projections.
Run check-continuation-model.py first. No production runtime is qualified here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "formal/continuations/reference"
OUT = ROOT / ".cargo-target/continuation-reference/qualification"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def model_entries(trace):
    states = re.split(r"(?m)^State \d+: ", trace)[1:]
    entries = []
    for previous, following in zip(states, states[1:]):
        if not re.match(r"<(EnterLookup|EnterNative|EnterHelper)\b", following):
            continue
        selected = re.search(r'(?m)^/\\ selected = "([^"]+)"', previous).group(1)
        cur = re.search(r"(?ms)^/\\ cur = (.*?)(?=^/\\ |\Z)", previous).group(1)
        receiver = re.search(r"r \|-> (\d+)", cur).group(1)
        selector = re.search(r's \|-> "([^"]+)"', cur).group(1)
        args = re.search(r"args \|-> <<(.*?)>>", cur, re.S).group(1)
        arguments = re.findall(r'"([^"]+)"', args)
        entries.append("ENTRY|" + "|".join([selected, receiver, selector, ",".join(arguments)]))
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--formal-receipt", type=Path, default=ROOT / ".cargo-target/continuations/check/receipt.json")
    args = parser.parse_args()
    formal = json.loads(args.formal_receipt.read_text())
    assert formal["status"].startswith("passed") and formal["suite"] == "all"
    for name in ["Metacircular.tla", "MetaCompletedSync.cfg", "MetaCompletedAsync.cfg"]:
        path = ROOT / "formal/continuations/tla" / name
        assert digest(path) == formal["source_sha256"][str(path.relative_to(ROOT))], name
    OUT.mkdir(parents=True, exist_ok=True)
    report = {"status": "incomplete", "formal_receipt_sha256": digest(args.formal_receipt), "comparisons": {}}
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    env = dict(os.environ, MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    common = ["--offline", "--locked", "--manifest-path", str(REFERENCE / "Cargo.toml"), "--target-dir", str(ROOT / ".cargo-target/continuation-reference")]
    check = subprocess.run(["mise", "exec", "--", "cargo", "test", *common], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
    output = check.stdout + check.stderr
    (OUT / "tests.log").write_text(output)
    assert check.returncode == 0 and "warning:" not in output, output
    report["tests"] = output
    report["tests_passed"] = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", output)))
    for mode in ["Sync", "Async"]:
        case = formal["tla"]["MetaCompleted" + mode]
        assert case["expected_failure"] == "Invariant NoCompletedSend is violated."
        expected = model_entries(case["counterexample"])
        assert len(expected) == 9, expected
        run = subprocess.run(["mise", "exec", "--", "cargo", "run", "--quiet", *common, "--", mode.lower()], cwd=ROOT, env=env, capture_output=True, text=True, timeout=30)
        assert run.returncode == 0 and not run.stderr, (run.stdout, run.stderr)
        (OUT / (mode + ".log")).write_text(run.stdout)
        actual = [line for line in run.stdout.splitlines() if line.startswith("ENTRY|")]
        assert actual == expected, {"mode": mode, "expected": expected, "actual": actual}
        assert "RESULT|answer" in run.stdout
        report["comparisons"][mode] = {"model": expected, "executable": actual, "stdout": run.stdout}
        print(mode, len(actual), "projected entries match TLC", flush=True)
    files = [p for p in REFERENCE.rglob("*") if p.is_file() and "target" not in p.parts]
    files.append(Path(__file__).resolve())
    report["source_sha256"] = {str(p.relative_to(ROOT)): digest(p) for p in sorted(files)}
    report["status"] = "passed executable reference tests and two finite trace comparisons; not production or ABI acceptance"
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Receipt:", OUT / "receipt.json")


if __name__ == "__main__":
    main()
