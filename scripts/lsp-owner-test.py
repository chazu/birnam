#!/usr/bin/env python3
"""Owned LSP state, retention, and new/legacy source-free images."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('prefix',type=Path);p.add_argument('--legacy-prefix',type=Path)
    p.add_argument('--skip-images',action='store_true');args=p.parse_args()
    prefix=args.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-lsp-owner-'))
    env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
    receipt={'passed':False,'prefix':str(prefix),'runs':[], 'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in [Path(__file__).resolve(),ROOT/'modules/lsp.birnam',ROOT/'tests/fixtures/lsp-owner.birnam']}}
    def run(name,argv,expected=None):
        r=command(argv,cwd=work,env=env,timeout=240)
        receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
        (work/(name+'.log')).write_text(r['output'])
        assert r['exit_code']==0 and not r['stderr'],(name,work,r)
        if expected is not None:assert r['stdout']==expected,(name,work,r)
        return r
    r=run('retention',[prefix/'bin/birnam',ROOT/'tests/fixtures/lsp-owner.birnam'])
    rows=json.loads(r['stdout']);assert rows['passed'];control=rows['batches'][0]['object_delta']
    assert [r['calls'] for r in rows['batches']]==[0,1,30,100]
    assert all(r['fact_delta']==0 and r['object_delta']<=control+4 for r in rows['batches']),rows
    receipt['retention']=rows
    if not args.skip_images:
        for label,writer in [('new',prefix)]+([('legacy',args.legacy_prefix.resolve())] if args.legacy_prefix else []):
            image=work/(label+'.image');source=work/(label+'.birnam')
            source.write_text('''
[Module load: #lsp];
[[|
let server := SERVER; let alias := server;
[server storeDoc: "file:///image" text: "saved" version: 7];
[server inputBuffer: "Content-Length: 2\\r\\n\\r\\n{}"];
[Globals at: #main put: [|
  [System gc];
  [System println: [server == alias]];
  [System println: [[server docsMap] at: "file:///image"]];
  [System println: [server versionOf: "file:///image"]];
  [System println: [server takeBufferedMessage]];
  [alias storeDoc: "file:///image" text: "changed" version: 8];
  [System gc];
  [System println: [[server docsMap] at: "file:///image"]];
  VERIFY
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('SERVER','[Lsp new]' if label=='new' else 'Lsp')
.replace('IMAGE',json.dumps(str(image)))
.replace('VERIFY', '''[System println: [[[Reflect slotsOf: server] size] = 13]];
  [System println: [[db currentValue: server attr: #docs] isNil]];
  [Reflect slot: #inputBuffer in: server put: "edited"];
  [System println: [server inputBuffer]]'''
 if label=='new' else '''[System println: [[db currentValue: server attr: #docs] == [server docsMap]]]'''))
            receipt[label+'_source']=source.read_text();run(label+'-save',[writer/'bin/birnam',source],'#saved\n');source.unlink()
            expected='true\nsaved\n7\n{}\nchanged\n'+('true\ntrue\nedited\n' if label=='new' else 'true\n')
            for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:
                run(label+'-'+mode,[prefix/'bin/birnam',*flags,image],expected)
            receipt[label+'_image_sha256']=sha(image)
    receipt['passed']=True;save_json(work/'results.json',receipt)
    print('lsp-owner: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
