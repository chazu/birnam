"""Closure test plan data. Each build/matrix is independently recorded and reusable."""
from pathlib import Path
from validation_support import ROOT

PRODUCTION = ('Cargo.toml', 'Cargo.lock', 'rust/**/*', 'src/id/**/*', 'modules/**/*', 'vendor/libtcc-src/**/*')
TARGET = ROOT / '.cargo-target/74t-closure-control-target'
FEATURES = 'birnam-runtime/ffi-test-hooks,birnam-codegen-runtime/test-evaluator-lifetime,birnam-tcc/allocation-metrics'


def closure_plan(prefix, output, suite='exhaustive', installed_dependency=('install',)):
    output = Path(output)
    profiles = ('release',) if suite == 'routine' else ('debug','release')
    steps=[]
    def add(name, argv, *, script, artifacts=(), depends=(), timeout=180):
        shared_sources = {
            'scripts/closure-string-cleanup-smoke.py': ('scripts/closure-sequence-smoke.py',),
            'scripts/evaluator-unit-lifetime-smoke.py': ('scripts/aggregate-source-smoke.py',),
        }.get(script, ())
        steps.append(dict(name=name, argv=[str(a) for a in argv], inputs=PRODUCTION +
                          (script, 'scripts/closure_plan.py', 'scripts/fixtures/*', 'scripts/*_common.py',
                           'scripts/measurement_archive.py') + shared_sources + (("scripts/recovery-graphs.py",) if script == "scripts/closure-matrix-verify.py" else ()), artifacts=artifacts,
                          depends=depends, timeout=max(timeout, 900) if suite == "exhaustive" else timeout))
    add("normal-probe-absence", ["bash", "scripts/closure-probe-absence.sh", prefix],
        script="scripts/closure-probe-absence.sh", depends=installed_dependency)
    for profile in profiles:
        build='diagnostics-'+profile
        add(build, ['cargo', 'build', '--locked','--offline', '--target-dir', TARGET,
                    '-p','birnam-id','-p','birnam-codegen-runtime','-p','birnam-tcc','--features',FEATURES]
                    + (['--release'] if profile=='release' else []), script='scripts/closure_plan.py',
                    artifacts=tuple(str(TARGET/profile/name) for name in ('libbirnam_id.dylib','libbirnam_codegen_runtime.dylib','libbirnam_tcc.dylib')), timeout=300)
        floor=TARGET/profile/'libbirnam_id.dylib'
        dependencies=installed_dependency+(build,)
        def matrix(label, driver, flags=()):
            receipt=output/f'{label}-{profile}.json'
            add(label+'-'+profile, ['python3','scripts/'+driver,'--prefix',prefix,'--floor',floor,'--output',receipt,*flags],
                script='scripts/'+driver, artifacts=(str(receipt),), depends=dependencies)
        coverage=['--coverage','routine'] if suite=='routine' else []
        for label, driver, flags in (
            ('literal-control','closure-literal-control-smoke.py',[]),
            ('literals','closure-literals-smoke.py',[]),
            ('sequence','closure-sequence-smoke.py',coverage),
            ('mutation','closure-sequence-mutation-smoke.py',[]),
            ('budget','closure-budget-smoke.py',coverage),
            ('numeric-actions','closure-numeric-actions-smoke.py',[]),
            ('numeric-inputs','closure-numeric-inputs-smoke.py',[]),
            ('numeric-source','closure-literals-smoke.py',['--numeric-body']),
            ('numeric-control','closure-literal-control-smoke.py',['--numeric-body']),
            ('numeric-sequence','closure-sequence-smoke.py',['--numeric-body',*coverage]),
            ('numeric-budget','closure-budget-smoke.py',['--numeric-body',*coverage]),
            ('numeric-string-cleanup','closure-string-cleanup-smoke.py',[]),
            ('journal','closure-journal-smoke.py',[])):
            matrix(label,driver,flags)
        for counting in ('off','on'):
            for family in ('sequence','budget','control','inputs'):
                driver={'control':'closure-literal-control-smoke.py','inputs':'closure-numeric-inputs-smoke.py'}.get(family,f'closure-{family}-smoke.py')
                flags=['--numeric-copy-region']
                if family!='inputs':flags+=['--numeric-body']
                if counting=='on':flags+=['--counted-frame-indices']
                if family in ('sequence','budget'):flags+=coverage
                matrix(f'joint-{family}-{counting}',driver,flags)
        add('api-control-'+profile, ['bash','scripts/closure-api-control-smoke.sh',prefix,floor,profile],
            script='scripts/closure-api-control-smoke.sh', depends=dependencies)
        if suite=='exhaustive':
            for label,driver,flags in (
                ('measure','closure-measurement-smoke.py',[]),
                ('numeric-measure','closure-measurement-smoke.py',['--numeric-body']),
                ('unboxed','unboxed-numeric-smoke.py',[]),
                ('unboxed-captured','unboxed-numeric-smoke.py',['--captured-inputs']),
                ('unboxed-control','unboxed-numeric-control-smoke.py',[]),
                ('unboxed-measure','unboxed-numeric-measurement-smoke.py',[]),
                ('loop-measure','loop-measurement-smoke.py',[]),
                ('loop-invariant','loop-invariant-numeric-smoke.py',['--profile',profile])):
                if 'measurement' in driver: flags+=['--profile',profile]
                matrix(label,driver,flags)
    dependencies=installed_dependency+tuple('diagnostics-'+profile for profile in profiles)
    for label, driver, extra in (
        ('evaluator-lifetime','evaluator-unit-lifetime-smoke.py',['--floor-root',TARGET]),
        ('baseline-native-storage','baseline-native-storage-smoke.py',['--heap-accounting'])):
        receipt=output/(label+'.json')
        add(label, ['python3','scripts/'+driver,'--prefix',prefix,'--target',TARGET,'--output',receipt,
                    '--profiles',*profiles,*extra], script='scripts/'+driver, artifacts=(str(receipt),), depends=dependencies)
    if suite == 'exhaustive':
        add('complete-matrix-oracle', ['python3','scripts/closure-matrix-verify.py','--directory',output,'--all'],
            script='scripts/closure-matrix-verify.py', depends=tuple(s['name'] for s in steps))
    return steps
