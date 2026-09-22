#!/usr/bin/env python3
"""74S: production workload comparison with only the runtime floor changed."""
from measurement_archive import load_json, read_bytes
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import time
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parent.parent
BASE = 'd06f7558'
SCENARIOS = ['eval', 'collections', 'portable-collections', 'materialize-cold', 'chain-1024']
spec = importlib.util.spec_from_file_location('focus', ROOT / 'scripts/native-tier-focus.py')
focus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(focus)
spec = importlib.util.spec_from_file_location('families', ROOT / 'scripts/optimization-families.py')
families = importlib.util.module_from_spec(spec)
spec.loader.exec_module(families)


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def sources():
    return dict(families.sources(), **{p: sha(ROOT / p) for p in (
        'scripts/optimization-applications.py', 'scripts/native-tier-focus.py',
        'bench/native-tier-focus.birnam', 'bench/runtime-native-tier.birnam',
        'scripts/fixtures/internal-guarded-sends.c')})


def closure(prefix):
    return {str(p.relative_to(prefix)): sha(p) for directory in ('bin', 'lib')
            for p in sorted((prefix / directory).rglob('*')) if p.is_file()}


def prepare(work):
    archive = work / 'baseline.tar'
    subprocess.run(['git', 'archive', '--format=tar', f'--output={archive}', BASE], cwd=ROOT, check=True)
    subprocess.run(['cc','-dynamiclib','-O2','-std=c11','-Wall','-Wextra','-Werror',
                    '-I',ROOT / 'abi/fixtures/idapi-v1',ROOT / 'scripts/fixtures/internal-guarded-sends.c',
                    '-o',work / 'lookup.dylib'],check=True)
    baseline = work / 'source-baseline'
    baseline.mkdir()
    subprocess.run(['tar', '-xf', archive, '-C', baseline], check=True)
    for name in focus.EXTRA_INPUTS:
        shutil.copy2(ROOT / name, baseline / name)
    result = dict(baseline=BASE, sources=sources(), helper_sha256=sha(work / "lookup.dylib"), prefixes={})
    for variant, source in [('on', ROOT), ('off', baseline)]:
        prefix = work / ('prefix-' + variant)
        focus.install(source, prefix, work / ('build-' + variant + '.log'))
        receipt = load_json(prefix / 'share/birnam/rust-install.json')
        result['prefixes'][variant] = dict(path=str(prefix), receipt=receipt,
                                          floor_sha256=sha(prefix / 'lib/libbirnam_id.dylib'))
    common = work / 'prefix-on'
    for variant in ('on', 'off'):
        run = work / ('run-' + variant)
        shutil.copytree(common, run)
        shutil.copy2(work / ('prefix-' + variant) / 'lib/libbirnam_id.dylib', run / 'lib/libbirnam_id.dylib')
        (run / 'share/birnam/rust-install.json').unlink()
        (run / 'share/birnam/74s-composition.json').write_text(json.dumps(dict(
            scope='measurement composition, not an installed-prefix receipt',
            common=str(common), floor=result['prefixes'][variant]['floor_sha256']), indent=2)+'\n')
        result['prefixes'][variant].update(measurement_path=str(run), binaries=closure(run))
    on, off = [result['prefixes'][v]['binaries'] for v in ('on', 'off')]
    assert on.keys() == off.keys()
    assert [p for p in on if on[p] != off[p]] == ['lib/libbirnam_id.dylib']
    assert result['sources'] == sources(), 'sources changed during build'
    (work / 'builds.json').write_text(json.dumps(result, indent=2)+'\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--measure', action='store_true')
    parser.add_argument('--pairs', type=int, default=8)
    parser.add_argument('--pause-rk-pid', type=int)
    args = parser.parse_args()
    assert args.prepare or args.measure
    assert args.pairs > 0
    args.work_dir = args.work_dir.resolve()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    receipt = dict(design='74S', status='running', sources=sources(), samples=[], gates=[],
                   timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   timing_qualification='not measured')
    paused = False

    def resume():
        nonlocal paused
        if paused:
            os.kill(args.pause_rk_pid, signal.SIGCONT)
            paused = False
            receipt['rk_resumed'] = True

    def pause():
        nonlocal paused
        if args.pause_rk_pid and not paused:
            pid = str(args.pause_rk_pid)
            assert subprocess.check_output(['ps','-p',pid,'-o','comm='],text=True).strip() == '/Users/chazu/.local/bin/rk'
            assert 'T' not in subprocess.check_output(['ps','-p',pid,'-o','stat='],text=True)
            os.kill(args.pause_rk_pid, signal.SIGSTOP)
            paused = True

    def gate(arguments):
        result = subprocess.run(['bash', 'scripts/bench-host-check.sh', *map(str, arguments)],
                                cwd=ROOT, env=focus.clean_env(), text=True, capture_output=True)
        receipt['gates'].append(dict(arguments=list(map(str, arguments)), exit_code=result.returncode,
                                     output=result.stdout+result.stderr))
        return result.returncode == 0

    try:
        builds = prepare(args.work_dir) if args.prepare else load_json(args.work_dir / 'builds.json')
        receipt['builds'] = builds
        assert builds['sources'] == sources(), 'build provenance is stale'
        assert builds['helper_sha256'] == sha(args.work_dir / 'lookup.dylib')
        for p in builds['prefixes'].values():
            assert p['binaries'] == closure(Path(p['measurement_path'])), 'measurement closure changed'
        if args.measure:
            for pair in range(args.pairs):
                cases = SCENARIOS[pair%len(SCENARIOS):]+SCENARIOS[:pair%len(SCENARIOS)]
                for case in cases:
                    # Admit each independent paired workload immediately before
                    # running it. Waits never select samples by measured results;
                    # any rejected interval still rejects the complete matrix.
                    for attempt in range(40):
                        pause()
                        if gate([f'74S-applications-{pair}-{case}']): break
                        resume()
                        print(f'Host busy before pair {pair} {case}; rk resumed; waiting 30 seconds', flush=True)
                        time.sleep(30)
                    else: raise RuntimeError('host did not become quiet')
                    path = args.work_dir / f'cpu-{pair}-{case}.log'
                    with path.open('w') as log:
                        monitor = subprocess.Popen(['top','-l','0','-s','1','-n','0','-stats','pid'], stdout=log, stderr=subprocess.DEVNULL)
                        try:
                            for variant in (['off','on'] if pair%2 == 0 else ['on','off']):
                                print(pair, case, variant, flush=True)
                                sample = focus.scenario(SimpleNamespace(work_dir=args.work_dir), builds, variant, case)
                                receipt['samples'].append(dict(pair=pair, variant=variant, **sample))
                        finally:
                            monitor.terminate()
                            monitor.wait()
                    assert gate(['--samples',path,f'74S-applications-{pair}-{case}']), 'measurement interval rejected'
            receipt['summary'] = {}
            for case in SCENARIOS:
                rows = [s for s in receipt['samples'] if s['scenario']==case]
                off, on = [[s for s in rows if s['variant']==v] for v in ('off','on')]
                metrics = {}
                for metric in off[0]['values']:
                    reductions = [100*(1-b['values'][metric]/a['values'][metric]) for a,b in zip(off,on) if a['values'][metric] != 0]
                    if reductions:
                        metrics[metric] = dict(median_reduction_percent=statistics.median(reductions), paired_percent=reductions,
                            medians={v:statistics.median(s['values'][metric] for s in rows if s['variant']==v) for v in ('off','on')})
                receipt['summary'][case] = metrics
            receipt['timing_qualification'] = 'host-gated'
        assert receipt['sources'] == sources(), 'sources changed during run'
        receipt['status'] = 'passed'
    except Exception as error:
        receipt.update(status='failed', error=str(error))
        raise
    finally:
        resume()
        receipt['interval_logs'] = {p.name:p.read_text() for p in args.work_dir.glob('cpu-*.log')}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2)+'\n')


if __name__ == '__main__':
    main()
