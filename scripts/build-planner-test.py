#!/usr/bin/env python3
"""Independent planner contracts and receipt rejection tests."""
import importlib.util
import unittest
import json
from copy import deepcopy
from itertools import product
from pathlib import Path
from build_planner_oracle import fixture, expected, plan, source, validate

spec = importlib.util.spec_from_file_location('runner', Path(__file__).with_name('build-planner.py'))
runner = importlib.util.module_from_spec(spec); spec.loader.exec_module(runner)


class Contracts(unittest.TestCase):
    def test_known_scenario(self):
        rows, history = expected(fixture('shared', 4, 10))
        self.assertEqual(rows[0]['plan']['order'], ['#m00000','#m00001','#m00002'])
        self.assertEqual(rows[1]['plan']['order'], [])
        self.assertEqual(rows[2]['plan']['order'], ['#m00002'])
        self.assertEqual(rows[3]['plan']['order'], ['#m00000','#m00001','#m00002'])
        self.assertEqual(rows[4]['plan']['changed'], [['#m00003','unbuilt']])
        self.assertEqual(rows[5]['plan']['changed'], [['#m00003','dependencies']])
        self.assertEqual(rows[6]['plan']['missing'], [['#m00001','#m00000'],['#m00003','#m00000']])
        self.assertEqual(rows[6]['plan']['order'], [])
        self.assertEqual(rows[8]['plan']['blocked'], ['#m00000','#m00001','#m00002'])
        self.assertEqual(rows[8]['plan']['order'], [])
        # Failed plans publish no artifacts. Restoration creates a new source
        # state while preserving the current successful artifacts, not old ones.
        self.assertEqual([r[2] for r in rows[6]['world']], [r[2] for r in rows[7]['world']])
        self.assertEqual([r[1] for r in rows[9]['world']], [r[1] for r in rows[0]['world']])
        self.assertNotEqual(rows[9]['world'], rows[0]['world'])
        for past in history:
            self.assertEqual(past['plan'], rows[past['index']]['plan'])
            self.assertEqual(past['world'], rows[past['index']]['world'])

    def test_all_three_node_graphs(self):
        ids = ['a','b','c']; edges = [(a,b) for a in ids for b in ids]
        for enabled in product((False,True), repeat=len(edges)):
            graph = {k:[b for (a,b),yes in zip(edges,enabled) if yes and a==k] for k in ids}
            sources = {k:source(k,graph[k]) for k in ids}
            answer = plan(sources,{},ids)
            # Independent transitive closure identifies cycles and everything
            # depending on them; graphlib's cycle exception only names one cycle.
            reach = {k:set(v) for k,v in graph.items()}
            for _ in ids:
                reach = {k: ds | set().union(*(reach[d] for d in ds)) for k,ds in reach.items()}
            cyclic = {k for k in ids if k in reach[k]}
            blocked = ['#'+k for k in ids if k in cyclic or reach[k]&cyclic]
            self.assertEqual(answer['blocked'],blocked)
            if not blocked:
                positions = {k[1:]:i for i,k in enumerate(answer['order'])}
                self.assertEqual(set(positions),set(ids))
                for a,b in edges:
                    if b in graph[a]: self.assertLess(positions[b],positions[a])

    def test_changed_dependency_closure(self):
        for shape in ('chain','wide','shared'):
            data=fixture(shape,8,10); src=dict(data['revisions'][0]['edits'])
            artifacts={k:dict(source=deepcopy(v),revision=0) for k,v in src.items()}
            for name in src:
                changed=deepcopy(src); changed[name]['digest']='edited'
                answer=plan(changed,artifacts,data['ids'])
                affected={name}
                while True:
                    expanded=affected|{k for k,v in src.items() if set(v['deps'])&affected}
                    if expanded==affected: break
                    affected=expanded
                self.assertEqual(set(answer['order']),{'#'+k for k in affected})

    def test_validation(self):
        data=fixture(); data['revisions'][1]['restore']=1
        with self.assertRaises(ValueError): validate(data)
        data=fixture(); data['revisions'][0]['edits'].append(data['revisions'][0]['edits'][0])
        with self.assertRaises(ValueError): validate(data)

    def test_receipt_rejects_corrupted_world_and_plan(self):
        data = fixture('shared',4,10); revisions, past = expected(data)
        rows = [dict(kind='memory', index=-1, objects=100, backend={})]
        for row in revisions:
            names = ['edit_read','edit','read','plan','checkpoint','publish']
            if data['revisions'][row['index']]['restore'] is not None: names.append('restore_read')
            rows.append(dict(kind='revision', **row,
                             times={n:dict(cpu_ns=1,wall_ns=1) for n in names}))
            rows.append(dict(kind='memory',index=row['index'],objects=100,backend={}))
        rows += [dict(kind='historical',**row) for row in past]
        def output(value): return '\n'.join(map(json.dumps,value))+'\ndone\n'
        runner.check_output(output(rows),data)
        corrupted=deepcopy(rows); corrupted[1]['world'][0][1][0]='wrong digest'
        with self.assertRaises(ValueError): runner.check_output(output(corrupted),data)
        corrupted=deepcopy(rows); corrupted[-1]['plan']['order']=[]
        with self.assertRaises(ValueError): runner.check_output(output(corrupted),data)

    def test_receipt_rejects_missing_results(self):
        with self.assertRaises(ValueError): runner.check_output('done\n',fixture())
        with self.assertRaises(ValueError): runner.check_output('',fixture())


if __name__ == '__main__': unittest.main()
