#!/usr/bin/env python3
"""Repeat real native consumer Runtime teardown in isolated test processes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parent.parent
FAMILIES = ('unboxed_numeric', 'loop_invariants', 'discarded_clones', 'virtual_fields')
TEST_PREFIX = 'runtime::native_tier::leaf_tests::consumer_teardown_tests::'
LINE = re.compile(r'^consumer-teardown family=(\w+) cycle=(\d+) native_units=(\d+) relocated_bytes=(\d+) heap_blocks=(\d+) heap_bytes=(\d+)$', re.M)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def inventory():
    result = runpy.run_path(str(ROOT / 'scripts/recovery-graphs.py'))['sources']()
    result['scripts/consumer-teardown-smoke.py'] = sha(__file__)
    return result


def validate(row):
    assert row['status'] == 'passed' and row['exit_code'] == 0, 'failed process'
    family = row['family']
    assert family in FAMILIES
    assert row['argv'][1:] == [TEST_PREFIX + family, '--exact', '--ignored', '--nocapture', '--test-threads=1']
    raw = row['stdout']
    assert re.search(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;', raw, re.M), 'test not executed'
    # libtest prints the first uncaptured line after its exact test-name prefix.
    raw = re.sub(r'^test ' + re.escape(TEST_PREFIX + family) + r' \.\.\. (?=consumer-teardown )', '', raw, flags=re.M)
    matches = LINE.findall(raw)
    assert len(matches) == 3, 'missing or extra cycle'
    result = []
    for cycle, (actual_family, actual_cycle, *values) in enumerate(matches):
        assert actual_family == family and int(actual_cycle) == cycle, 'wrong family or cycle order'
        assert list(map(int, values)) == [0, 0, 0, 0], 'unreleased compiler resources'
        result.append(dict(cycle=cycle, native_units=0, relocated_bytes=0, heap_blocks=0, heap_bytes=0))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--profiles', nargs='+', choices=('debug', 'release'), default=['debug', 'release'])
    args = parser.parse_args()
    output = args.output.resolve()
    assert not output.exists(), 'preserve existing evidence'
    assert len(set(args.profiles)) == len(args.profiles)
    output.parent.mkdir(parents=True, exist_ok=True)
    result = dict(status='running', qualified=False, sources=inventory(), profiles=args.profiles, builds=[], rows=[],
                  scope='Private compiler heap/relocation release after real driver fixtures and complete Runtime destruction. Excludes installed source closure teardown, Rust metadata bytes and process footprint.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(CARGO_TARGET_DIR=str(args.target.resolve()), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))

    def save():
        output.write_text(json.dumps(result, indent=2) + '\n')

    def run(row, log):
        assert inventory() == result['sources'], 'sources changed'
        row.update(status='running', log=str(log))
        with log.open('w') as stream:
            process = subprocess.Popen(row['argv'], cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
            row['pid'] = process.pid
            save()
            print(row.get('family', 'build'), row['profile'], process.pid, flush=True)
            code = process.wait()
        row.update(exit_code=code, status='passed' if code == 0 else 'failed', log_sha256=sha(log), stdout=log.read_text())
        save()
        assert code == 0, str(log)
        assert inventory() == result['sources'], 'sources changed'

    try:
        save()
        for profile in args.profiles:
            build = dict(profile=profile, argv=['mise', 'exec', '--', 'cargo', 'test', '--locked', '--offline', '-p', 'birnam-id',
                        '--features', 'allocation-metrics', '--no-run', '--message-format=json', *(['--release'] if profile == 'release' else [])])
            result['builds'].append(build)
            run(build, output.with_suffix('.' + profile + '-build.log'))
            artifacts = []
            for line in build['stdout'].splitlines():
                if not line.startswith('{'):
                    continue
                item = json.loads(line)
                if item.get('reason') == 'compiler-artifact' and item.get('executable') and item['target']['name'] == 'birnam_id':
                    assert item['profile']['test'] and 'allocation-metrics' in item['features']
                    artifacts.append(Path(item['executable']).resolve())
            assert len(artifacts) == 1, 'ambiguous test binary'
            binary = artifacts[0]
            build.update(binary=str(binary), binary_sha256=sha(binary))
            for family in FAMILIES:
                row = dict(profile=profile, family=family, binary_sha256=sha(binary), argv=[str(binary), TEST_PREFIX + family, '--exact', '--ignored', '--nocapture', '--test-threads=1'])
                result['rows'].append(row)
                run(row, output.with_suffix('.' + profile + '-' + family + '.log'))
                assert sha(binary) == build['binary_sha256'], 'binary changed'
                row['cycles'] = validate(row)
                save()
        assert len(result['rows']) == len(args.profiles) * len(FAMILIES)
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
