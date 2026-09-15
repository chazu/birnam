#!/usr/bin/env python3
"""Compare all eight residual Machine actions at every numeric closure return."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def source(tail, captures):
    first = '[input + 15]' if captures else '[20 + 15]'
    last = '[first + 7]' if tail else 'let second := [first + 7]; second'
    return f'''[System load: "modules/os.birnam"];
[System preciseGc: [[System getEnv: "BIRNAM_CLOSURE_PRECISE"] = "1"]];
let source := [:input |
    let closure := [| let first := {first}; {last}];
    let first := [closure value];
    let second := [closure value];
    let third := [closure value];
    third];
let factory := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];
let answer := [ClosureControlProbe actionState: factory input: 20];
[[answer = 42] ifTrue: [| nil] ifFalse: [| [OS exit: 61]]];
[System gc];
[[answer = 42] ifTrue: [| nil] ifFalse: [| [OS exit: 62]]];
[System println: "closure-actions: passed"];
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    sha = lambda p: hashlib.sha256(read_bytes(p)).hexdigest()
    paths = [Path(__file__).resolve()] + [p for p in (root / 'rust').rglob('*') if p.is_file()]
    sources = {str(p.relative_to(root)): sha(p) for p in paths}
    manifest = prefix / 'share/birnam/rust-install.json'
    publication = load_json(manifest)
    assert publication['publication']['state'] == 'published'
    artifacts = dict(publication['staged_manifest'])
    artifacts['share/birnam/rust-install.json'] = sha(manifest)
    floor_hash = sha(floor)
    result = dict(status='running', qualified=False, sources=sources, artifacts=artifacts,
                  floor_sha256=floor_hash, floor_overlay=True, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_CLOSURE_CONTROL='1', BIRNAM_DEV_LITERAL_CALLEES='0',
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0')
    def current():
        assert all(sha(root / n) == h for n, h in sources.items())
        assert all(sha(prefix / n) == h for n, h in artifacts.items())
        assert sha(floor) == floor_hash
    def save():
        save_json(args.output, result)
    try:
        with tempfile.TemporaryDirectory(prefix='birnam-numeric-actions-') as directory:
            fixture = Path(directory) / 'actions.birnam'
            for tail in [False, True]:
                for captures in [False, True]:
                    program = source(tail, captures)
                    fixture.write_text(program)
                    for precise in [0, 1]:
                        baseline = None
                        for enabled, forced in [(0, -1), (1, -1), (1, 0), (1, 1), (1, 2)]:
                            current()
                            run = subprocess.run([str(prefix / 'bin/birnam'), str(fixture)], cwd=root,
                                env=dict(env, BIRNAM_CLOSURE_PRECISE=str(precise),
                                         BIRNAM_DEV_CLOSURE_ELISION=str(enabled), BIRNAM_DEV_CLOSURE_FORCE_AT=str(forced)),
                                text=True, capture_output=True, timeout=180)
                            row = dict(tail=tail, captures=captures, precise=precise, enabled=enabled, forced=forced,
                                       source=program, source_sha256=sha(fixture), exit_code=run.returncode,
                                       stdout=run.stdout, stderr=run.stderr)
                            result['rows'].append(row); save()
                            assert run.returncode == 0, row
                            lines = run.stdout.splitlines()
                            assert lines.count('closure-actions: passed') == 1, row
                            snapshots = [json.loads(s.removeprefix('closure-actions: ')) for s in lines
                                         if s.startswith('closure-actions: [')]
                            assert len(snapshots) == 1 and len(snapshots[0]) == 3, row
                            for event in snapshots[0]:
                                assert [len(a) for a in event] == [4, 9, 8, 5, 7, 7, 6, 7, 4], row
                                assert event[-1][2:] == [1, 0], row
                            expected = 0 if not enabled else 3 if forced < 0 else forced
                            assert lines.count(f'closure-actions-completed: {expected}') == 1, row
                            if baseline is None:
                                baseline = snapshots
                            assert snapshots == baseline, row
                            row['snapshots'] = snapshots[0]; save()
                            print(f'actions: tail={tail} captures={captures} precise={precise} enabled={enabled} forced={forced} passed', flush=True)
        current(); result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    main()
