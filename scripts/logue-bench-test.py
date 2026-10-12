#!/usr/bin/env python3
"""Analytical oracle checks and negative tests for the acceptance checker."""
import importlib.util
import json
from pathlib import Path
import unittest
from copy import deepcopy
from logue_oracle import Arena, RNG, generation, expected

spec=importlib.util.spec_from_file_location('logue_bench',Path(__file__).with_name('logue-bench.py'))
bench=importlib.util.module_from_spec(spec); spec.loader.exec_module(bench)

class OracleTests(unittest.TestCase):
    def test_rng_known_first_step(self):
        rng=RNG(0)
        self.assertEqual(rng.n(2147483648),1103527590)

    def test_quiet_turns_do_not_consume_randomness(self):
        arena=Arena(0,8,0)
        for _ in range(3): arena.step()
        state=arena.snapshot()
        self.assertEqual((state['turn'],state['rng'],state['player']['hp'],state['player']['hunger']), (3,1,10000,997))
        self.assertEqual(state['monsters'],[])

    def test_one_monster_approaches_then_attacks(self):
        arena=Arena(0,3,1)
        for y in (2,3,4):
            arena.step(); self.assertEqual(arena.monsters[0]['y'],y)
            self.assertEqual(arena.p['hp'],10000)
        arena.step()
        self.assertEqual(arena.p['hp'],9999)
        self.assertEqual(arena.rng.state,1103527590)
        self.assertEqual(arena.messages,1)
        arena.step(frozen=True)
        self.assertEqual((arena.p['hp'],arena.rng.state,arena.turn,arena.p['hunger']), (9999,1103527590,5,995))

    def test_messages_stay_bounded_under_multiple_attacks(self):
        arena=Arena(12345,16,8)
        for _ in range(64):
            arena.step()
            self.assertLessEqual(arena.messages,6)
        self.assertEqual(arena.messages,6)

    def test_generation_structure_and_seeds(self):
        a=generation(12345)
        self.assertEqual(a,generation(12345))
        self.assertNotEqual(a,generation(777))
        self.assertEqual(set(a['levels']),{'1','2','3','4','5'})
        for depth,level in a['levels'].items():
            x,y=level['up']; self.assertEqual(level['tiles'][str(y*60+x)],'up')
            self.assertTrue(5<=len(level['rooms'])<=7)
        self.assertEqual(sum(i['kind']=='macguffin' for level in a['levels'].values() for i in level['items']),1)

    def test_checker_rejects_bad_state_and_missing_checkpoints(self):
        config=dict(phase='arena',seed=0,width=8,monsters=1,turns=8,batches=2,samples=1,allocation=False,replace=True)
        states,replacement=expected(config)
        good=dict(config={'#'+k:v for k,v in config.items()},replacement=replacement,results=[[dict(sample=1,batch=i+1,cpu_ns=1,wall_ns=1,state_json=json.dumps(s)) for i,s in enumerate(states[0])]])
        bench.check_report(good,config)
        bad=deepcopy(good); wrong=json.loads(bad['results'][0][0]['state_json']); wrong['rng']+=1
        bad['results'][0][0]['state_json']=json.dumps(wrong)
        with self.assertRaisesRegex(ValueError,'rng'): bench.check_report(bad,config)
        bad=deepcopy(good); bad['results'][0].pop()
        with self.assertRaisesRegex(ValueError,'missing checkpoints'): bench.check_report(bad,config)
        bad=deepcopy(good); bad['replacement']['player']['#hp']-=1
        with self.assertRaisesRegex(ValueError,'replacement'): bench.check_report(bad,config)

if __name__=='__main__': unittest.main()
