#!/usr/bin/env python3
"""Check the proposed opaque-continuation boundary. No production verification.
Uses the existing checksum-pinned tools; downloads only with --fetch-tools.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "formal/continuations"
OUT = ROOT / ".cargo-target/continuations/check"
SPEC = importlib.util.spec_from_file_location("existing_formal", ROOT / "scripts/check-rooting-model.py")
TOOLS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOLS)
CASES = {
    "Safe": None, "Tail": None, "ThreeTasks": None,
    "Relookup": "Invariant SelectedEntry is violated.",
    "CachedLookup": "Invariant LookupCurrent is violated.",
    "Replay": "Invariant NoReplay is violated.",
    "TailPush": "Invariant TailBound is violated.",
    "CrossAnswer": "Invariant AnswerDestination is violated.",
    "DropCode": "Invariant CodeAlive is violated.",
    "DoubleWake": "Invariant OneShotWake is violated.",
    "NativeLookup": "Temporal properties were violated.",
    "Unfair": "Temporal properties were violated.",
    "NoLookupPause": "Invariant NoLookupPause is violated.",
    "NoSelectedReplacement": "Invariant NoSelectedReplacement is violated.",
    "NoBodyPause": "Invariant NoBodyPause is violated.",
    "NoTwoWaiting": "Invariant NoTwoWaiting is violated.",
}
MUTATIONS = {
    "tail-push": ("(⟨m, f.destination⟩ :: fs, .entered m f.destination)",
                  "(⟨m, f.destination⟩ :: f :: fs, .entered m f.destination)"),
    "wrong-destination": ("frames.dropLast ++ [⟨m, f.destination⟩]",
                          "frames.dropLast ++ [⟨m, m⟩]"),
    "reusable-wakeup": ("if token ∈ used then none else some (token :: used)",
                        "some (token :: used)"),
    "erase-event": ("(s'', event :: events)", "(s'', events)"),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, log, cwd=ROOT, env=None, timeout=180):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True,
                            timeout=timeout)
    output = result.stdout + result.stderr
    log.write_text(output)
    return result.returncode, output, round(time.monotonic() - started, 3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lean")
    parser.add_argument("--tla-jar")
    parser.add_argument("--java")
    parser.add_argument("--fetch-tools", action="store_true")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    jar, lean, java, version = TOOLS.tools(args)
    report = {"status": "incomplete", "baseline": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "tools": {"lean_version": version, "lean_sha256": digest(lean),
                  "tlc_sha256": digest(jar)}, "tla": {}, "lean": {}}
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    for name, expected in CASES.items():
        command = [java, "-XX:ActiveProcessorCount=1", "-XX:+UseParallelGC", "-Xmx768m",
                   "-cp", str(jar), "tlc2.TLC", "-workers", "1", "-seed", "1", "-fp", "1",
                   "-metadir", str(OUT / ("states-" + name)), "-config",
                   str(MODEL / "tla" / (name + ".cfg")), str(MODEL / "tla/ContinuationSend.tla")]
        code, output, seconds = run(command, OUT / (name + ".log"))
        if expected:
            assert code != 0 and expected in output and "State 1:" in output, (name, code, output[-3000:])
        else:
            assert code == 0 and "Model checking completed. No error has been found." in output, (name, output[-3000:])
        states = re.findall(r"(\d+) states generated, (\d+) distinct states found", output)
        report["tla"][name] = {"returncode": code, "seconds": seconds,
            "expected_failure": expected, "states": states[-1] if states else None,
            "log_sha256": digest(OUT / (name + ".log")),
            "counterexample": output[output.index("Error:"):] if expected else None}
        print(name, "confirmed counterexample" if expected else "passed", states[-1:] or "", flush=True)
    source = (MODEL / "lean/Continuation.lean").read_text()
    assert not re.search(r"\b(sorry|admit|axiom|native_decide)\b", source)
    build = OUT / "lean"
    build.mkdir(exist_ok=True)
    env = dict(os.environ, LEAN_PATH=str(build))
    code, output, seconds = run([str(lean), "-o", str(build / "Continuation.olean"),
        "Continuation.lean"], OUT / "lean.log", cwd=MODEL / "lean", env=env)
    assert code == 0 and "warning:" not in output, output
    code, audit, _ = run([str(lean), "Audit.lean"], OUT / "audit.log", cwd=MODEL / "lean", env=env)
    assert code == 0, audit
    names = re.findall(r"^theorem (\w+)", source, re.M)
    for name in names:
        assert "Birnam.Continuation." + name in audit, name
    for group in re.findall(r"depends on axioms:\s*\[([^]]*)\]", audit):
        assert set(filter(None, re.split(r"[,\s]+", group))) <= TOOLS.ALLOWED_AXIOMS, group
    assert "sorryAx" not in audit
    report["lean"] = {"theorems": names, "audit": audit, "seconds": seconds, "mutations": {}}
    for name, (before, after) in MUTATIONS.items():
        assert source.count(before) == 1, name
        file = build / ("Mutant_" + name.replace("-", "_") + ".lean")
        file.write_text(source.replace(before, after))
        code, output, _ = run([str(lean), str(file)], OUT / ("lean-" + name + ".log"), env=env)
        assert code != 0 and "Tactic `decide` proved that the proposition" in output, (name, output)
        report["lean"]["mutations"][name] = {"returncode": code, "errors": output}
        print("Lean", name, "rejected", flush=True)
    files = sorted(p for p in MODEL.rglob("*") if p.is_file() and ".lake" not in p.parts)
    files.append(Path(__file__).resolve())
    report["source_sha256"] = {str(p.relative_to(ROOT)): digest(p) for p in files}
    report["status"] = "passed abstract checks and required counterexamples; no implementation proof"
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Receipt:", OUT / "receipt.json", flush=True)


if __name__ == "__main__":
    main()
