"""Exercise candidate changes and conditional undo through ordinary sends."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve() / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'
with tempfile.TemporaryDirectory(prefix='birnam-compiled-unit-') as temporary:
    work = Path(temporary)
    forms = ['[Module load: #repl]']
    checks = []

    def check(expression, label):
        checks.append(label)
        forms.append(f'[System println: #[#compiledUnitCheck {len(checks)} {expression}]]')

    def method(selector, value):
        return f'[Reflect defineMethod: #{selector} on: Integer as: [:self | {value}]]'

    def source(name, *operations, requires='#[]'):
        return f'[Module named: #{name} requires: {requires}]; ' + '; '.join(operations)

    baselines = {
        'cuPolicy': source('cuPolicy', method('cuReplace', 11), method('cuRemove', 22)),
        'cuChain': source('cuChain', method('cuInherited', 7), method('cuOverlay', 8)),
        'cuMethodless': source('cuMethodless', method('cuMethodless', 6)),
        'cuUnmapped': source('cuUnmapped', method('cuUnmapped', 4)),
        'cuMissing': source('cuMissing', method('cuDependencyGuard', 31), requires='#[#cuUnavailable]'),
    }
    for name, baseline in baselines.items():
        forms.append(f'[Codegen eval: {json.dumps(baseline)}]')
        forms.append(f'[Codegen recordModuleSource: {json.dumps(baseline)} named: #{name}]')
    # Source identity is known, but this dependency has never been loaded.
    forms.append('[Codegen recordModuleSource: "[Module named: #cuUnavailable requires: #[]]" named: #cuUnavailable]')
    forms.append('[DispatchGeneration adoptCurrent]')
    image = work / 'baseline.image'
    baseline_script = work / 'baseline.birnam'
    baseline_script.write_text(';\n'.join(forms) + ';\n' +
                               f'[System saveImage: {json.dumps(str(image))}]')
    prepared = subprocess.run([binary, baseline_script], cwd=work, env=env,
                              capture_output=True, timeout=300)
    assert prepared.returncode == 0 and image.is_file() and not prepared.stderr, prepared
    baseline_script.unlink()
    forms.clear()
    check('[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]', 'image does not adopt automatically')
    check('[[[DispatchGeneration adoptCurrent] isError] not]', 'explicit image baseline adoption')

    def candidate(variable, name, text, selection=''):
        path = json.dumps(str(work / f'{variable}.o'))
        suffix = f' selection: {selection}' if selection else ''
        check(f'[[CompiledUnit compile: {json.dumps(text)} named: #{name} output: {path}{suffix}] = {path}]', f'{variable} compiled')
        forms.append(f'let {variable} := [CompiledUnit open: {path}]')
        check(f'[[{variable} state] = #candidate]', f'{variable} opened inertly')
        return path

    forms.append('let initialPolicy := [CompiledUnit activeFor: #cuPolicy]')
    path = candidate('policy', 'cuPolicy', source('cuPolicy', method('cuReplace', 111),
                     method('cuAdd', 222), '[Reflect removeMethod: #cuRemove on: Integer]'),
                     '#resumableRequired')
    check('[[3 cuReplace] = 11]', 'inspection preserves dispatch')
    check('[[policy map] == policy]', 'map returns candidate')
    check('[[3 cuReplace] = 11]', 'mapping preserves dispatch')
    check(f'[[[System readFile: {path}] isError] not]', 'mapping preserves author file')
    forms.append('[[| [policy activate]] value]')
    check('[[policy state] = #active]', 'activation publishes state')
    check('[[CompiledUnit activeFor: #cuPolicy] == policy]', 'active unit published')
    check('[[db currentValue: policy attr: #activationEvent] isNil]', 'activation creates no event facts')
    check('[[3 cuReplace] = 111]', 'replacement active')
    check('[[3 cuAdd] = 222]', 'addition active')
    check('[[3 cuRemove] isError]', 'removal active')
    forms.append('[policy rollback]')
    check('[[policy state] = #rolledBack]', 'rollback publishes state')
    check('[[CompiledUnit activeFor: #cuPolicy] == initialPolicy]', 'rollback restores the explicit baseline entry')
    check('[[db currentValue: policy attr: #activationEvent] isNil]', 'rollback creates no event facts')
    check('[[3 cuReplace] = 11]', 'replacement restored')
    check('[[3 cuRemove] = 22]', 'removal restored')
    check('[[3 cuAdd] isError]', 'addition withdrawn')
    forms.append('[policy rollback]')
    check('[[3 cuReplace] = 11]', 'repeated rollback is inert')

    candidate('missing', 'cuMissing', source('cuMissing', method('cuDependencyGuard', 310), requires='#[#cuUnavailable]'))
    check('[[[missing requires] at: 0] = #cuUnavailable]', 'requires retained')
    check('[[[missing missingDependencies] size] = 1]', 'unloaded dependency reported')
    check('[[[missing map] kind] = #dependenciesMissing]', 'unloaded dependency prevents map')
    check('[[missing state] = #candidate]', 'dependency rejection preserves state')
    check('[[3 cuDependencyGuard] = 31]', 'dependency rejection preserves dispatch')

    candidate('unmapped', 'cuUnmapped', source('cuUnmapped', method('cuUnmapped', 44)))
    check('[[unmapped validate] isError]', 'unmapped validation fails')
    check('[[unmapped state] = #candidate]', 'failed preparation preserves state')
    check('[[3 cuUnmapped] = 4]', 'failed preparation preserves dispatch')
    forms.append('[unmapped discard]')
    check('[[unmapped state] = #discarded]', 'unmapped candidate discarded')

    path = candidate('methodless', 'cuMethodless', source('cuMethodless', '[Reflect removeMethod: #cuMethodless on: Integer]'))
    check('[[methodless map] == methodless]', 'methodless candidate maps')
    forms.append('[methodless activate]')
    check('[[3 cuMethodless] isError]', 'methodless candidate removes binding')
    forms.append('[methodless rollback]')
    check('[[3 cuMethodless] = 6]', 'methodless rollback restores binding')
    check('[[methodless removeFiles] == methodless]', 'explicit file cleanup returns unit')
    check(f'[[System readFile: {path}] isError]', 'explicit cleanup deletes admitted author file')
    check(f'[[System readFile: [{path} , ".candidate"]] isError]', 'explicit cleanup deletes sidecar')
    check('[[methodless discard] == methodless]', 'logical discard returns unit')
    check('[[methodless state] = #discarded]', 'logical discard publishes status')

    candidate('first', 'cuChain', source('cuChain', method('cuInherited', 70)))
    forms.extend(['[first map]', '[first activate]'])
    candidate('second', 'cuChain', source('cuChain', method('cuOverlay', 80)))
    forms.extend(['[second map]', '[second activate]'])
    check('[[first state] = #superseded]', 'predecessor superseded')
    check('[[second state] = #active]', 'successor active')
    check('[[3 cuInherited] = 70]', 'successor inherits untouched binding')
    check('[[3 cuOverlay] = 80]', 'successor overlays binding')
    check('[[Reflect defineMethod: #cuOrdinary on: Integer as: [:self | 700]] == Integer]', 'ordinary running edit allowed')
    check('[[3 cuOrdinary] = 700]', 'ordinary edit is live')
    check('[[Reflect removeMethod: #cuOrdinary on: Integer] == Integer]', 'ordinary removal allowed')
    forms.append('[second rollback]')
    check('[[CompiledUnit activeFor: #cuChain] == first]', 'rollback restores predecessor')
    check('[[first state] = #active]', 'predecessor reactivated')
    check('[[3 cuInherited] = 70]', 'rollback preserves inherited binding')
    check('[[3 cuOverlay] = 8]', 'rollback restores overlaid binding')
    # Discard releases an active inverse and cancels pending tokens; installed code stays live.
    check('[[first discard] == first]', 'active handle can be released')
    check('[[first rollbackAvailable] = false]', 'discard releases undo capability')
    check('[[3 cuInherited] = 70]', 'discard leaves installed code live')
    forms.append(':quit')
    result = subprocess.run([binary, image], cwd=work, env=env,
                            input=('\n'.join(forms) + '\n').encode(),
                            capture_output=True, timeout=900)
    transcript = result.stdout.decode()
    # Retain useful failure evidence even when an earlier form failed to compile.
    assert result.returncode == 0, (result.returncode, transcript, result.stderr)
    assert result.stderr.decode().splitlines() == [
        'unit has no current binding ledger',
        'candidate is not mapped'
    ], (transcript, result.stderr)
    for index, label in enumerate(checks, 1):
        assert f'#[#compiledUnitCheck {index} true]\n' in transcript, (label, transcript)
    assert '!! ' not in transcript, transcript
print(f'compiled-unit: {len(checks)} policy checks through ordinary sends passed')
