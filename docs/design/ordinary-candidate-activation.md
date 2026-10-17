# Candidate activation is an ordinary send

2026-10-17. Follows `1c4b6613`. Implemented and qualified.

`CompiledUnit activate` and `rollback` now perform their operations directly.
They can be called from Blocks, entered Methods, image main, console and TCP.
EvolutionIntent, its fields/facade, the Generation fulfillment entry and dynamic
quiescence flag, and Host result-type detection/fulfillment are removed. The extra
commitActivation/commitRollback facade selectors are removed too. No compatibility
adapter or alternative calling convention is introduced.

This does not remove physical retirement boundaries. Execution retains entered
and captured code, and TCC checkpoints still determine when mappings can be freed.
Generation's private-state reentry exclusion remains; ordinary reflective writes
and setter side effects remain available. The transaction, source and inverse
contracts are unchanged from module-scoped candidates.

Native errors now traverse the ordinary language result boundary (for example,
`#nativeFailure`) rather than the removed Host-specific raw-error path. Explicitly
returned Error objects retain identity. The low-level native API still returns
native status/detail to native callers. There is no new error normalization rule
in Execution and no silent retry of failed construction.

## Evidence

A real Method activates the replacement of its own binding while entered, invokes
GC, and completes its old body; subsequent sends execute the replacement. The test
injects both native failure and an Error from the current state setter, checks
compensation, then rolls back through image main. Native transaction/retirement
checks, all 826 language assertions, 60 CLI checks, console/TCP, relocated Host,
source-free StringBuilder/native-loader images and baked entry points pass.

Lean checks 364 theorems, rejects 107 mutation controls, and TLC obtains all 236
expected outcomes. New laws/models distinguish installation from entered-code
ownership and reject both idle-only admission and premature retirement/frame
replacement. They do not replace native-code lifetime tests.

Stale smoke fixtures that expected ordinary Reflect edits to be rejected were
updated to verify the edits take effect. A test expecting the old raw Host error
text was corrected to assert the existing ordinary `nativeFailure` contract.
See the [receipt](../measurements/ordinary-candidate-activation-20261017.json).

The remaining generation simplification is logical discard versus explicit file
cleanup and optional diagnostics. No user installation or push accompanies this
slice; retained authored facades/images should be rebuilt for the removed APIs.
