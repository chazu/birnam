#!/usr/bin/env python3
"""74M: isolate native-tier cost with unchanged dispatch and separate diagnostics."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes

import argparse
import difflib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
BASE = "24e200a3"
NATIVE = "rust/birnam-id/src/runtime/native_tier.rs"
SCENARIOS = ("chain-1024", "chain-4096", "eval", "collections", "portable-collections", "materialize-cold", "steady-chain")
GROUPS = 7
EXTRA_INPUTS = ("wrapped/linenoise/liblinenoise.dylib", "wrapped/termbox2/libtermbox2.dylib")
spec = importlib.util.spec_from_file_location("tier_reference", ROOT / "scripts/runtime-native-tier.py")
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)


def run(command, *, cwd=ROOT, env=None, timeout=1800):
    result = subprocess.run(list(map(str, command)), cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {command}\n{result.stdout}{result.stderr}")
    return result.stdout


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def save():
    save_json(path, value)


def replace_once(source, before, after):
    assert source.count(before) == 1, before
    return source.replace(before, after)


def disabled(source):
    # Keep call sites and every ordinary compiler/dispatch path identical.
    begin = source.index("        self.staged.clear();")
    end = source.index("    pub(super) fn end_stage", begin)
    source = source[:begin] + "        self.staged.clear();\n    }\n\n" + source[end:]
    begin = source.index("    let mut registry = runtime.native_tier.borrow_mut();")
    end = source.index('\nunsafe extern "C" fn shim_send', begin)
    return source[:begin] + "    None\n}\n" + source[end:]


def profiled(source):
    source = "#[path = \"native_tier/profile.rs\"]\nmod profile;\n" + source.replace("//!", "//", 4)
    source = replace_once(source, "        let Ok(inventory)", "        profile::count(&profile::STAGES, 1);\n        let _preparation = profile::Timer(std::time::Instant::now(), &profile::PREPARE_NS);\n        let Ok(inventory)")
    source = replace_once(source, "            let prepared = (|| {", "            profile::count(&profile::ROWS, 1);\n            let prepared = (|| {")
    source = replace_once(source, "            self.staged.push(prepared.map(Rc::new));", "            if let Some(value) = &prepared {\n                profile::count(&profile::ELIGIBLE, 1);\n                profile::count(&profile::SOURCE_BYTES, value.source.as_bytes().len() as u64);\n            }\n            self.staged.push(prepared.map(Rc::new));")
    source = replace_once(source, "        self.prune();", "        profile::count(&profile::REGISTERED, 1);\n        self.prune();")
    source = replace_once(source, "    let mut registry = runtime.native_tier.borrow_mut();", "    profile::count(&profile::SELECTIONS, 1);\n    let mut registry = runtime.native_tier.borrow_mut();")
    source = replace_once(source, "        record.disabled |= record.candidate.is_some();", "        if record.candidate.is_some() { profile::count(&profile::INVALIDATIONS, 1); }\n        record.disabled |= record.candidate.is_some();")
    source = replace_once(source, "        record.candidate = prepared.compile().map(Rc::new);", "        {\n            profile::count(&profile::COMPILES, 1);\n            let _compilation = profile::Timer(std::time::Instant::now(), &profile::COMPILE_NS);\n            record.candidate = prepared.compile().map(Rc::new);\n            if record.candidate.is_none() { profile::count(&profile::FAILURES, 1); }\n        }")
    source = replace_once(source, "    Some(Selected {", "    profile::count(&profile::ENTRIES, 1);\n    Some(Selected {")
    source = replace_once(source, "        Some(Candidate {", "        profile::created();\n        Some(Candidate {")
    source += "\nimpl Drop for Candidate {\n    fn drop(&mut self) { profile::dropped(); }\n}\n"
    return source


def clean_env():
    return {key: value for key, value in os.environ.items() if not key.startswith(("BIRNAM_DEV_", "BENCH_HOST_", "NATIVE_TIER_", "BIRNAM_74J_"))}


def prepare(args):
    work = args.work_dir
    assert not (work / "builds.json").exists(), "use a fresh work directory for preparation"
    work.mkdir(parents=True, exist_ok=True)
    archive = work / "source.tar"
    run(["git", "archive", "--format=tar", f"--output={archive}", BASE])
    source = work / "source-on"
    source.mkdir(exist_ok=True)
    run(["tar", "-xf", archive, "-C", source])
    for name in EXTRA_INPUTS:
        (source / name).write_bytes((ROOT / name).read_bytes())
    original = (source / NATIVE).read_text()
    baseline = {"commit": run(["git", "rev-parse", BASE]).strip(), "archive_sha256": sha(archive),
                "prebuilt_wrapper_inputs": {name: sha(ROOT / name) for name in EXTRA_INPUTS}}
    on = args.on_prefix
    if on is None:
        on = work / "prefix-on"
        install(source, on, work / "build-on.log")
    evidence = {"on": inspect_prefix(on, source)}
    patches = {}
    for variant, transform in (("off", disabled), ("profile", profiled)):
        destination = work / f"source-{variant}"
        destination.mkdir(exist_ok=True)
        run(["tar", "-xf", archive, "-C", destination])
        for name in EXTRA_INPUTS:
            (destination / name).write_bytes((ROOT / name).read_bytes())
        changed = transform(original)
        (destination / NATIVE).write_text(changed)
        patches[variant] = "".join(difflib.unified_diff(original.splitlines(True), changed.splitlines(True), fromfile=NATIVE, tofile=NATIVE))
        (work / f"{variant}.patch").write_text(patches[variant])
        if variant == "profile":
            (destination / "rust/birnam-id/src/runtime/native_tier/profile.rs").write_bytes((ROOT / "scripts/fixtures/native-tier-profile.rs").read_bytes())
        prefix = work / f"prefix-{variant}"
        if not prefix.exists():
            install(destination, prefix, work / f"build-{variant}.log")
        evidence[variant] = inspect_prefix(prefix, destination)
    for variant in ("off", "profile"):
        assert evidence[variant]["exports"] == evidence["on"]["exports"]
        differences = sorted(name for name, digest in evidence["on"]["binary_sha256"].items() if evidence[variant]["binary_sha256"].get(name) != digest)
        evidence[variant]["raw_build_binary_differences"] = differences
    # Native and stage1 LC_ID_DYLIB names can contain temporary build paths.
    # Compose diagnostic/measurement prefixes with one identical binary closure
    # and replace only the floor. Preserve original build receipts separately;
    # the composed prefix is explicitly labelled and has no install receipt.
    for variant in ("on", "off", "profile"):
        destination = work / f"run-{variant}"
        shutil.copytree(on, destination)
        original_prefix = Path(evidence[variant]["path"])
        shutil.copy2(original_prefix / "lib/libbirnam_id.dylib", destination / "lib/libbirnam_id.dylib")
        (destination / "share/birnam/rust-install.json").unlink()
        save(destination / "share/birnam/native-tier-focus.json", {
            "schema": "birnam-native-tier-experiment-composition-v1", "variant": variant,
            "common_prefix": str(on), "floor_build_prefix": str(original_prefix),
            "floor_sha256": evidence[variant]["floor_sha256"],
            "scope": "experiment composition; not a canonical installed-prefix receipt"})
        evidence[variant]["measurement_path"] = str(destination)
        binaries = {str(path.relative_to(destination)): sha(path) for folder in ("bin", "lib") for path in sorted((destination / folder).rglob("*")) if path.is_file()}
        assert [name for name, digest in binaries.items() if digest != evidence["on"]["binary_sha256"][name]] == ([] if variant == "on" else ["lib/libbirnam_id.dylib"])
        evidence[variant]["measurement_binary_sha256"] = binaries
    helper = work / "lookup.dylib"
    run(["cc", "-dynamiclib", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", ROOT / "abi/fixtures/idapi-v1", ROOT / "scripts/fixtures/internal-guarded-sends.c", "-o", helper])
    result = {"schema": "birnam-native-tier-isolation-builds-v1", "baseline": baseline,
              "prefixes": evidence, "patches": patches, "profile_source_sha256": sha(ROOT / "scripts/fixtures/native-tier-profile.rs"),
              "helper_sha256": sha(helper), "binary_isolation": "only lib/libbirnam_id.dylib differs"}
    save(work / "builds.json", result)
    return result


def install(source, prefix, log):
    print(f"native-tier-focus: build {prefix.name}", file=sys.stderr, flush=True)
    with log.open("w") as output:
        subprocess.run(["bash", ROOT / "scripts/rust-install.sh", "install", "--root", source, "--prefix", prefix, "--profile", "release"],
                       cwd=ROOT, env=clean_env(), stdout=output, stderr=subprocess.STDOUT, check=True)


def inspect_prefix(prefix, source):
    evidence = reference.prefix(prefix)
    manifest = reference.source_manifest(source)
    digest = hashlib.sha256(json.dumps(manifest, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()).hexdigest()
    assert evidence["receipt"]["source_manifest_sha256"] == digest
    evidence["path"] = str(prefix)
    evidence["source_manifest"] = manifest
    evidence["binary_sha256"] = {str(path.relative_to(prefix)): sha(path) for folder in ("bin", "lib") for path in sorted((prefix / folder).rglob("*")) if path.is_file()}
    return evidence


def scenario(args, builds, variant, name):
    env = clean_env()
    if name.startswith("chain-"):
        env.update(NATIVE_TIER_ITERATIONS=name.split("-")[1], BIRNAM_74J_HELPER=str(args.work_dir / "lookup.dylib"), BIRNAM_74J_MODE="fallback")
        fixture = "bench/runtime-native-tier.birnam"
    elif name == "steady-chain":
        fixture = "bench/native-tier-steady.birnam"
    else:
        env["NATIVE_TIER_FOCUS"] = name
        fixture = "bench/native-tier-focus.birnam"
    binary = Path(builds["prefixes"][variant]["measurement_path"]) / "bin/birnam"
    usage = args.work_dir / "rusage.txt"
    start = time.perf_counter_ns()
    result = subprocess.run(["/usr/bin/time", "-l", "-o", usage, binary, fixture], cwd=ROOT, env=env, capture_output=True, text=True, timeout=600)
    wall_us = (time.perf_counter_ns() - start) // 1000
    assert result.returncode == 0, f"{variant} {name}: {result.returncode}\n{result.stdout}{result.stderr}"
    lines = result.stdout.splitlines()
    keys = ("materialize-us", "first-us", "warmup-us", "warm-us", "fallback-us", "sample-us")
    samples = {key: [int(line.split()[1]) for line in lines if line.startswith(key + " ")] for key in keys}
    samples = {key: values for key, values in samples.items() if values}
    if name.startswith("chain-"):
        assert "tier-ready" in lines and "tier-mutation-and-eviction-ok" in lines and "74j-mode fallback" in lines
        assert set(samples) == set(keys) - {"sample-us"}
    else:
        assert f"focus-ok {name}" in lines and len(samples["sample-us"]) == 5
    usage_text = usage.read_text()
    rss = int(re.search(r"(\d+)\s+maximum resident set size", usage_text).group(1))
    values = {key: statistics.median(sample) for key, sample in samples.items()}
    values.update(wall_us=wall_us, max_rss_bytes=rss)
    profiles = [json.loads(line.removeprefix("NATIVE_TIER_PROFILE ")) for line in result.stderr.splitlines() if line.startswith("NATIVE_TIER_PROFILE ")]
    assert len(profiles) == (1 if variant == "profile" else 0), result.stderr
    return {"scenario": name, "values": values, "samples": samples, "stdout": result.stdout, "stderr": result.stderr, "resource_usage": usage_text, "profile": profiles}


def classify(groups):
    rows = []
    for scenario_name in SCENARIOS:
        metrics = groups[0]["on"][scenario_name]["values"]
        for metric in metrics:
            samples = {v: [group[v][scenario_name]["values"][metric] for group in groups] for v in ("off", "on")}
            medians = {v: statistics.median(values) for v, values in samples.items()}
            ranges = {v: max(values) - min(values) for v, values in samples.items()}
            difference = medians["off"] - medians["on"]
            spread = max(ranges.values())
            label = "improvement" if difference > spread else "regression" if -difference > spread else "inconclusive"
            rows.append({"scenario": scenario_name, "metric": metric, "samples": samples, "medians": medians, "ranges": ranges, "difference": difference, "classification": label})
    return rows


def diagnose(args, builds):
    observations = []
    for repetition in range(3):
        for name in SCENARIOS:
            print(f"native-tier-focus: diagnostic {repetition + 1}/3 {name}", file=sys.stderr, flush=True)
            row = scenario(args, builds, "profile", name)
            row["repetition"] = repetition + 1
            observations.append(row)
    assert any(row["profile"][0]["entries"] > 0 for row in observations), "no independently observed activation"
    assert all(row["profile"][0]["entries"] >= 120000 for row in observations if row["scenario"] == "steady-chain"), "steady harness did not retain native execution into measurement"
    return observations


def measure(args, builds, receipt):
    receipt["groups"] = []
    for index in range(GROUPS):
        print(f"native-tier-focus: host gate and group {index + 1}/{GROUPS}", file=sys.stderr, flush=True)
        group = {"number": index + 1, "order": ["off", "on"] if index % 2 == 0 else ["on", "off"]}
        receipt["groups"].append(group)
        group["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", f"74M group {index + 1}"], env=clean_env())
        monitor_path = args.work_dir / "monitor.txt"
        with monitor_path.open("w") as output:
            monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0"], stdout=output, stderr=subprocess.DEVNULL)
            try:
                for variant in group["order"]:
                    group[variant] = {}
                    for name in SCENARIOS:
                        print(f"native-tier-focus: {variant} {name}", file=sys.stderr, flush=True)
                        group[variant][name] = scenario(args, builds, variant, name)
            finally:
                monitor.terminate()
                monitor.wait(timeout=10)
        group["monitor"] = monitor_path.read_text()
        group["interval_gate"] = run(["bash", "scripts/bench-host-check.sh", "--samples", monitor_path, f"74M group {index + 1}"], env=clean_env())
    receipt["comparisons"] = classify(receipt["groups"])
    receipt["result"] = "completed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--on-prefix", type=Path)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--diagnose", action="store_true")
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    args.work_dir = args.work_dir.resolve()
    if args.on_prefix:
        args.on_prefix = args.on_prefix.resolve()
    assert platform.system() == "Darwin"
    assert args.prepare or args.diagnose or args.measure
    if args.prepare:
        prepare(args)
    builds = load_json(args.work_dir / "builds.json")
    for evidence in builds["prefixes"].values():
        assert sha(Path(evidence["path"]) / "lib/libbirnam_id.dylib") == evidence["floor_sha256"]
        for name, digest in evidence["measurement_binary_sha256"].items():
            assert sha(Path(evidence["measurement_path"]) / name) == digest
    source_paths = ["scripts/native-tier-focus.py", "scripts/runtime-native-tier.py", "scripts/fixtures/native-tier-profile.rs", "scripts/fixtures/internal-guarded-sends.c", "scripts/bench-host-check.sh", "bench/native-tier-focus.birnam", "bench/native-tier-steady.birnam", "bench/runtime-native-tier.birnam"]
    receipt = {"schema": "birnam-native-tier-focus-v1", "design": "74M", "result": "prepared", "builds": builds,
               "host": {"system": platform.system(), "machine": platform.machine()},
               "source_sha256": {name: sha(ROOT / name) for name in source_paths},
               "predeclared_protocol": (ROOT / "docs/design/74m-native-tier-performance-isolation.md").read_text(),
               "protocol": {"groups": GROUPS, "scenarios": SCENARIOS, "timed_repetitions": 5,
                            "criterion": "median difference exceeds both process sample ranges; cold/setup, wall and RSS classified separately",
                            "diagnostics": "separate instrumented source snapshot; never a timing comparison"}}
    output = args.output or args.work_dir / "receipt.json"
    try:
        if args.diagnose:
            receipt["diagnostics"] = diagnose(args, builds)
            receipt["result"] = "diagnostics-completed"
        if args.measure:
            measure(args, builds, receipt)
    except Exception as error:
        receipt["result"] = "incomplete-or-rejected"
        receipt["error"] = str(error)
        raise
    finally:
        save(output, receipt)
    print(f"native-tier-focus: {receipt['result']}; {output}", file=sys.stderr)


if __name__ == "__main__":
    main()
