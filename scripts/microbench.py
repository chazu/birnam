#!/usr/bin/env python3
"""Bounded paired Id benchmarks. No idle-host gate and no startup-image rebuild."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import shutil
import sys
import time
from validation_support import ROOT, clean_environment, command, save_json, sha
from microbench_report import summarize

OWNERS = ('native_backend', 'execution', 'portable_code', 'object_model', 'word', 'vector', 'string', 'bytes', 'root_registry', 'collect', 'id', 'namespace', 'codegen_runtime')
TARGET = ROOT / '.cargo-target/decomposition-install/target'
WORK = ROOT / '.cargo-target/microbench'


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', type=Path, default=WORK / 'prefix')
    p.add_argument('--prepare-from', type=Path, help='copy a compatible installed prefix; rebuild changed release owners')
    p.add_argument('--case', choices=['all','abi-audit','abi-audit-object-incoming','abi-audit-read','abi-audit-roundtrip','abi-audit-word-incoming','abi-audit-bytes-incoming','word-nested','word-locals','helper','integer-add','integer-add-core','integer-multiply','public-integer-loop','public-integer-roots','public-integer-retains','public-integer-default','semantic-string-length','semantic-vector-access','semantic-word-as-integer','semantic-bytes-as-string','semantic-boolean-print'], default='all')
    p.add_argument('--seconds', type=float, default=10, help='total measurement budget, at most 10 seconds')
    p.add_argument('--batch-ms', type=float, default=25)
    p.add_argument('--gc', choices=['normal','off'], default='normal')
    p.add_argument('--counts-only', action='store_true', help='untimed selector census using an isolated metrics Id build')
    p.add_argument('--trace', action='store_true', help='full warmed candidate message tree; requires counts-only')
    p.add_argument('--profile', choices=['baseline','candidate'], help='sample one fixture; profiling timings are not comparison evidence')
    p.add_argument('--setup-timeout', type=float, default=60)
    p.add_argument('--output', type=Path, default=WORK / 'latest.json')
    return p


def build(env, argv, label):
    start = time.monotonic()
    result = command(['mise','exec','--','cargo','build','--release','--offline','--locked',*argv], cwd=ROOT, env=env, timeout=180)
    if result['exit_code']:
        raise RuntimeError(label + ': ' + result['stderr'][-4000:])
    return time.monotonic() - start


def main():
    p = parser(); args = p.parse_args()
    if not .1 <= args.seconds <= 10 or not 1 <= args.batch_ms <= 100 or not 1 <= args.setup_timeout <= 120:
        p.error('seconds must be 0.1..10; batch-ms 1..100; setup-timeout 1..120')
    if args.trace and (not args.counts_only or args.case != 'public-integer-default'):
        p.error('trace currently requires --counts-only --case public-integer-default')
    if args.profile and (args.case in ('all','abi-audit') or not 2 <= args.seconds <= 5):
        p.error('profiling requires one --case and --seconds between 2 and 5')
    env = clean_environment(); env['CARGO_TARGET_DIR'] = str(TARGET)
    build_seconds = 0
    prefix = args.prefix.resolve()
    if args.prepare_from:
        source = args.prepare_from.resolve()
        if source == prefix or prefix in source.parents or source in prefix.parents:
            p.error('source and diagnostic prefix must be separate directories')
        if prefix.exists() and not (prefix / 'microbench-prefix.json').exists():
            p.error('refusing to modify a prefix without a microbench ownership marker')
        if not (source / 'lib/libbirnam_bootstrap.dylib').is_file():
            p.error('prepare-from must be a compatible Birnam prefix')
        build_seconds += build(env, ['--lib',*[x for owner in OWNERS for x in ('-p','birnam-'+owner.replace('_','-'))]], 'owner build')
        shutil.copytree(source, prefix, ignore=shutil.ignore_patterns('*.image'), dirs_exist_ok=True)
        for owner in OWNERS:
            shutil.copy2(TARGET / 'release' / f'libbirnam_{owner}.dylib', prefix / 'lib' / f'libbirnam_{owner}.dylib')
        save_json(prefix / 'microbench-prefix.json', {'diagnostic_only': True, 'source_prefix': str(source), 'note': 'Copied install receipt is stale. This is not a published installation.'})
    if not prefix.is_dir():
        p.error('prepare a diagnostic prefix with --prepare-from PREFIX, or provide --prefix PREFIX')
    if not args.prepare_from and (prefix / 'microbench-prefix.json').exists():
        build_seconds += build(env, ['--lib',*[x for owner in OWNERS for x in ('-p','birnam-'+owner.replace('_','-'))]], 'owner build')
        for owner in OWNERS:
            source = TARGET / 'release' / f'libbirnam_{owner}.dylib'
            target = prefix / 'lib' / source.name
            if sha(source) != sha(target): shutil.copy2(source, target)
    build_seconds += build(env, ['-p','birnam-siphon','--example','microbench'], 'runner build')
    if args.counts_only:
        if args.profile: p.error('counts-only and profile are separate runs')
        metrics_target=WORK/'metrics-id'
        metrics_env=dict(env,CARGO_TARGET_DIR=str(metrics_target))
        build_seconds += build(metrics_env,['-p','birnam-id','--lib','--features','dispatch-metrics'],'metrics Id build')
        metrics_prefix=WORK/'counts-prefix'
        shutil.copytree(prefix,metrics_prefix,ignore=shutil.ignore_patterns('*.image','microbench-prefix.json'),dirs_exist_ok=True)
        (metrics_prefix/'microbench-prefix.json').unlink(missing_ok=True)
        shutil.copy2(metrics_target/'release/libbirnam_id.dylib',metrics_prefix/'lib/libbirnam_id.dylib')
        prefix=metrics_prefix
    env['BIRNAM_TEST_MODULE_DIRECTORY'] = str(prefix / 'lib')
    output = args.output.resolve(); output.parent.mkdir(parents=True, exist_ok=True)
    scratch = output.parent / (output.stem + '-artifacts'); scratch.mkdir(exist_ok=True)
    stderr_path = output.with_suffix('.stderr.log')
    binary = TARGET / 'release/examples/microbench'
    argv = [str(binary),'--case',args.case,'--seconds',str(args.seconds),'--batch-ms',str(args.batch_ms),'--gc',args.gc,'--scratch',str(scratch)]
    if args.profile: argv += ['--profile',args.profile]
    if args.counts_only: argv += ['--counts-only']
    if args.trace: argv += ['--trace']
    revision = command(['git','rev-parse','HEAD'],cwd=ROOT,env=env)['stdout'].strip()
    status = command(['git','status','--porcelain'],cwd=ROOT,env=env)['stdout']
    def source_digest():
        digest = hashlib.sha256()
        paths = [*ROOT.glob('rust/**/*.rs'), *ROOT.glob('rust/**/*.c'), *ROOT.glob('rust/**/Cargo.toml'), *ROOT.glob('modules/**/*.birnam'), ROOT/'Cargo.lock', ROOT/'Cargo.toml']
        for path in sorted(paths):
            digest.update(str(path.relative_to(ROOT)).encode()); digest.update(bytes.fromhex(sha(path)))
        return digest.hexdigest()
    source_hash = source_digest()
    libraries = {path.name: sha(path) for path in sorted((prefix/'lib').glob('*.dylib'))}
    started = time.monotonic(); sampler = None
    with stderr_path.open('w') as errors:
        proc = subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=errors,text=True,start_new_session=True)
        try:
            os.setpriority(os.PRIO_PROCESS, proc.pid, 5)
            poll = selectors.DefaultSelector(); poll.register(proc.stdout, selectors.EVENT_READ)
            if not poll.select(args.setup_timeout): raise TimeoutError('setup deadline exceeded')
            line = proc.stdout.readline()
            if not line: raise RuntimeError(f'runner failed during setup; see {stderr_path}')
            ready = json.loads(line)
            if ready.get('event') != 'ready': raise RuntimeError('missing ready barrier')
            poll.close()
            sample_path = output.with_suffix('.sample.txt')
            if args.profile:
                sample_path.unlink(missing_ok=True)
                sampler = subprocess.Popen(['sample',str(proc.pid),str(int(args.seconds)), '1','-file',str(sample_path)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                proc.stdin.write('go\n'); proc.stdin.flush()
            if args.counts_only:
                proc.stdin.write('go\n'); proc.stdin.flush()
            stdout, _ = proc.communicate(timeout=args.seconds + 8)
            if proc.returncode: raise RuntimeError(f'runner exited {proc.returncode}; see {stderr_path}')
            result = json.loads(stdout.strip())
            if result.get('event') != 'result': raise RuntimeError('missing result record')
            if sampler is not None:
                sampler.wait(timeout=8)
                if sampler.returncode or not sample_path.is_file() or not sample_path.stat().st_size:
                    raise RuntimeError('sampling failed to produce a profile')
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid,signal.SIGTERM)
                try: proc.wait(timeout=2)
                except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL); proc.wait()
            if sampler is not None and sampler.poll() is None: sampler.terminate(); sampler.wait(timeout=2)
    changed = source_digest() != source_hash or libraries != {path.name: sha(path) for path in sorted((prefix/'lib').glob('*.dylib'))}
    summaries = [] if args.profile or args.counts_only else [summarize(case) for case in result['cases']]
    for summary in summaries:
        if summary['case'].startswith('abi-audit-'):
            summary['comparison_kind'] = 'ABI cost contrast; different carrier or leaf contract, not a regression verdict'
    if changed:
        for summary in summaries: summary['verdict'] = 'inconclusive'; summary['reason'] = 'sources or libraries changed during run' 
    evidence = {'schema':1,'source_revision':revision,'dirty_status':status,'source_sha256':source_hash,
                'prefix':str(prefix),'library_sha256':libraries,'runner_sha256':sha(binary),
                'build_seconds':build_seconds,'process_seconds':time.monotonic()-started,'ready':ready,
                'settings':{'seconds':args.seconds,'batch_ms':args.batch_ms,'gc':args.gc,'case':args.case,'profile':args.profile,'counts_only':args.counts_only,'trace':args.trace,'nice':5},
                'raw':result,'summary':summaries,'inputs_changed_during_run':changed}
    if args.profile: evidence['profile_path'] = str(sample_path)
    save_json(output,evidence)
    print(f"setup {ready['setup_ms']/1000:.2f}s; measurement {result['measurement_ms']/1000:.2f}s; build {build_seconds:.2f}s")
    for s in summaries:
        ratio = s.get('candidate_over_baseline')
        label = 'ABI cost contrast' if s.get('comparison_kind') else s['verdict']
        print(f"{s['case']}: {label}; {s['pairs']} pairs" + (f"; candidate/baseline CPU {ratio:.3f}" if ratio else ''))
    print(output)


if __name__ == '__main__':
    try: main()
    except (RuntimeError,TimeoutError,ValueError,subprocess.TimeoutExpired) as error:
        print(f'microbench: {error}',file=sys.stderr);sys.exit(1)
