"""Admit evidence for a named phase without demanding unrelated cold/timing work."""
from pathlib import Path
from measurement_archive import load_json
from validation_support import ROOT, audit_steps


def load_proof(path, *, measurement=None, applications=False, required=()):
    proof = load_json(path)
    if 'steps' not in proof:
        return proof  # Historical schema retains its original strict admission.
    if proof.get('status') != 'passed': raise ValueError('validation plan is not passed')
    names = ['workspace', 'install'] + (['native'] if applications else []) + list(required)
    if measurement: names.append(measurement)
    visited=audit_steps(proof["steps"], names)
    sources={}
    commands=[]
    for name in visited:
        row=proof['steps'][name]
        sources.update({str(Path(n).relative_to(ROOT)):h for n,h in row['binding']['inputs'].items()})
        commands.append(dict(label=name, argv=row['argv'], exit_code=row['exit_code'],
                             output=Path(row['log']).read_text()))
    result=dict(status='passed', phase_verified=True, production_verification='not run',
                scope='Named phase prerequisites only; no claim of full release qualification',
                sources=sources, commands=commands, closure_source_matrices={}, unboxed_source_matrices={})
    if measurement:
        row=proof['steps'][measurement]
        matrices=[Path(n) for n in row['artifacts'] if n.endswith('.json')]
        if len(matrices)!=1: raise ValueError('measurement phase must bind one matrix')
        data=load_json(matrices[0])
        if data.get('status')!='passed':raise ValueError('measurement matrix is not passed')
        family='closure_source_matrices' if measurement in ('measure-release','numeric-measure-release') else 'unboxed_source_matrices'
        result[family][matrices[0].name]=data
    return result
