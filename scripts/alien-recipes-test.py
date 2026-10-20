#!/usr/bin/env python3
"""Current foreign registrations, code reuse, owner aliases and successive images."""
import json
from pathlib import Path
import sys
import tempfile
from validation_support import clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve(); work = Path(tempfile.mkdtemp(prefix='birnam-alien-recipes-'))
source = work/'recipes.birnam'; first = work/'first.image'; next_image = work/'next.image'
source.write_text('''[[|
let before := [[AlienRecipes objectCode] size];
[Alien wrap: "labs" signature: "l l" as: #ownerAbsolute on: Integer];
let code := [AlienRecipes objectFor: "labs" signature: "l l"];
[Alien wrap: "labs" signature: "l l" as: #ownerOtherAbsolute on: Integer];
[System println: [code == [AlienRecipes objectFor: "labs" signature: "l l"]]];
[System println: [[[AlienRecipes objectCode] size] = [before + 1]]];
[System println: [[Store attrCount: #__aliens__] = 0]];
// Returning to an older function must not let an older row win during replay.
[Alien wrap: "toupper" signature: "i i" as: #ownerCase on: Integer];
[Alien wrap: "tolower" signature: "i i" as: #ownerCase on: Integer];
[Alien wrap: "toupper" signature: "i i" as: #ownerCase on: Integer];
[System println: [[97 ownerCase] = 65]];
[Alien wrap: "labs" signature: "l l" as: #ownerRemoved on: Integer];
[Reflect removeMethod: #ownerRemoved on: Integer];
[System println: [[Execution directMethodFor: #ownerRemoved in: [Reflect vtableOf: Integer]] isNil]];
[Alien wrap: "labs" signature: "l l" as: #ownerReplaced on: Integer];
[Reflect defineMethod: #ownerReplaced on: Integer as: [:self | 123]];
[System println: [[-7 ownerReplaced] = 123]];
// Cache data is independent of already installed Methods and preserves aliases.
let copy := [Execution bytesResultOf: code selector: #bytes resultFactory: Bytes];
[System println: [[copy == code] not]];
let objects := [AlienRecipes objectCode];
let signatures := [[objects at: "labs"] at: "l l" put: copy];
[AlienRecipes objectCode: [objects at: "labs" put: signatures]];
[Globals at: #SavedRecipeBytes put: copy];
[Globals at: #SavedRecipeOwner put: AlienRecipes];
[Globals at: #SavedRecipeCycle put: [0 asCell]];
[Globals at: #main put: [|
  [System gc];
  [System println: [SavedRecipeOwner == AlienRecipes]];
  [System println: [[-7 ownerAbsolute] = 7]];
  [System println: [[-8 ownerOtherAbsolute] = 8]];
  [System println: [[97 ownerCase] = 65]];
  [System println: [[Execution directMethodFor: #ownerRemoved in: [Reflect vtableOf: Integer]] isNil]];
  [System println: [[-7 ownerReplaced] = 123]];
  [System println: [[AlienRecipes objectFor: "labs" signature: "l l"] notNil]];
  [System println: [[AlienRecipes objectFor: "labs" signature: "l l"] == SavedRecipeBytes]];
  [SavedRecipeCycle inc]; [System println: [SavedRecipeCycle value]];
  [[[SavedRecipeCycle value] < 3] ifTrue: [| [System saveImage: NEXT]]]
]]
] value];
[System gc]; [System saveImage: FIRST];
[System println: [[System imageSaveReport] status]]
'''.replace('FIRST', json.dumps(str(first))).replace('NEXT', json.dumps(str(next_image))))
env = clean_environment(); env['BIRNAM_GC_PRECISE'] = '1'
receipt = {'passed': False, 'prefix': str(prefix), 'source': source.read_text(), 'runner_sha256': sha(Path(__file__).resolve()), 'runs': []}
for name, args, expected in [
    ('source', [source], 'true\n'*7+'#saved\n'),
    ('mapped', [first], 'true\n'*8+'1\n'),
    ('rebuilt-resave', ['--rebuild', next_image], 'true\n'*8+'2\n'),
    ('mapped-resave', [next_image], 'true\n'*8+'3\n'),
]:
    run = command([prefix/'bin/birnam', *args], cwd=work, env=env, timeout=600)
    receipt['runs'].append(dict(name=name, **run)); save_json(work/'results.json', receipt)
    assert run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == expected, (work, name, run)
    if name == 'source': source.unlink()
receipt['passed'] = True; save_json(work/'results.json', receipt)
first.unlink(); next_image.unlink()
print('Alien recipes: passed; ' + str(work/'results.json'))
