#!/usr/bin/env python3
"""Ensure the unattended inspector rejects missing or incorrect observations."""
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('inspector', Path(__file__).with_name('code-inspector.py'))
inspector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inspector)


def output():
    return [dict(phase=p, direct=d, answer=a, undocumented=u,
                 aliases=['InspectorChild', 'InspectorAlias'],
                 inspection_fact_delta=0, inspection_cpu_ns=1)
            for p, d, a, u in zip(['initial', 'replaced', 'removed'],
                [['answer'], ['answer', 'extra'], ['extra']], [20, 42, 10],
                [[], ['extra'], ['extra']])] + [dict(history=[20, 42, 10], passed=True)]


def validate(rows):
    return inspector.validate('\n'.join(json.dumps(row) for row in rows))


class Contracts(unittest.TestCase):
    def test_known_world(self):
        self.assertEqual(validate(output()), output())

    def test_incomplete_output(self):
        for rows in (output()[:-1], output()[1:], output()+[{}]):
            with self.assertRaises(AssertionError): validate(rows)

    def test_semantic_corruption(self):
        for index, field, value in [(2, 'answer', 42), (0, 'direct', []),
                (1, 'undocumented', []), (0, 'aliases', ['InspectorChild']),
                (2, 'inspection_fact_delta', 1), (3, 'history', [20, 20, 10])]:
            with self.subTest(field=field):
                rows=output(); rows[index][field]=value
                with self.assertRaises(AssertionError): validate(rows)


if __name__ == '__main__': unittest.main()
