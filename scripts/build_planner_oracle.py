"""Deterministic project manifests and an independent full-recompute planner.

The oracle uses Python's graphlib, plain copied dictionaries, and source signatures;
it neither calls Birnam nor consumes the Birnam implementation's answers.
"""
from copy import deepcopy
from graphlib import TopologicalSorter, CycleError
from hashlib import sha256


def source(name, dependencies=(), version=0):
    return dict(digest=sha256(f'module {name}\nversion {version}\n'.encode()).hexdigest(),
                deps=sorted(set(dependencies)))


def fixture(shape='shared', size=8, history=12):
    if shape not in ('chain', 'wide', 'shared') or size < 4 or history < 10:
        raise ValueError('shape chain/wide/shared, size >= 4, history >= 10 required')
    ids = [f'm{i:05}' for i in range(size)]
    rows = {}
    for i, name in enumerate(ids[:-1]):  # final id is reserved for addition
        dependencies = ([] if i == 0 else [ids[i-1]]) if shape == 'chain' else (
            [] if i == 0 else [ids[0]]) if shape == 'wide' else (
            [ids[j] for j in sorted({max(0, i-1), i//2}) if j < i])
        rows[name] = source(name, dependencies)
    revisions = []
    def edit(label, pairs=(), restore=None):
        revisions.append(dict(label=label, edits=list(pairs), restore=restore))
    edit('initial', rows.items())
    edit('no-op')
    leaf = ids[-2]
    edit('leaf-source', [(leaf, source(leaf, rows[leaf]['deps'], 1))])
    edit('root-source', [(ids[0], source(ids[0], (), 1))])
    edit('add-module', [(ids[-1], source(ids[-1], [leaf]))])
    edit('change-dependencies', [(ids[-1], source(ids[-1], [ids[0]]))])
    edit('remove-required-module', [(ids[0], None)])
    edit('restore-valid-revision', restore=3)
    edit('introduce-cycle', [(ids[0], source(ids[0], [leaf], 1))])
    edit('restore-initial-sources', restore=0)
    for i in range(10, history):
        edit('history-edit-' + str(i), [(leaf, source(leaf, rows[leaf]['deps'], i))])
    return dict(schema=1, shape=shape, ids=ids, revisions=revisions,
                queries=sorted({0, 3, 6, 8, len(revisions)-1}))


def validate(data):
    ids = data['ids']; revisions = data['revisions']
    if data['schema'] != 1 or not ids or ids != sorted(set(ids)):
        raise ValueError('ids must be sorted, unique, nonempty')
    for i, rev in enumerate(revisions):
        restore = rev['restore']
        if restore is not None and (type(restore) is not int or not 0 <= restore < i or rev['edits']):
            raise ValueError('restoration must reference an earlier revision and contain no edits')
        edited = []
        for name, row in rev['edits']:
            if name not in ids or name in edited:
                raise ValueError('unknown or repeated edited module')
            edited.append(name)
            if row is not None:
                if not isinstance(row['digest'], str) or not row['digest']:
                    raise ValueError('nonempty source digest required')
                if row['deps'] != sorted(set(row['deps'])) or any(d not in ids for d in row['deps']):
                    raise ValueError('dependencies must be distinct known ids in order')
    if any(type(i) is not int or not 0 <= i < len(revisions) for i in data['queries']):
        raise ValueError('invalid historical query')


def plan(sources, artifacts, ids):
    active = {k: sources[k] for k in ids if sources.get(k) is not None}
    changed = []
    for name, row in active.items():
        old = artifacts.get(name, {}).get('source')
        if old != row:
            reason = 'unbuilt' if old is None else 'source' if old['digest'] != row['digest'] else 'dependencies'
            changed.append([name, reason])
    missing = [[name, dep] for name, row in active.items() for dep in row['deps'] if dep not in active]
    sorter = TopologicalSorter({k: [d for d in row['deps'] if d in active] for k, row in active.items()})
    try:
        sorter.prepare()
    except CycleError:
        pass  # graphlib still permits draining every unblocked node
    topology = []
    while sorter.is_active():
        ready = sorter.get_ready()
        topology.extend(ready)
        sorter.done(*ready)
    visited = set(topology)
    blocked = [k for k in active if k not in visited]
    # Independently recompute affectedness from dependency closure in topo order.
    affected = {k for k, _ in changed}
    for name in topology:
        if any(dep in affected for dep in active[name]['deps']):
            affected.add(name)
    valid = not missing and not blocked
    return dict(changed=[['#'+k, reason] for k, reason in changed],
                order=['#'+k for k in topology if k in affected] if valid else [],
                missing=[['#'+k, '#'+d] for k, d in missing],
                blocked=['#'+k for k in blocked], valid=valid)


def wire(sources, artifacts, ids):
    def row(value):
        return None if value is None else [value['digest'], ['#'+d for d in value['deps']]]
    return [['#'+k, row(sources.get(k)), None if k not in artifacts else
             [artifacts[k]['revision'], row(artifacts[k]['source'])]] for k in ids]


def expected(data):
    validate(data)
    sources, artifacts, checkpoints, results = {}, {}, [], []
    for i, revision in enumerate(data['revisions']):
        if revision['restore'] is not None:
            sources = deepcopy(checkpoints[revision['restore']][0])
        else:
            for name, row in revision['edits']:
                sources[name] = deepcopy(row)
        answer = plan(sources, artifacts, data['ids'])
        results.append(dict(index=i, label=revision['label'], plan=answer,
                            world=wire(sources, artifacts, data['ids'])))
        checkpoints.append((deepcopy(sources), deepcopy(artifacts)))
        if answer['valid']:
            for name in answer['order']:
                key = name[1:]
                artifacts[key] = dict(source=deepcopy(sources[key]), revision=i)
    past = [dict(index=i, plan=plan(*checkpoints[i], data['ids']),
                 world=wire(*checkpoints[i], data['ids'])) for i in data['queries']]
    return results, past
