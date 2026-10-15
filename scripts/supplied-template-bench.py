#!/usr/bin/env python3
"""Compare supplied-template activation and installed host startup in fresh processes.

Build the artifact_templates native probe with cargo test --no-run first. For native
allocation bytes, use separate module prefixes containing resource-metrics TCC.
Host prefixes must retain their original production TCC and startup images.
"""
import argparse
import json
from pathlib import Path
import re
import statistics
import tempfile
from validation_support import clean_environment, command, save_json, sha

p = argparse.ArgumentParser(description=__doc__)
for flag in ['baseline', 'candidate', 'baseline-modules', 'candidate-modules', 'probe']:
    p.add_argument('--'+flag, type=Path, required=True)
p.add_argument('--samples', type=int, default=7)
a = p.parse_args()
assert a.samples > 0
work = Path(tempfile.mkdtemp(prefix='birnam-supplied-template-bench-'))
source = work/'startup.birnam'
source.write_text('[System println: 42]\n')
receipt = dict(passed=False, samples=a.samples, script_sha256=sha(Path(__file__).resolve()),
               probe_sha256=sha(a.probe), runs=[], scope=(
                   'Alternating fresh processes; no timing threshold. Native activation timers exclude '
                   'bootstrap, artifact compilation and verification. Host wall time and peak RSS include '
                   'startup-image loading, source compilation, printing 42 and process exit. Warm filesystem '
                   'caches. Peak RSS is distinct from TCC owned allocation counters.'))
for i in range(a.samples):
    for name in (['baseline', 'candidate'] if i % 2 == 0 else ['candidate', 'baseline']):
        env = clean_environment()
        env['BIRNAM_TEST_MODULE_DIRECTORY'] = str(getattr(a, name+'_modules').resolve())
        env['BIRNAM_GC_PRECISE'] = '1'
        if name == 'baseline':
            env['BIRNAM_ARTIFACT_TEMPLATE_BASELINE'] = '1'
        native = command([a.probe.resolve(), '--include-ignored', '--nocapture'], cwd=work, env=env)
        assert native['exit_code'] == 0, native
        matches = re.findall(r'artifact-template synchronous=(false|true) units=(\d+) native_bytes=(Some\((\d+)\)|None) elapsed_ns=(\d+)', native['output'])
        assert len(matches) == 2, native
        values = {mode: dict(units=int(units), native_bytes=int(size) if size else None, elapsed_ns=int(ns))
                  for mode, units, _, size, ns in matches}
        host = command(['/usr/bin/time', '-l', getattr(a, name).resolve()/'bin/birnam', source],
                       cwd=work, env=clean_environment())
        assert host['exit_code'] == 0 and host['stdout'] == '42\n', host
        rss = re.search(r'(\d+)\s+maximum resident set size', host['stderr'])
        assert rss, host
        receipt['runs'].append(dict(sample=i, name=name, native=native, host=host,
                                    activation=values, host_peak_rss=int(rss[1])))
        save_json(work/'results.json', receipt)
receipt['medians'] = {}
for name in ['baseline', 'candidate']:
    runs = [r for r in receipt['runs'] if r['name'] == name]
    receipt['medians'][name] = dict(
        resumable_activation_ns=statistics.median(r['activation']['false']['elapsed_ns'] for r in runs),
        synchronous_activation_ns=statistics.median(r['activation']['true']['elapsed_ns'] for r in runs),
        resumable_units=statistics.median(r['activation']['false']['units'] for r in runs),
        resumable_native_bytes=runs[0]['activation']['false']['native_bytes'],
        host_wall_ns=statistics.median(r['host']['wall_ns'] for r in runs),
        host_peak_rss=statistics.median(r['host_peak_rss'] for r in runs))
receipt['passed'] = True
save_json(work/'results.json', receipt)
print(json.dumps(receipt['medians'], indent=2))
print(work/'results.json')
