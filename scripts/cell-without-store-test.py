#!/usr/bin/env python3
"""Load actual Cell initializers; retain exported ancillary owner/registry probes."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

MODULES = 'id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer float prelude file_io vector map artifact tcc native_backend codegen_runtime error globalsOwner cell'.split()

HARNESS = [
 ('TestContext', ['runner'], ['slotNames','runner','for:']),
 ('ConsoleRunner', ['pass','fail'], ['slotNames','passedCount','failedCount','reset']),
 ('TestSuite', ['name','cases','testsModule'], ['slotNames','name','cases','module','named:module:','test:do:']),
 ('TestCase', ['cname','cblock'], ['slotNames','name','body']),
 ('Test', ['suites'], ['slotNames','suites','register:','unregister:','suitesForModule:']),
 ('BenchSuite', ['name','cases'], ['slotNames','name','cases','named:','addCase:k:blk:','case:opsPerIter:do:','rawCase:opsPerIter:do:','fixedCase:iters:do:']),
 ('BenchCase', ['cname','k','raw','fixedN','cblock'], ['slotNames','cname','k','raw','fixedN','cblock']),
 ('BenchResult', ['rname','nsOp','nsOpRaw','netUs','ops','iters','nreps','medianUs','minUs'], ['slotNames','rname','nsOp','nsOpRaw','netUs','ops','iters','nreps','medianUs','minUs']),
 ('Bench', ['results','failures','suiteName'], ['slotNames','results','failures','suiteName']),
]
HARNESS_HELPERS = ['anySatisfy:','count:','select:','reject:']

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);p.add_argument('--slots',action='store_true',help='also export ordinary slot reflection and Module libDir');p.add_argument('--harness',action='store_true',help='also exercise Test/Bench fields and registry');p.add_argument('--records',action='store_true',help='also exercise ordinary record fields');p.add_argument('--registry',action='store_true',help='also exercise the owned Module registry');args=p.parse_args();args.slots = args.slots or args.registry or args.records or args.harness
    prefix=args.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-cell-without-store-'))
    programs=work/'programs';programs.mkdir();floor=work/'floor';floor.mkdir()
    source=work/'export.birnam'
    record_setup='[Module load: #log]; [Module load: #config]; [Module load: #http]; [Module load: #process]; [System load: '+json.dumps(str(prefix/'share/birnam/src/modules/app-config.birnam'))+'];\n'
    harness_setup=''.join('[System load: '+json.dumps(str(prefix/'share/birnam/src/modules'/f'{m}.birnam'))+'];\n' for m in ['test','bench'])
    if args.harness:(programs/'harness.json').write_text(json.dumps(HARNESS))
    source.write_text((harness_setup if args.harness else '') + (record_setup if args.records else '') + '''
[[|
let requests := REQUESTS;
[0 to: [[requests size] - 1] do: [:index |
  let request := [requests at: index];
  let rows := [Reflect directMethodsOf: [request at: 0]];
  let row := [rows detect: [:row | [[row at: #selector] == [request at: 1]]]];
  let block := [row at: #method];
  [System println: [block captureCount]];
  let bytes := [CodeObject canonicalBytesOf: [block codeObject]];
  [System writeFile: [[DIRECTORY , [index printString]] , ".bin"] contents: [bytes asString]]
]]
] value]
'''.replace('DIRECTORY',json.dumps(str(programs)+'/')).replace('REQUESTS',
      '#[' +
      (' #[Object #slotNames] #[Reflect #isSymbol:]'
       ' #[Reflect #slotsOf:] #[Reflect #slotIndex:in:] #[Reflect #slot:in:] #[Reflect #slot:in:put:]'
       ' #[Module #slotNames] #[Module #libDir] #[Module #libDir:]' if args.slots else '') +
      (' #[Module #registry] #[Module #registry:] #[Module #recordNames] #[Module #recordOf:]'
       ' #[Module #registryMutation:] #[Module #record:put:] #[Module #field:of:] #[Module #field:of:put:]'
       ' #[Module #removeRecord:] #[Module #named:requires:] #[Module #kernel:] #[Module #requiresOf:]'
       ' #[Module #kindOf:] #[Module #isRegistered:] #[Module #isLoaded:] #[Module #markLoaded:]'
       ' #[Module #loadOrder:] #[Module #visit:order:path:] #[Module #publishRegistry:from:]' if args.registry else '') +
      (' #[Logger #slotNames] #[Logger #name] #[Logger #thresholdRank] #[Logger #sinkPath] #[Logger #named:] #[Logger #toFile:] #[Logger #toStdout]'
       ' #[Config #slotNames] #[Config #path] #[HttpRequest #slotNames] #[HttpRequest #method] #[HttpRequest #path] #[HttpRequest #raw]'
       ' #[ProcResult #slotNames] #[ProcResult #stdout] #[ProcResult #exitCode] #[AppConfig #slotNames] #[AppConfig #values] #[AppConfig #origins] #[AppConfig #path] #[AppConfig #at:] #[AppConfig #originAt:]' if args.records else '') +
      (''.join(' #[%s #%s]' % (name, method) for name,fields,methods in HARNESS for method in methods) + ''.join(' #[Collection #%s]' % method for method in HARNESS_HELPERS) if args.harness else '') + ']'))
    env=clean_environment()
    export=command([prefix/'bin/birnam',source],env=env,timeout=120)
    (work/'export.log').write_text(export['output'])
    assert export['exit_code']==0 and not export['stderr'] and export['stdout']=='0\n'*((28 if args.registry else 9 if args.slots else 0) + (22 if args.records else 0) + (sum(len(methods) for name,fields,methods in HARNESS)+len(HARNESS_HELPERS) if args.harness else 0)),(work,export)
    for module in MODULES: shutil.copy2(prefix/'lib'/f'libbirnam_{module}.dylib',floor)
    assert not (floor/'libbirnam_store.dylib').exists()
    env.update(BIRNAM_CELL_PROGRAMS=str(programs), BIRNAM_TEST_MODULE_DIRECTORY=str(floor),
               CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
    if args.slots: env['BIRNAM_CELL_SLOTS']='1'
    if args.harness: env['BIRNAM_HARNESS_OWNERS']='1'
    if args.records: env['BIRNAM_ORDINARY_RECORDS']='1'
    if args.registry: env['BIRNAM_MODULE_REGISTRY']='1'
    run=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','cell_without_store','--','--include-ignored'],env=env,timeout=180)
    (work/'run.log').write_text(run['output'])
    receipt={'passed':run['exit_code']==0,'prefix':str(prefix),'export':export,'run':run,
      'slots':args.slots, 'registry':args.registry, 'records':args.records, 'harness':args.harness,
      'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in [ROOT/'modules/cell.birnam', ROOT/'modules/stdlib.birnam', ROOT/'modules/modules.birnam',ROOT/'rust/birnam-siphon/tests/cell_without_store.rs',Path(__file__).resolve()]+([ROOT/'modules'/f'{m}.birnam' for m in ['log','config','http','process','app-config']] if args.records else [])+([ROOT/'modules/test.birnam',ROOT/'modules/bench.birnam'] if args.harness else [])},
      'program_sha256':{f.name:sha(f) for f in programs.glob('*.bin')},
      'floor_sha256':{f.name:sha(f) for f in floor.iterdir()},
      'scope':'Actual Error/Globals/Cell initializers without Store/Relational/Bootstrap/Lang/Generation, plus exported unchanged ancillary slot/record/registry/harness bodies. No synthetic Cell or Globals alias. Not a Store-free full application bootstrap.'}
    save_json(work/'results.json',receipt)
    assert receipt['passed'],work
    print('cell-without-store: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
