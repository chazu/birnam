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
BOUNDARY_CASES = {
    "MetaSync": None, "MetaAsync": None, "MetaDeep": None,
    "MetaEarlyPublish": "Invariant BootstrapReady is violated.",
    "MetaNoAnchor": "Invariant ResolutionBound is violated.",
    "MetaFrozenAnchor": "Invariant SelectionPreserved is violated.",
    "MetaShortcut": "Invariant CompletedCorrectly is violated.",
    "MetaReselect": "Invariant SelectionPreserved is violated.",
    "MetaLoseRequest": "Invariant RequestPreserved is violated.",
    "MetaLookupReplay": "Invariant NoLookupReplay is violated.",
    "MetaSelectedLookup": "Invariant DirectEntry is violated.",
    "MetaDropLookupCode": "Invariant CodeAlive is violated.",
    "MetaNativeEscape": "Invariant NativeBarrier is violated.",
    "MetaNativeTail": "Invariant CompletedCorrectly is violated.",
    "MetaDoubleCompletion": "Invariant Once is violated.",
    "MetaCallbackTail": "Invariant NoLookupReplay is violated.",
    "MetaNoLookupCallback": "Invariant NoLookupCallback is violated.",
    "MetaOverlap": "Invariant SingleDriver is violated.",
    "MetaNoSuspendedLookup": "Invariant NoSuspendedLookup is violated.",
    "MetaNoSuspendedNativeCallback": "Invariant NoSuspendedNativeCallback is violated.",
    "MetaNoDeniedNativeSuspend": "Invariant NoDeniedNativeSuspend is violated.",
    "MetaNoReplacedLookupEntry": "Invariant NoReplacedLookupEntry is violated.",
    "MetaNoStaleCompletion": "Invariant NoStaleCompletion is violated.",
    "ScopeSafe": None,
    "ScopeClearAncestors": "Invariant NativeFrames is violated.",
    "ScopeForeignReturn": "Invariant NativeFrames is violated.",
    "ScopeIgnoreOwner": "Invariant OwnerOnly is violated.",
    "ScopeReuseTicket": "Invariant AtMostOnce is violated.",
    "ScopeIgnoreBarrier": "Invariant NoEscape is violated.",
    "ScopeNoDetachedWait": "Invariant NoDetachedWait is violated.",
    "ScopeNoRefusal": "Invariant NoRefusal is violated.",
}
BOUNDARY_MUTATIONS = {
    "early-publication": ("if s.graph && s.seeded then { s with published := true } else s", "{ s with published := true }"),
    "broad-anchor": ("r.receiver == root && r.selector == lookup", "r.receiver == root || r.selector == lookup"),
    "clear-ancestors": ("{ s with frames := rest }", "{ s with frames := [] }"),
    "ignore-barrier": ("if s.frames.isEmpty then { s with parked := true } else s", "{ s with parked := true }"),
    "ignore-owner": ("caller = t.owner ∧ t.used = false", "t.used = false"),
    "reuse-completion": ("caller = t.owner ∧ t.used = false", "caller = t.owner"),
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
    parser.add_argument("--suite", choices=["all", "boundaries"], default="all")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    jar, lean, java, version = TOOLS.tools(args)
    report = {"status": "incomplete", "baseline": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "tools": {"lean_version": version, "lean_sha256": digest(lean),
                  "tlc_sha256": digest(jar)}, "suite": args.suite, "tla": {}, "lean": {}}
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    cases = BOUNDARY_CASES if args.suite == "boundaries" else {**CASES, **BOUNDARY_CASES}
    for name, expected in cases.items():
        module = "Metacircular" if name.startswith("Meta") else "NativeScopes" if name.startswith("Scope") else "ContinuationSend"
        command = [java, "-XX:ActiveProcessorCount=1", "-XX:+UseParallelGC", "-Xmx768m",
                   "-cp", str(jar), "tlc2.TLC", "-workers", "1", "-seed", "1", "-fp", "1",
                   "-metadir", str(OUT / ("states-" + name)), "-config",
                   str(MODEL / "tla" / (name + ".cfg")), str(MODEL / "tla" / (module + ".tla"))]
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
    build = OUT / "lean"
    build.mkdir(exist_ok=True)
    env = dict(os.environ, LEAN_PATH=str(build))
    modules = {"Continuation": MUTATIONS, "Boundary": BOUNDARY_MUTATIONS}
    names = []
    for module in modules:
        source = (MODEL / "lean" / (module + ".lean")).read_text()
        assert not re.search(r"\b(sorry|admit|axiom|native_decide)\b", source)
        code, output, seconds = run([str(lean), "-o", str(build / (module + ".olean")),
            module + ".lean"], OUT / ("lean-" + module + ".log"), cwd=MODEL / "lean", env=env)
        assert code == 0 and "warning:" not in output, output
        names.extend("Birnam." + module + "." + name for name in re.findall(r"^theorem (\w+)", source, re.M))
    code, audit, _ = run([str(lean), "Audit.lean"], OUT / "audit.log", cwd=MODEL / "lean", env=env)
    assert code == 0, audit
    for name in names:
        assert name in audit, name
    for group in re.findall(r"depends on axioms:\s*\[([^]]*)\]", audit):
        assert set(filter(None, re.split(r"[,\s]+", group))) <= TOOLS.ALLOWED_AXIOMS, group
    assert "sorryAx" not in audit
    report["lean"] = {"theorems": names, "audit": audit, "mutations": {}}
    for module, mutations in modules.items():
        source = (MODEL / "lean" / (module + ".lean")).read_text()
        for mutation, (before, after) in mutations.items():
            name = module + "-" + mutation
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
