#!/usr/bin/env python3
"""Cold Doc loading and HTML coverage must work without a preloaded Html module."""
import subprocess
import sys
import tempfile
from pathlib import Path

binary = Path(sys.argv[1]).resolve() / 'bin/birnam'
repo = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='birnam-doc-render-') as directory:
    source = Path(directory) / 'render.birnam'
    source.write_text('''
[Module load: #doc];
[Doc module: #demoH is: "needs <escaping>"];
[Doc for: #zzDemoHFoo in: #demoH says: "a & b"];
[System println: [Doc htmlForModule: #demoH sources: [Map new]]];
// The page only needs a count. Trap the sorted-list path to prove it isn't used.
[Reflect defineMethod: #undocumentedSelectors on: Doc as: [:self |
  [Error kind: #unexpectedSortedCoverage]]];
let page := [Doc htmlIndex];
[System println: [page includesSubstring: "recorded selectors still source-only</footer>"]]
''')
    result = subprocess.run([binary, source], cwd=repo, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45)
    expected = (b'<h2><a href="#demoH">demoH</a></h2>'
                b'<p class="summary">needs &lt;escaping&gt;</p>'
                b'<dl><dt id="demoH-zzDemoHFoo"><code>zzDemoHFoo</code></dt>'
                b'<dd>a &amp; b</dd></dl>\ntrue\n')
    assert result.returncode == 0 and result.stdout == expected and not result.stderr, result
print('Doc: cold HTML dependency, escaped markup and coverage without sorting passed')
