"""Guarded resumable calls fall back after source-free image restoration."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

prefix = Path(sys.argv[1]).resolve()
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_NO_STDLIB='1', BIRNAM_GC_PRECISE='1')
with tempfile.TemporaryDirectory(prefix='birnam-frame-call-image-') as directory:
    scratch = Path(directory)
    image = scratch / 'calls.image'
    source = scratch / 'save.birnam'
    source.write_text(f'''
let probe := [Object clone];
[Reflect defineMethod: #pick:with: on: probe as: [:self :a :b | a]];
[Reflect defineMethod: #replacement:with: on: probe as: [:self :a :b | b]];
[Reflect defineMethod: #run:with: on: probe as: [:self :a :b |
  let x := [self pick: a with: b];
  let y := x; let z := y;
  [self pick: z with: b]
]];
let first := 41; let second := 99;
let main := [|
  [System println: [probe run: first with: second]];
  // Withdraw the optional protocol through the existing low-level Id API.
  // Ordinary Reflect publication retains its separate quiescence contract.
  [[Reflect vtableOf: Machine] removeMethod: #tryPrepareCall:slots:copies:selector:in:tail:];
  [[Reflect vtableOf: Machine] removeMethod: #tryPrepareCall:slots:selector:in:tail:];
  [System println: [[[Machine methodFor: #tryPrepareCall:slots:selector:in:tail:] isNil]
    & [[Machine methodFor: #tryPrepareCall:slots:copies:selector:in:tail:] isNil]]];
  [System gc];
  [System println: [probe run: first with: second]];
  [[Reflect vtableOf: probe] addMethod: [probe methodFor: #replacement:with:] as: #pick:with:];
  [System gc];
  [System println: [probe run: first with: second]]
];
[System saveImage: "{image}"];
[System println: [[System imageSaveReport] status]];
''')

    def run(args):
        result = subprocess.run([str(prefix / 'bin/birnam'), *args], env=env,
                                cwd=scratch, capture_output=True, timeout=180)
        assert result.returncode == 0 and not result.stderr, (result.returncode, result.stdout, result.stderr)
        return result.stdout

    saved = run([str(source)])
    assert saved == b'#saved\n', saved
    source.unlink()
    for args in [[str(image)], ['--rebuild', str(image)]]:
        observed = run(args)
        assert observed == b'41\ntrue\n41\n99\n', (args, observed)
    print('Frame calls: source-free mapped/rebuilt images, protocol withdrawal, collecting fallback and fresh target lookup passed')
