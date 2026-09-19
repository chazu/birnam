#!/usr/bin/env python3
"""74T recovery and numeric preparation evidence; no timing claim."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import tempfile
from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
BASE = "ff71ec23"
AUTHORITIES = (
    "rust/birnam-artifact/src/frame_descriptor.rs", "rust/birnam-id/src/registration.rs",
    "rust/birnam-codegen/src/resumable_c.rs", "rust/birnam-image/src/lib.rs", "rust/birnam-image/src/v8.rs",
)
CURRENT_IDAPI = ("rust/birnam-abi/src/lib.rs", "abi/idapi-v5.json",
                 "abi/fixtures/idapi-v5/idapi_v5.h")


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def sources():
    # This runner executes recovery/compiler tests and native admission fixtures.
    # Timing drivers, publication/cold gates and their output settings have their
    # own phase bindings; editing them must not invalidate this unit proof.
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', Path(__file__).resolve()]
    paths += [ROOT / 'scripts' / name for name in
              ('compatibility_authority.py', 'compatibility-authority-test.py', 'measurement_archive.py',
               'validation_support.py', 'validation_proof.py')]
    paths += [ROOT / 'scripts/fixtures' / name for name in
              ('native-block-construction.c', 'closure-api-context.c', 'object-model-support.h', 'integer-support.h')]
    for folder in ('rust', 'src/id', 'modules', 'abi', 'vendor/libtcc-src'):
        paths.extend(p for p in (ROOT / folder).rglob('*') if p.is_file())
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(set(paths))}


def check_numeric_closure_matrices(matrices):
    sequence_source = runpy.run_path(str(ROOT / "scripts/closure-sequence-smoke.py"))["source"]
    measure_module = runpy.run_path(str(ROOT / "scripts/closure_timing_common.py"))
    action_source = runpy.run_path(str(ROOT / "scripts/closure-numeric-actions-smoke.py"))["source"]
    budget_source = runpy.run_path(str(ROOT / "scripts/closure-budget-smoke.py"))["source"]
    modes = [(0, 1, -1), (1, 0, -1), (1, 1, -1), (1, 1, 0), (1, 1, 1), (1, 1, 2)]
    expected_source = {(p, l, e, f) for p in (0, 1) for l, e, f in modes}
    expected_control = {(p, c, l, e, f) for p, l, e, f in expected_source if l == 1
                        for c in ("capture", "suspend")}
    expected_inputs = {(p, e, f) for p in (0, 1) for e, f in ((0, -1), (1, -1), (1, 0), (1, 1), (1, 2))}
    expected_sequence = {(p, n, escape, enabled, force) for p in (0, 1)
                         for n in (1, 2, 3, 32, 33) for escape in (False, True)
                         for enabled, force in [(False, -1), (True, -1)] + [(True, f) for f in range(min(n, 32))]}
    digest = lambda source: hashlib.sha256(source.encode()).hexdigest()
    for profile in ("debug", "release"):
        def rows(name, keys, expected):
            values = matrices[f"numeric-{name}-{profile}.json"]["rows"]
            if len(values) != len(expected) or {tuple(row[k] for k in keys) for row in values} != expected:
                raise RuntimeError(f"incomplete numeric closure {name} matrix")
            return values
        rows("source", ("precise", "literals", "enabled", "forced"), expected_source)
        rows("control", ("precise", "control", "literals", "enabled", "forced"), expected_control)
        rows("inputs", ("precise", "enabled", "forced"), expected_inputs)
        expected_measures = {(u, s, e) for u in (1, 3, 4, 8, 16, 32) for s in range(4) for e in (0, 1)}
        measures = rows("measure", ("uses", "shape", "enabled"), expected_measures)
        for row in measures:
            if row["source_sha256"] != digest(measure_module["source"](row["uses"], numeric=True)):
                raise RuntimeError("numeric closure measurement used a different body")
        for uses, shape in {(u, s) for u, s, _ in expected_measures}:
            pair = {r["enabled"]: r["values"] for r in measures if r["uses"] == uses and r["shape"] == shape}
            measure_module["check_pair"](pair[0], pair[1], 2, numeric=True)

        expected_actions = {(tail, captures, p, e, f) for tail in (False, True)
                            for captures in (False, True) for p, e, f in expected_inputs}
        actions = rows("actions", ("tail", "captures", "precise", "enabled", "forced"), expected_actions)
        baselines = {(r["tail"], r["captures"], r["precise"]): r["snapshots"] for r in actions if not r["enabled"]}
        for row in actions:
            if row["source_sha256"] != digest(action_source(row["tail"], row["captures"])):
                raise RuntimeError("numeric closure action probe used a different body")
            events = row["snapshots"]
            if len(events) != 3 or any([len(a) for a in e] != [4, 9, 8, 5, 7, 7, 6, 7, 4] or e[-1][2:] != [1, 0] for e in events):
                raise RuntimeError("numeric closure action snapshot is incomplete")
            if events != baselines[row["tail"], row["captures"], row["precise"]]:
                raise RuntimeError("numeric closure residual actions differ")

        for name, fixture in (("source", "closure-numeric.birnam"), ("control", "closure-numeric-control.birnam")):
            if matrices[f"numeric-{name}-{profile}.json"]["source_sha256"].get(f"scripts/fixtures/{fixture}") != sha(ROOT / "scripts/fixtures" / fixture):
                raise RuntimeError("numeric closure fixture differs from tested source")
        for row in rows("sequence", ("precise", "calls", "escaping", "enabled", "force"), expected_sequence):
            if row["source_sha256"] != digest(sequence_source(row["calls"], row["escaping"], row["precise"], numeric=True)):
                raise RuntimeError("numeric closure sequence used a different body")
        budget = matrices[f"numeric-budget-{profile}.json"]["rows"]
        expected_budget = set()
        for precise in (0, 1):
            for calls in (3, 32):
                for escaping in (False, True):
                    initial = [row for row in budget if (row["precise"], row["calls"], row["escaping"], row["gap"], row["enabled"]) == (precise, calls, escaping, 65536, 0)]
                    if len(initial) != 1 or initial[0]["values"]["collections"] != 0:
                        raise RuntimeError("numeric budget has no complete baseline extent")
                    values = initial[0]["values"]
                    extent = values["remaining_bytes"] - (values["threshold"] - 65536)
                    gaps = {65536, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, extent - 8, extent, extent + 8}
                    expected_budget.update((precise, calls, escaping, gap, enabled) for gap in gaps for enabled in (0, 1))
        for row in rows("budget", ("precise", "calls", "escaping", "gap", "enabled"), expected_budget):
            if row["source_sha256"] != digest(budget_source(row["calls"], row["escaping"], row["precise"], row["gap"], numeric=True)):
                raise RuntimeError("numeric closure budget used a different body")


def check_joint_numeric_closure_pair(family, off, on):
    """Admit complete joint matrices and compare observable state across counting modes."""
    def require(condition, detail):
        if not condition:
            raise RuntimeError(f"joint numeric closure {family}: {detail}")

    require(off['floor_sha256'] == on['floor_sha256'], 'different diagnostic floors')
    for binding in ('installed_sha256', 'artifacts'):
        require(off.get(binding) == on.get(binding), 'different installed artifacts')
    keys = {
        'sequence': ('precise', 'calls', 'escaping', 'enabled', 'force'),
        'budget': ('precise', 'calls', 'escaping', 'gap', 'enabled'),
        'control': ('precise', 'control', 'literals', 'enabled', 'forced'),
        'inputs': ('precise', 'enabled', 'forced'),
    }[family]
    indexed = []
    for counting, receipt in ((False, off), (True, on)):
        require(receipt['status'] == 'passed', 'matrix did not pass')
        require(receipt.get('numeric_copy_region') is True and
                receipt.get('counted_frame_indices') is counting, 'wrong body or counting mode')
        rows = {tuple(row[key] for key in keys): row for row in receipt['rows']}
        require(len(rows) == len(receipt['rows']), 'duplicate coordinates')
        modes = ((0, -1), (1, -1), (1, 0), (1, 1), (1, 2))
        if family == 'sequence':
            expected = {(p, n, e, enabled, force) for p in (0, 1) for n in (1, 2, 3, 32, 33)
                        for e in (False, True) for enabled, force in
                        [(0, -1), (1, -1)] + [(1, f) for f in range(min(n, 32))]}
        elif family == 'control':
            expected = {(p, c, 1, enabled, force) for p in (0, 1)
                        for c in ('capture', 'suspend') for enabled, force in modes}
        elif family == 'inputs':
            expected = {(p, enabled, force) for p in (0, 1) for enabled, force in modes}
        else:
            expected = set()
            for p in (0, 1):
                for n in (3, 32):
                    for e in (False, True):
                        baseline = rows.get((p, n, e, 65536, 0))
                        require(baseline is not None, 'missing baseline budget extent')
                        values = baseline['values']
                        extent = values['remaining_bytes'] - (values['threshold'] - 65536)
                        require(values['collections'] == 0 and 64 < extent < 65528 and extent % 8 == 0,
                                'invalid baseline budget extent')
                        gaps = {65536, 64, 128, 256, 512, 1024, 2048, 4096, 8192,
                                16384, 32768, extent - 8, extent, extent + 8}
                        expected.update((p, n, e, gap, enabled) for gap in gaps for enabled in (0, 1))
                        group = [r['values'] for key, r in rows.items() if key[:3] == (p, n, e)]
                        completed = {v['completed_calls'] for v in group}
                        require(0 in completed and n in completed and any(0 < c < n for c in completed),
                                'missing budget recovery prefixes')
                        require({v['collections'] for v in group} == {0, 1}, 'missing GC boundary')
                        if counting:
                            require(any(v['counted_recorded'] > 0 for v in group), 'no counted budget work')
        require(set(rows) == expected, 'incomplete coordinates')
        if family in ('sequence', 'budget'):
            generator = runpy.run_path(str(ROOT / f'scripts/closure-{family}-smoke.py'))['source']
            for row in rows.values():
                args = [row['calls'], row['escaping'], row['precise']]
                if family == 'budget':
                    args.append(row['gap'])
                body = generator(*args, numeric=True, numeric_copy_region=True)
                require(row['source_sha256'] == hashlib.sha256(body.encode()).hexdigest(), 'different source body')
                v = row['values']
                indices = v['counted_indices']
                require(0 <= indices <= v['physical_saved'], 'invalid counted savings')
                if family == 'budget':
                    require(indices <= v['counted_recorded'], 'missing counted events')
                    if not counting:
                        require(v['counted_recorded'] == 0, 'counted events while disabled')
                require((indices > 0 if counting and family == 'sequence' else True) and
                        (counting or indices == 0), 'counting mode was not exercised')
        else:
            sources = receipt['sources' if family == 'inputs' else 'source_sha256']
            fixture = f'scripts/fixtures/closure-counted-numeric-{family}.birnam'
            require(sources.get(fixture) == sha(ROOT / fixture), 'different source fixture')
            require(all(sha(ROOT / name) == digest for name, digest in sources.items()), 'stale source hashes')
            for row in rows.values():
                if family == 'inputs':
                    require(row['exit_code'] == 0, 'input fixture failed')
                else:
                    v = row['counted']
                    require(set(v) == {'before_pause', 'after_resume'}, 'missing control counters')
                    require((v['before_pause'] > 0 and v['after_resume'] >= v['before_pause']) if counting
                            else v['before_pause'] == v['after_resume'] == 0, 'counted work absent at pause')
                    raw = (row['stdout'] + row['stderr']).splitlines()
                    counter_lines = [line for line in raw if line.startswith('closure-counted-control: ')]
                    require(counter_lines == [f"closure-counted-control: before_pause={v['before_pause']} after_resume={v['after_resume']}"],
                            'control counters differ from actual output')
                    normalized = [re.sub(r'calls=\d+', 'calls=CHECKED', line) for line in raw
                                  if not line.startswith(('closure-elision:', 'closure-counted-control: '))]
                    require(normalized == row['output'], 'control output differs from actual output')
        indexed.append(rows)
    require(indexed[0].keys() == indexed[1].keys(), 'counting modes have different coordinates')
    for key, baseline in indexed[0].items():
        candidate = indexed[1][key]
        if family in ('sequence', 'budget'):
            def observable(row):
                values = dict(row['values'])
                values['physical_saved'] -= values.pop('counted_indices')
                values.pop('counted_recorded', None)
                return values
            require(observable(baseline) == observable(candidate), f'counting changed accounting or recovery at {key}')
        elif family == 'control':
            require((baseline['output'], baseline['completed_calls']) ==
                    (candidate['output'], candidate['completed_calls']), f'counting changed V2 behavior at {key}')
        else:
            require((baseline['stdout'], baseline['stderr']) == (candidate['stdout'], candidate['stderr']),
                    f'counting changed current inputs or effects at {key}')


def check_closure_string_cleanup_matrix(receipt):
    def require(condition, detail):
        if not condition:
            raise RuntimeError('closure String cleanup: ' + detail)
    require(receipt['status'] == 'passed' and receipt.get('quick') is False, 'incomplete matrix')
    keys = ('precise', 'calls', 'escaping', 'cleanup', 'enabled', 'force', 'counted')
    expected = {(p, n, e, c, enabled, force, counting) for p in (0, 1) for n in (3, 33)
                for e in (False, True) for c in (1, 2, 3) for counting in (False, True)
                for enabled, force in ((False, -1), (True, -1), (True, 0), (True, 1), (True, min(n, 32) - 1))}
    rows = {tuple(r[k] for k in keys): r for r in receipt['rows']}
    require(len(rows) == len(receipt['rows']) == len(expected) and set(rows) == expected, 'wrong coordinates')
    require(all(sha(ROOT / n) == h for n, h in receipt['sources'].items()), 'stale sources')
    source = runpy.run_path(str(ROOT / 'scripts/closure-sequence-smoke.py'))['source']
    for key, row in rows.items():
        require(row['exit_code'] == 0, 'failed process')
        body = source(row['calls'], row['escaping'], row['precise'], numeric=True, numeric_copy_region=True)
        require(row['source_sha256'] == hashlib.sha256(body.encode()).hexdigest(), 'different body')
        for prefix, field in (('closure-sequence ', 'sequence'), ('closure-cleanup ', 'collection')):
            lines = [s for s in row['stdout'].splitlines() if s.startswith(prefix)]
            require(len(lines) == 1, 'missing diagnostic')
            require({k: int(v) for k, v in re.findall(r'(\w+)=(\d+)', lines[0])} == row[field], 'diagnostic differs from raw output')
        v, c = row['sequence'], row['collection']
        require(v['counted_indices'] > 0 if row['counted'] else v['counted_indices'] == 0, 'counting not exercised')
        require(c['pending'] == v['physical_saved'] and c['settled'] == (0 if row['cleanup'] == 1 else c['pending']), 'incorrect settlement')
        require(c['mode'] == row['cleanup'] and c['callbacks'] == int(row['cleanup'] != 1) and c['string_released'] == 1, 'cleanup not exercised')
        require(c['reclaimed'] > 0 and c['after_live'] == c['before_live'] - c['reclaimed'], 'allocation conservation')
        if row['cleanup'] == 1:
            require(c['callback_live'] == c['callback_bytes'] == 0, 'unexpected observing callback')
        else:
            require(c['callback_live'] == c['after_live'] + c['remaining_dead'] and
                    c['callback_bytes'] == c['after_bytes'] + c['remaining_dead_bytes'], 'callback sweep accounting')
        if row['counted']:
            baseline = rows[(*key[:-1], False)]
            def accounting(row):
                sequence = dict(row['sequence'])
                sequence['physical_saved'] -= sequence.pop('counted_indices')
                collection = {k: v for k, v in row['collection'].items() if k not in
                              ('pending', 'settled', 'callback_live', 'callback_bytes', 'remaining_dead', 'remaining_dead_bytes')}
                return sequence, collection
            require(accounting(baseline) == accounting(row), 'counting changed logical observations')


def check_baseline_storage_matrix(storage):
    if storage.get('status') != 'passed' or storage.get('heap_accounting') is not True:
        raise RuntimeError('baseline TCC compiler heap accounting was not exercised')
    expected = {(profile, size, repetition) for profile in ('debug', 'release')
                for size in (4096, 65536) for repetition in range(3)}
    if len(storage['rows']) != 12 or {(r['profile'], r['payload_bytes'], r['repetition']) for r in storage['rows']} != expected:
        raise RuntimeError('incomplete baseline TCC storage lifecycle matrix')
    checks = runpy.run_path(str(ROOT / 'scripts/baseline-native-storage-smoke.py'))
    for row in storage['rows']:
        source = checks['fixture'](row['payload_bytes'], True)
        if row['source_sha256'] != hashlib.sha256(source.encode()).hexdigest():
            raise RuntimeError('baseline TCC storage source differs')
        measurements = checks['validate_row'](row, True)
        if any(row.get(name) != value for name, value in measurements.items()):
            raise RuntimeError('baseline TCC storage observations differ from raw output')


def verify_complete_matrices(evidence, include_unboxed=True):
    """Preserved full coordinate and semantic oracles; no build or subprocess."""
    evidence = Path(evidence)
    unboxed_evidence = evidence
    receipt = {}
    output = ''.join(sha(ROOT / '.cargo-target/74t-closure-control-target' / profile / 'libbirnam_id.dylib')
                     for profile in ('debug', 'release'))
    expected = {f"{family}-{profile}.json" for family in ("sequence", "mutation", "budget", "journal", "measure", "literals", "literal-control",
                                                                      "numeric-source", "numeric-control", "numeric-sequence", "numeric-budget", "numeric-inputs", "numeric-string-cleanup", "numeric-actions", "numeric-measure")
                for profile in ("debug", "release")}
    expected.update(f"joint-{family}-{counting}-{profile}.json"
                    for family in ('sequence', 'budget', 'control', 'inputs')
                    for counting in ('off', 'on') for profile in ('debug', 'release'))
    expected.update({"evaluator-lifetime.json", "baseline-native-storage.json"})
    paths = [p for p in Path(evidence).glob("*.json") if p.name in expected]
    if {p.name for p in paths} != expected:
        raise RuntimeError("missing source closure matrix receipts")
    matrices = {p.name: load_json(p) for p in sorted(paths)}
    if any(m["status"] != "passed" for m in matrices.values()):
        raise RuntimeError("source closure matrix did not pass")
    for profile in ("debug", "release"):
        hashes = {m["floor_sha256"] for name, m in matrices.items() if name.endswith(f"-{profile}.json")}
        if len(hashes) != 1 or next(iter(hashes)) not in output:
            raise RuntimeError("source matrix floor differs from control verification")
    expected_measurements = {(u, s, e) for u in (1, 3, 4, 8, 16, 32) for s in range(4) for e in (0, 1)}
    for profile in ("debug", "release"):
        rows = matrices[f"measure-{profile}.json"]["rows"]
        if len(rows) != 48 or {(r["uses"], r["shape"], r["enabled"]) for r in rows} != expected_measurements:
            raise RuntimeError("incomplete closure profitability-threshold measurement matrix")
    expected_literals = {(p, l, e, f) for p in (0, 1)
                         for l, e, f in ((0, 1, -1), (1, 0, -1), (1, 1, -1),
                                         (1, 1, 0), (1, 1, 1), (1, 1, 2))}
    for profile in ("debug", "release"):
        rows = matrices[f"literals-{profile}.json"]["rows"]
        if len(rows) != 12 or {(r["precise"], r["literals"], r["enabled"], r["forced"]) for r in rows} != expected_literals:
            raise RuntimeError("incomplete fresh literal closure result matrix")
    expected_literal_control = {(p, c, l, e, f) for p, l, e, f in expected_literals
                                for c in ("capture", "suspend")}
    for profile in ("debug", "release"):
        rows = matrices[f"literal-control-{profile}.json"]["rows"]
        if len(rows) != 24 or {(r["precise"], r["control"], r["literals"], r["enabled"], r["forced"]) for r in rows} != expected_literal_control:
            raise RuntimeError("incomplete literal result capture/suspension matrix")
    check_numeric_closure_matrices(matrices)
    for profile in ('debug', 'release'):
        check_closure_string_cleanup_matrix(matrices[f'numeric-string-cleanup-{profile}.json'])
        for family in ('sequence', 'budget', 'control', 'inputs'):
            check_joint_numeric_closure_pair(family,
                matrices[f'joint-{family}-off-{profile}.json'],
                matrices[f'joint-{family}-on-{profile}.json'])
    lifetime = matrices.pop("evaluator-lifetime.json")
    expected_lifetime = {(profile, case) for profile in ("debug", "release")
                         for case in ("probe-root-boundary", "logicvar-original", "error-original",
                                      "nested-sync", "nested-sync-block", "nested-persisting")}
    if len(lifetime["rows"]) != 12 or {(r["profile"], r["case"]) for r in lifetime["rows"]} != expected_lifetime:
        raise RuntimeError("incomplete evaluator/native root lifetime matrix")
    for profile in ("debug", "release"):
        floor = ROOT / ".cargo-target/74t-closure-control-target" / profile / "libbirnam_id.dylib"
        if lifetime["artifacts"].get(str(floor.resolve())) != matrices[f"sequence-{profile}.json"]["floor_sha256"]:
            raise RuntimeError("evaluator lifetime floor differs from source matrix floor")
    receipt["evaluator_lifetime_matrix"] = lifetime
    baseline_storage = matrices.pop("baseline-native-storage.json")
    check_baseline_storage_matrix(baseline_storage)
    receipt["baseline_native_storage_matrix"] = baseline_storage
    receipt["closure_source_matrices"] = matrices
    if include_unboxed:
        expected_unboxed = {f"{family}-{p}.json" for family in ("unboxed", "unboxed-captured", "unboxed-control", "unboxed-measure", "loop-invariant", "loop-measure") for p in ("debug", "release")}
        paths = [p for p in unboxed_evidence.glob("*.json") if p.name in expected_unboxed]
        if {p.name for p in paths} != expected_unboxed:
            raise RuntimeError("missing source unboxed matrix receipts")
        unboxed = {p.name: load_json(p) for p in sorted(paths)}
        for profile in ("debug", "release"):
            matrix = unboxed[f"unboxed-{profile}.json"]
            expected_floor = matrices[f"sequence-{profile}.json"]["floor_sha256"]
            if matrix["status"] != "passed" or matrix["floor_sha256"] != expected_floor:
                raise RuntimeError("source unboxed matrix failed or floor differs")
            if len(matrix["rows"]) != 146 or not matrix["production_probe_absent"]:
                raise RuntimeError("incomplete source unboxed matrix")
            captured = unboxed[f"unboxed-captured-{profile}.json"]
            if captured["status"] != "passed" or captured["floor_sha256"] != expected_floor:
                raise RuntimeError("source captured numeric matrix failed or floor differs")
            if len(captured["rows"]) != 146 or not captured["production_probe_absent"] or not captured["captured_inputs"]:
                raise RuntimeError("incomplete source captured numeric matrix")
            control = unboxed[f"unboxed-control-{profile}.json"]
            if control["status"] != "passed" or control["floor_sha256"] != expected_floor:
                raise RuntimeError("source unboxed control matrix failed or floor differs")
            if len(control["rows"]) != 448:
                raise RuntimeError("incomplete source unboxed control matrix")
            measure = unboxed[f"unboxed-measure-{profile}.json"]
            if measure["status"] != "passed" or measure["floor_sha256"] != expected_floor:
                raise RuntimeError("source unboxed measurement matrix failed or floor differs")
            expected_rows = {(n, s, e) for n in (3, 16, 32) for s in range(5) for e in (0, 1)}
            if len(measure["rows"]) != 30 or {(r["operations"], r["shape"], r["enabled"]) for r in measure["rows"]} != expected_rows:
                raise RuntimeError("incomplete source unboxed measurement matrix")
            costs = unboxed[f"loop-measure-{profile}.json"]
            if costs["status"] != "passed" or costs["floor_sha256"] != expected_floor:
                raise RuntimeError("source loop measurement matrix failed or floor differs")
            expected_costs = {(n, s, p, u, e) for n in (3, 16, 32) for s in range(4) for p in (0, 1) for u in (0, 1) for e in (0, 1)}
            if len(costs["rows"]) != 96 or {(r["operations"], r["shape"], r["precise"], r["unboxed"], r["enabled"]) for r in costs["rows"]} != expected_costs:
                raise RuntimeError("incomplete source loop measurement matrix")
            loop = unboxed[f"loop-invariant-{profile}.json"]
            if loop["status"] != "passed" or loop["floor_sha256"] != expected_floor:
                raise RuntimeError("source loop invariant matrix failed or floor differs")
            expected_loop = {(s, p, u, e) for s in range(4) for p in (0, 1) for u in (0, 1) for e in (0, 1)}
            if len(loop["rows"]) != 32 or {(r["shape"], r["precise"], r["unboxed"], r["enabled"]) for r in loop["rows"]} != expected_loop:
                raise RuntimeError("incomplete source loop invariant matrix")
        receipt["unboxed_source_matrices"] = unboxed
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true", help="Validate retained complete matrices; never launch nested verification")
    parser.add_argument("--matrix-directory", type=Path, help="Complete retained exhaustive matrices from validate.py")
    parser.add_argument("--validation-receipt", type=Path, help="Passed release plan supplying production evidence")
    parser.add_argument("--all-families", action="store_true", help="Explicitly select every family")
    parser.add_argument("--unboxed", action="store_true", help="experimental unboxed source and V2 recovery verification")
    parser.add_argument("--numeric", action="store_true")
    parser.add_argument("--chains", action="store_true", help="production raw-chain verification")
    parser.add_argument("--tails", action="store_true", help="registered tail-loop entry verification")
    parser.add_argument("--allocations", action="store_true", help="discarded aggregate allocation and GC verification")
    parser.add_argument("--fields", action="store_true", help="live aggregate projection and physical recovery verification")
    parser.add_argument("--closures", action="store_true", help="experimental source closure elimination and recovery verification")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.verify and (not args.matrix_directory or not args.validation_receipt):
        parser.error("--verify now consumes --matrix-directory and --validation-receipt; run validate.py --suite release first")
    if args.all_families:
        args.unboxed = args.closures = args.fields = args.allocations = True
        args.tails = args.chains = args.numeric = True
    if args.output is None:
        args.output = ROOT / ".cargo-target/measurements" / (
            "74v-unboxed-numeric.json" if args.unboxed else
            "74t-closure-plans.json" if args.closures else
            "74t-virtual-fields.json" if args.fields else
            "74t-discarded-clones.json" if args.allocations else
            "74t-tail-entry.json" if args.tails else
            "74t-numeric-chains.json" if args.chains else
            "74t-numeric-preparation.json" if args.numeric else "74t-recovery-graphs.json")
    work = ROOT / ".cargo-target/74t-recovery"
    work.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    source_inputs = sources()
    receipt = {
        "design": "74T", "status": "running", "sources": dict(source_inputs), "commands": [],
        "scope": ("Experimental source numeric box elimination and V2 identity recovery; default-off and broader acceptance pending"
                  if args.unboxed else "Experimental source closure allocation elimination, guarded capture substitution and identity recovery; disabled by default and broader acceptance pending"
                  if args.closures else "Bounded production live aggregate field projection and identity recovery; broader acceptance remains open"
                  if args.fields else "Bounded repeated discarded-clone consumer and dead-allocation accounting; live aggregate replacement and remaining family obligations in progress"
                  if args.allocations else "Registered tail-loop entry plans; ordinary lookup, allocation, Frame identity and stackless control retained"
                  if args.tails else "Production raw chains with per-call operand and implementation revalidation; profitability and remaining families in progress"
                  if args.chains else "Numeric declarations, guarded boxed calls and raw kernel preparation"
                  if args.numeric else "Recovery graph infrastructure; four optimizing families remain in progress"),
        "timing_qualification": "not measured", "authorities": {}, "binaries": {},
    }

    def save():
        save_json(args.output, receipt)

    def run(argv, label):
        print("74T: " + label, flush=True)
        argv = [str(a) for a in argv]
        completed = subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, text=True)
        output = completed.stdout + completed.stderr
        path = work / (label + ".log")
        path.write_text(output)
        receipt["commands"].append({"label": label, "argv": argv,
                                    "exit_code": completed.returncode, "output": output})
        save()
        if completed.returncode:
            raise RuntimeError(f"{label} failed; see {path}")
        return completed.stdout

    save()
    try:
        for name in AUTHORITIES:
            data = (ROOT / name).read_bytes()
            baseline = subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT)
            if not unchanged_authority(name, data, baseline):
                raise RuntimeError("changed compatibility authority: " + name)
            receipt["authorities"][name] = sha(ROOT / name)
        run(["python3", "scripts/compatibility-authority-test.py"], "image-compatibility-authority")
        receipt["image_authority_migration"] = "Design 81K owner/parent, 81L Integer and 81R Float dependency call substitutions; encoding unchanged"
        # 81J/81K/81L/81R deliberately reduce IdApi. Verify the current independent v5
        # manifest instead of claiming that this table matches the 74T baseline.
        run(["cargo", "test", "-p", "birnam-abi", "--locked", "--offline"], "current-idapi-v5")
        receipt["current_id_api"] = {
            "schema_version": 5, "change": "Design 81R Float dependency",
            "files_sha256": {name: sha(ROOT / name) for name in CURRENT_IDAPI},
        }
        for profile in ("debug", "release"):
            run(["cargo", "test", "--locked", "--offline",
                 "-p", "birnam-native-compiler", "--features", "allocation-metrics"]
                + (["--release"] if profile == "release" else []), profile + "-native-resource-ownership")
            run(["cargo", "test", "--locked", "--offline",
                 "-p", "birnam-tcc", "--features", "allocation-metrics"]
                + (["--release"] if profile == "release" else []), profile + "-baseline-resource-ownership")
            if args.closures:
                run(["cargo", "build", "--locked", "--offline",
                     "-p", "birnam-id", "-p", "birnam-object-model", "-p", "birnam-integer", "-p", "birnam-float", "-p", "birnam-prelude", "-p", "birnam-collection",
                     "-p", "birnam-error", "-p", "birnam-portable-code",
                     "-p", "birnam-codegen-runtime", "-p", "birnam-tcc"]
                    + (["--release"] if profile == "release" else []), profile + "-closure-native-build")
                fixture = work / (profile + "-block-construction")
                run(["cc", "-Wall", "-Wextra", "-Werror", "-Iabi/fixtures/idapi-v5",
                     "scripts/fixtures/native-block-construction.c", "-o", fixture], profile + "-closure-native-fixture")
                libraries = ROOT / ".cargo-target" / profile
                run([fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "prelude", "collection", "error", "portable_code")]
                    + ["abi/fixtures/portable-code-v1/program-explicit-captures.bin"], profile + "-closure-native-admission")
                run([fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "prelude", "collection", "error", "portable_code")]
                    + ["abi/fixtures/portable-code-v1/program-explicit-captures.bin", "without-prelude"], profile + "-closure-native-absent-family")
                api_fixture = work / (profile + "-closure-api-context")
                run(["cc", "-Wall", "-Wextra", "-Werror", "-Iabi/fixtures/idapi-v5",
                     "scripts/fixtures/closure-api-context.c", "-o", api_fixture], profile + "-closure-api-fixture")
                run([api_fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "tcc", "codegen_runtime")], profile + "-closure-api-context")
            output = run(["cargo", "test", "--locked", "--offline",
                          "-p", "birnam-id", "-p", "birnam-codegen", "-p", "birnam-integer", "-p", "birnam-float", "-p", "birnam-prelude", "-p", "birnam-relational", "--no-run", "--message-format=json"]
                         + (["--release"] if profile == "release" else []), profile + "-build")
            binaries = {}
            for line in output.splitlines():
                row = json.loads(line)
                if row.get("reason") == "compiler-artifact" and row.get("executable") and row["profile"]["test"]:
                    binaries[row["target"]["name"]] = Path(row["executable"])
            receipt["binaries"][profile] = {name: {"path": str(path), "sha256": sha(path)}
                                              for name, path in binaries.items()}
            checks = [
                ("birnam_codegen", "shape", "recovery_graph::tests"),
                ("birnam_id", "materialization", "runtime::recovery::graph::tests"),
                ("birnam_id", "id-producer", "runtime::native_tier::leaf_tests::graph_tests"),
                ("birnam_id", "dense-recovery", "production_recovery_admits_id_recipe"),
            ]
            if args.closures:
                checks.append(("birnam_codegen", "closure-plan", "closure_elision::"))
                checks.append(("birnam_id", "literal-callee", "runtime::native_tier::leaf_tests::literal_tests"))
                checks.append(("birnam_id", "closure-context", "closure_context_requires_active_nil"))
                checks.append(("birnam_id", "closure-staging", "disabled_closure_mode_skips_preparation"))
            if args.numeric:
                checks += [
                    ("birnam_codegen", "numeric-plan", "numeric_chain::"),
                    ("birnam_id", "numeric-kernels", "runtime::numeric::tests"),
                    ("birnam_id", "numeric-driver", "runtime::native_tier::leaf_tests::numeric_tests"),
                    ("birnam_integer", "numeric-declarations", "numeric_declarations_name_exact_implementations"),
                ]
            if args.unboxed:
                checks.append(("birnam_id", "unboxed-numeric", "runtime::native_tier::leaf_tests::unboxed_numeric_tests"))
            if args.chains:
                checks.append(("birnam_id", "numeric-chain-storage", "runtime::numeric::chain::tests"))
                checks.append(("birnam_id", "numeric-fusion", "runtime::native_tier::leaf_tests::fused_numeric_tests"))
            if args.tails:
                checks += [
                    ("birnam_codegen", "tail-entry-plan", "tail_entry::tests"),
                    ("birnam_id", "tail-entry-arena", "execution::tail_entry::tests"),
                    ("birnam_id", "tail-entry-loop", "runtime::native_tier::leaf_tests::tail_entry_tests"),
                    ("birnam_id", "loop-invariant-kernel", "runtime::numeric::chain::loop_reuse::tests"),
                    ("birnam_id", "loop-iteration-evidence", "execution::loop_iteration::tests"),
                    ("birnam_id", "loop-invariant-consumer", "runtime::native_tier::leaf_tests::loop_invariant_tests"),
                ]
            if args.allocations:
                checks += [
                    ("birnam_codegen", "discarded-clone-plan", "discarded_clone::tests"),
                    ("birnam_id", "dead-allocation-journal", "heap::dead_allocations::tests"),
                    ("birnam_id", "discarded-clone-driver", "runtime::native_tier::leaf_tests::discarded_clone_tests"),
                ]
            if args.fields:
                checks += [
                    ("birnam_codegen", "virtual-field-plan", "virtual_aggregate::tests"),
                    ("birnam_id", "virtual-field-driver", "runtime::native_tier::leaf_tests::virtual_aggregate_tests"),
                    ("birnam_id", "virtual-field-layout", "all_oop_layout_checks_current_prefix_tail_and_exact_vtable"),
                    ("birnam_relational", "native-field-declaration", "native_field_projection_declares_only_the_exact_pure_getter"),
                ]
            for package, label, test_filter in checks:
                output = run([binaries[package], test_filter, "--nocapture"], profile + "-" + label)
                match = re.search(r"test result: ok\. (\d+) passed; 0 failed", output)
                if not match or int(match[1]) == 0:
                    raise RuntimeError("no passing tests for " + label)
        if args.verify:
            from validation_proof import load_proof
            from validate import plan
            full = load_json(args.validation_receipt)
            install_argv = full['steps']['install']['argv']
            prefix = Path(install_argv[install_argv.index('--prefix') + 1])
            required = tuple(step['name'] for step in plan('release', prefix, args.matrix_directory.parent))
            verified = load_proof(args.validation_receipt, applications=True, required=required)
            receipt["sources"].update(verified["sources"])
            receipt.update(verify_complete_matrices(args.matrix_directory))
        if source_inputs != sources() or any(sha(ROOT / name) != digest for name, digest in receipt["sources"].items()):
            raise RuntimeError("sources changed during verification")
        receipt["status"] = "passed"
        receipt["production_verification"] = "passed" if args.verify else "not run"
    except BaseException as error:
        receipt["status"] = "failed"
        receipt["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    main()
