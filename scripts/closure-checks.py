#!/usr/bin/env python3
"""Standalone entrypoint for the shared closure matrix plan."""
import argparse
import json
from pathlib import Path
from closure_plan import closure_plan
from validation_support import ROOT, PlanRunner, audit_steps


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', required=True, type=Path)
    p.add_argument('--suite', choices=('routine','exhaustive'), default='exhaustive')
    p.add_argument('--output', type=Path, default=ROOT/'.cargo-target/validation-closures')
    p.add_argument('--deadline', type=int, default=1800)
    p.add_argument('--plan', action='store_true')
    args=p.parse_args()
    steps=closure_plan(args.prefix.resolve(), args.output.resolve()/'matrices', args.suite, ())
    if args.plan:
        print(json.dumps(steps,indent=2));return
    runner=PlanRunner(args.output,deadline=args.deadline)
    try:
        for step in steps:runner.run(**step)
        audit_steps(runner.rows,[step['name'] for step in steps])
        runner.receipt['status']='passed'
    except BaseException as error:
        runner.receipt.update(status='failed',error=str(error));raise
    finally:runner.save()

if __name__=='__main__':main()
