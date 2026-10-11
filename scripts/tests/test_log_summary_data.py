import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('log_summary_data',Path(__file__).resolve().parents[1]/'log_summary_data.py')
data=importlib.util.module_from_spec(spec)
spec.loader.exec_module(data)

class LogDataTests(unittest.TestCase):
    def test_seed_and_golden_fixture(self):
        with tempfile.TemporaryDirectory() as a,tempfile.TemporaryDirectory() as b:
            left=data.generate(a);right=data.generate(b)
            self.assertEqual(left,right)
            for path in Path(a).iterdir():
                self.assertEqual(path.read_bytes(),(Path(b)/path.name).read_bytes())
            tiny=json.loads((Path(a)/'tiny.expected.json').read_text())
            self.assertEqual(tiny,{'format':'log-summary-v1','requests':3,'errors':2,'bytes':1960,
                                  'endpoints':[{'path':'/','requests':1,'errors':1,'bytes':0},
                                               {'path':'/search','requests':2,'errors':1,'bytes':1960}]})

    def test_oracle_rejects_boolean_numbers_and_preserves_error_boundary(self):
        with self.assertRaises(ValueError):
            data.report('{"path":"/","status":true,"bytes":0,"duration_ms":0}')
        text='\n'+data.encode([dict(path='/',status=399,bytes=1,duration_ms=0),
                               dict(path='/',status=400,bytes=2,duration_ms=0)])
        self.assertEqual(data.report(text)['errors'],1)
        self.assertEqual(data.report(text)['bytes'],3)

if __name__=='__main__': unittest.main()
