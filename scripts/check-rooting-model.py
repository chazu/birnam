#!/usr/bin/env python3
"""Bounded, single-worker TLC checks + fresh Lean kernel checks and mutations.

No downloads unless --fetch-tools is given. Results live under .cargo-target.
This verifies the abstract contract, not the generated C or current Root owner.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
FORMAL = ROOT / "formal/rooting"
CACHE = ROOT / ".cargo-target/formal"
ALLOWED_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}
CASES = {
    "Safe": None,
    "RegistrationSafe": None,
    "RegistrationUnrooted": "Invariant InputsAlive is violated.",
    "RegistrationPartial": "Invariant ExactRegistrations is violated.",
    "BatchSafe": None,
    "BorrowSafe": None,
    "BorrowCallback": "Invariant SameInspection is violated.",
    "BatchStale": "Invariant ExactCustomCallbacks is violated.",
    "BatchOrder": "Invariant ExactRetainedOrder is violated.",
    "Canonical": None,
    "Graph": None,
    "Nested": None,
    "Recovery": None,
    "OmitFirst": "Invariant RootCoverage is violated.",
    "EarlyRelease": "Invariant RootCoverage is violated.",
    "IgnorePermission": "Invariant ObservableEquality is violated.",
    "BadRevoke": "Invariant ObservableEquality is violated.",
    "LeakExit": "Invariant ReachabilityEquality is violated.",
    "UnfairRecovery": "Temporal properties were violated.",
}


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def checked_download(url, dest, sha):
    if dest.exists() and digest(dest) == sha:
        return
    partial = dest.with_suffix(dest.suffix + ".download")
    print(f"Downloading {dest.name}", flush=True)
    deadline = time.monotonic() + 180
    try:
        with urllib.request.urlopen(url, timeout=30) as response, partial.open("wb") as f:
            while chunk := response.read(1024 * 1024):
                if time.monotonic() > deadline:
                    raise TimeoutError("tool download exceeded 180 seconds")
                f.write(chunk)
        if digest(partial) != sha:
            raise RuntimeError(f"checksum mismatch: {dest.name}")
        partial.replace(dest)
    finally:
        partial.unlink(missing_ok=True)


def run(command, log, cwd=ROOT, env=None, timeout=30):
    start = time.monotonic()
    # Low priority and one TLC worker; no idle-machine requirement.
    result = subprocess.run(command, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            timeout=timeout, check=False)
    log.write_text(result.stdout)
    return result, round(time.monotonic() - start, 3)


def tools(args):
    lock = json.loads((FORMAL / "tools.json").read_text())
    cache = CACHE / "tools"
    cache.mkdir(parents=True, exist_ok=True)
    jar = Path(args.tla_jar).resolve() if args.tla_jar else cache / "tla2tools-1.7.4.jar"
    lean = Path(args.lean).resolve() if args.lean else None
    if args.fetch_tools and not args.tla_jar:
        checked_download(lock["tla"]["url"], jar, lock["tla"]["sha256"])
    if not jar.is_file() or digest(jar) != lock["tla"]["sha256"]:
        raise RuntimeError("missing/wrong pinned TLC jar; run with --fetch-tools")
    if lean is None:
        key = f"{platform.system()}-{platform.machine()}"
        archive = lock["lean"]["archives"].get(key)
        if archive is None:
            raise RuntimeError(f"no pinned Lean archive for {key}; supply --lean")
        dest = cache / archive["name"]
        lean = cache / archive["name"].removesuffix(".tar.zst") / "bin/lean"
        if args.fetch_tools and not lean.is_file():
            checked_download(lock["lean"]["base_url"] + archive["name"], dest, archive["sha256"])
            if not shutil.which("unzstd"):
                raise RuntimeError("unzstd is needed to extract the pinned Lean archive")
            subprocess.run(["tar", "--use-compress-program=unzstd", "-xf", str(dest),
                            "-C", str(cache)], check=True, timeout=90)
    if not lean.is_file():
        raise RuntimeError("missing Lean; run with --fetch-tools or supply --lean")
    java = args.java or shutil.which("java")
    if not java:
        raise RuntimeError("Java 17+ is needed for TLC")
    version = subprocess.check_output([str(lean), "--version"], text=True, timeout=10).strip()
    if not version.startswith(f"Lean (version {lock['lean']['version']},"):
        raise RuntimeError(f"wrong Lean version: {version}")
    return jar, lean, java, version


def check_tla(jar, java, out):
    results = {}
    for name, expected_failure in CASES.items():
        log = out / f"tlc-{name}.log"
        cmd = [java, "-XX:ActiveProcessorCount=1", "-XX:+UseParallelGC", "-Xmx512m",
               "-cp", str(jar), "tlc2.TLC", "-workers", "1", "-seed", "1", "-fp", "1",
               "-metadir", str(out / f"states-{name}"),
               "-config", str(FORMAL / f"tla/{name}.cfg"),
               str(FORMAL / ("tla/RegistrationBatch.tla" if name.startswith("Registration") else "tla/BatchCleanup.tla" if name.startswith("Batch") else "tla/BorrowInspection.tla" if name.startswith("Borrow") else "tla/DuplicateRoots.tla"))]
        result, seconds = run(cmd, log)
        text = result.stdout
        if expected_failure:
            ok = result.returncode != 0 and f"Error: {expected_failure}" in text
            ok &= "The behavior up to this point is:" in text or "counter-example" in text.lower()
        else:
            ok = result.returncode == 0 and "Model checking completed. No error has been found." in text
            ok &= "0 states left on queue." in text and "Error:" not in text
        if not ok:
            raise RuntimeError(f"TLC {name} did not give the required result; see {log}")
        counts = re.search(r"([\d,]+) states generated, ([\d,]+) distinct states found", text)
        results[name] = {"expected_violation": expected_failure, "seconds": seconds,
                         "generated_states": int(counts[1].replace(",", "")) if counts else None,
                         "distinct_states": int(counts[2].replace(",", "")) if counts else None,
                         "log": log.name}
        print(f"TLC {name}: {'expected counterexample' if expected_failure else 'pass'} ({seconds}s)", flush=True)
    return results


def lean_theorems():
    names = []
    for module, namespace in [("Rooting", "Birnam.Rooting"), ("Examples", "Birnam.Rooting.Examples"), ("GC", "Birnam.GC"), ("Memo", "Birnam.Memo"), ("Borrow", "Birnam.Borrow"), ("Registration", "Birnam.Registration")]:
        text = (FORMAL / f"lean/{module}.lean").read_text()
        names.extend(f"{namespace}.{name}" for name in re.findall(r"^theorem (\w+)", text, re.M))
        # The kernel audit below is authoritative; this additionally rejects
        # accidental proof holes or unsafe evaluator shortcuts in source.
        without_comments = re.sub(r"/\-.*?\-/|--[^\n]*", "", text, flags=re.S)
        if re.search(r"\b(sorry|admit|axiom|unsafe|native_decide)\b", without_comments):
            raise RuntimeError(f"proof hole/custom axiom/unsafe shortcut in {module}")
    return names


def check_lean(lean, out):
    build = out / "lean"
    build.mkdir()
    env = dict(os.environ, LEAN_PATH=str(build))
    for module in ["Rooting", "Examples", "GC", "Memo", "Borrow", "Registration"]:
        result, _ = run([str(lean), "-DwarningAsError=true", str(FORMAL / f"lean/{module}.lean"),
                         "-o", str(build / f"{module}.olean")], out / f"lean-{module}.log", env=env)
        if result.returncode:
            raise RuntimeError(f"Lean {module} failed; see {out / f'lean-{module}.log'}")
    names = lean_theorems()
    # Ensure the tracked audit lists every theorem, including counterexamples.
    audit = (FORMAL / "lean/Audit.lean").read_text()
    if set(re.findall(r"^#print axioms (\S+)$", audit, re.M)) != set(names):
        raise RuntimeError("Audit.lean must list every theorem")
    result, _ = run([str(lean), "-DwarningAsError=true", str(FORMAL / "lean/Audit.lean")],
                     out / "lean-axioms.log", env=env)
    if result.returncode:
        raise RuntimeError("Lean axiom audit failed")
    reported = re.findall(r"'([^']+)' (?:depends on axioms: \[([^\]]*)\]|does not depend on any axioms)", result.stdout)
    if {name for name, _ in reported} != set(names):
        raise RuntimeError("incomplete kernel axiom audit")
    for name, raw in reported:
        used = {a.strip() for a in raw.split(",") if a.strip()}
        if not used <= ALLOWED_AXIOMS:
            raise RuntimeError(f"unapproved axiom dependencies in {name}: {used}")
    source = (FORMAL / "lean/Rooting.lean").read_text()
    mutations = {
        "omit-first": ("if optimized then (if p = 0 then 1 else p) else p + 1",
                       "if optimized then (if p = 0 then 0 else p) else p + 1"),
        "leak-on-exit": ("physical := fun s o => if s = scope then 0 else st.physical s o",
                         "physical := st.physical"),
        "skip-materialization": ("{st with physical := st.logical, optimized := fun _ => false}",
                                 "{st with physical := st.physical, optimized := fun _ => false}"),
    }
    extra_mutations = {
        "wrong-release-order": ("GC", "callerTokens.reverse.foldl release st", "callerTokens.foldl release st"),
        "lost-index-mark": ("GC", "indexed := if st.indexed (index o) then st.indexed else fun i => if i = index o then true else st.indexed i", "indexed := st.indexed"),
        "wrong-memo-key": ("Memo", "if key = k then some v else lookup rest key", "some v"),
        "truncated-borrow": ("Borrow", "bytes.map id", "[]"),
    }
    for name, (module, before, after) in extra_mutations.items():
        text = (FORMAL / f"lean/{module}.lean").read_text()
        if text.count(before) != 1: raise RuntimeError(f"mutation {name} is stale")
        changed = build / f"Broken_{name.replace('-', '_')}.lean"
        changed.write_text(text.replace(before, after))
        result, _ = run([str(lean), str(changed)], out / f"lean-mutation-{name}.log", env=env)
        if result.returncode == 0 or "error:" not in result.stdout:
            raise RuntimeError(f"Lean failed to reject mutation {name}")
        print(f"Lean {name}: rejected", flush=True)
    for name, (before, after) in mutations.items():
        if source.count(before) != 1:
            raise RuntimeError(f"mutation {name} is stale")
        changed = build / f"Broken_{name.replace('-', '_')}.lean"
        changed.write_text(source.replace(before, after))
        result, _ = run([str(lean), str(changed)], out / f"lean-mutation-{name}.log", env=env)
        if result.returncode == 0 or "error:" not in result.stdout or not any(
                message in result.stdout for message in ["unsolved goals", "failed", "Type mismatch"]):
            raise RuntimeError(f"Lean failed to reject mutation {name}")
        print(f"Lean {name}: rejected", flush=True)
    print(f"Lean: {len(names)} theorems checked; only standard kernel axioms", flush=True)
    return {"theorems": names, "allowed_axioms": sorted(ALLOWED_AXIOMS),
            "rejected_mutations": list(mutations) + list(extra_mutations)}


def source_hashes():
    files = sorted(p for p in FORMAL.rglob("*") if p.is_file() and ".lake" not in p.parts)
    files.extend([Path(__file__).resolve(), ROOT / "mise.toml"])
    return {str(p.relative_to(ROOT)): digest(p) for p in files}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch-tools", action="store_true", help="download pinned, checksum-verified tools into ignored cache")
    parser.add_argument("--tla-jar", help="explicit pinned jar")
    parser.add_argument("--lean", help="explicit Lean 4.24.0 executable")
    parser.add_argument("--java", help="Java executable")
    args = parser.parse_args()
    os.nice(5)
    jar, lean, java, version = tools(args)
    results = CACHE / "results"
    results.mkdir(parents=True, exist_ok=True)
    out = Path(tempfile.mkdtemp(prefix="rooting-", dir=results))
    start = time.monotonic()
    before = source_hashes()
    try:
        tla = check_tla(jar, java, out)
        proof = check_lean(lean, out)
        after = source_hashes()
        if before != after:
            raise RuntimeError("formal inputs changed during verification; rerun on stable sources")
        summary = {"scope": "Abstract duplicate-root owner contract; no production compiler verification",
                   "lean_version": version, "tla_jar_sha256": digest(jar),
                   "lean_binary_sha256": digest(lean), "seconds": round(time.monotonic()-start, 3),
                   "source_sha256": after,
                   "tla": tla, "lean": proof}
        (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(f"Rooting verification passed: {out / 'summary.json'}")
    except (RuntimeError, subprocess.TimeoutExpired, OSError) as error:
        (out / "failure.txt").write_text(str(error) + "\n")
        raise


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.SubprocessError, OSError) as error:
        print(f"rooting verification failed: {error}", file=sys.stderr)
        sys.exit(1)
