#!/usr/bin/env python3
"""Check private scratch lifetimes, public Store data and reentrant/collecting sends.

By default tests compiled installed modules. --source is a development-only overlay
and is explicitly recorded; it is not cold-bootstrap acceptance.
"""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--source', action='store_true')
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    work = args.work.resolve() if args.work else Path(tempfile.mkdtemp(prefix='birnam-scratch-state-test-'))
    marker = work/'scratch-state-work.json'
    if args.work and work.exists() and not marker.is_file(): parser.error('work directory must be dedicated to this runner')
    if work == ROOT or ROOT.is_relative_to(work) or prefix == work or prefix.is_relative_to(work): parser.error('invalid work directory')
    work.mkdir(parents=True,exist_ok=True); save_json(marker,{'purpose':'private scratch validation'})
    (work/'summary.rs').write_text('//! Retained public summary.\n')
    (work/'declaration.rs').write_text('//! Public declaration summary.\n/// Visible declaration.\n#[repr(C)]\npub struct PublicThing {}\n')
    source = ROOT/'tests/fixtures/scratch-state.birnam'
    prelude = ''
    if args.source:
        # Load before overlaying so a later Module load cannot restore old code.
        prelude = '[Module load: #markdown]; [Module load: #nativedoc]; [Module load: #alienCompiler];\n'
        for name in ('markdown','nativedoc','alien-compiler'):
            prelude += '[System load: '+json.dumps(str(ROOT/'modules'/f'{name}.birnam'))+'];\n'
    script = work/'proof.birnam'; script.write_text(prelude+source.read_text())
    run = command([prefix/'bin/birnam',script],cwd=work,env=clean_environment(),timeout=180)
    (work/'run.log').write_text(run['output'])
    result = dict(source_overlay=args.source,prefix=str(prefix),run=run,
                  source_sha256={str(p.relative_to(ROOT)):sha(p) for p in [source,Path(__file__).resolve(),*[ROOT/'modules'/f'{n}.birnam' for n in ('markdown','nativedoc','alien-compiler')]]},
                  runtime_sha256={str(p.relative_to(prefix)):sha(p) for p in [prefix/'bin/birnam',*sorted((prefix/'lib').glob('*.dylib')),*sorted((prefix/'lib').glob('*.image'))]},passed=False)
    save_json(work/'results.json',result)
    if run['exit_code'] or run['stderr'] or not run['stdout'].endswith('scratch-state: passed\n'):
        raise SystemExit('scratch-state failed; see '+str(work/'run.log'))
    rows = [json.loads(line) for line in run['stdout'].splitlines()[:-1]]
    assert [row['case'] for row in rows] == ['markdown','alien-parse','alien-error','alien-classify','alien-compile','nativedoc']
    assert all(len(row['batches'])==3 and row['calls_per_batch']==20 for row in rows)
    assert all(batch['fact_delta']==0 and batch['object_delta']<16 for row in rows for batch in row['batches'])
    result.update(passed=True,rows=rows); save_json(work/'results.json',result)
    print('scratch-state: six operations x 60 calls; catalog, external facts, asCell, reentry and GC passed')
    print(work/'results.json')


if __name__ == '__main__': main()
