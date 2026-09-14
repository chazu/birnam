# 74T verification support integration

Status: production verification complete. This integrates
shared correctness/resource evidence for all four optimization families. It does
not complete a family or promote an optimizing consumer.

## Required result

Main must be able to verify source-level recovery and allocation behavior while
preserving live outer native caller roots, and account separately for baseline
TinyCC allocations and floor-private compiled units. These checks already passed
in the consolidated experimental checkpoint `c24030c9`, but that checkpoint also
contains the held Error-field projection declaration. Separate the shared support
from that declaration and verify the exact composition before integration.

## Implementation

Start from main `f71396f4`. Carry the source-probe native-root boundary, diagnostic
Codegen evaluator-lifetime assertions, and baseline TCC resource metrics from
`c24030c9`. Retain the lifetime and baseline-storage source drivers and their full
production-gate wiring. Keep the main Error module and native-field declaration
contract; the Error-specific optimization/report experiments are not dependencies
of these checks. The lifetime fixture still exercises ordinary Error objects
with the aggregate consumers disabled, so no Error projection declaration is
needed for its ownership assertions.

The source-probe wrapper must retain the caller's original native range while
excluding the probe's own transient scratch. It must not remove ordinary native
caller roots, alter precise/conservative GC, change finalizer decisions, keep a
Unit globally alive, or suppress collection. Real nested synchronous and
persisting evaluations must remain callable after probe collection; scratch-only
objects must still be reclaimable.

Baseline TCC counters are diagnostic-feature-only. They count the relocated
allocation owned by baseline Units separately from private compiler units, handle
repeated queries/relocation/release, and remain absent from ordinary installations.
Existing unit ownership, public IdApi/Method ABI, canonical descriptors/images,
optimization switches and language semantics remain the main contracts.

## Acceptance

1. Inspect the complete diff against main and the consolidated checkpoint. Error
   implementation/declaration and all optimizing consumers must equal main;
   carried support source must equal its already verified checkpoint, except the
   production source inventory excludes unrelated Error experiment inputs.
2. Run the full production recovery gate with unboxed source verification. Keep
   its original assertions and require all75commands, complete debug/release
   source/control matrices,12evaluator-lifetime and12baseline-storage coordinates,
   matching test-floor hashes and unchanged source inventory.
3. Retain and commit the exact-source proof. Only then integrate this branch into
   main. Its proof concerns verification infrastructure; performance/default
   promotion of numeric, loop, aggregate and closure consumers remains governed
   by their own measurements and resource obligations.

The counted-index candidate and direct loop-cache comparison remain frozen in
their separate worktrees while this support composition is verified.

## Completed verification

The exact composition passed all75production commands,3164optimization source
cases,12evaluator/native-root cases and12baseline-storage cases in
`74t-verification-support-production.json`. The independent production audit
rechecked all288current source inputs, every required matrix count/status, both
current diagnostic floor hashes, and complete lifetime/storage coordinates. The
full gate includes installed-prefix, image, cold-archive and relocation checks.
Its temporary installations are not claimed to remain published.

The Error implementation, native-field declaration contract, public ABI and
canonical descriptor/image implementations equal the main starting point. The
carried support source matches `c24030c9`; only the production source inventory
excludes the unrelated Error experiment inputs. This evidence permits integration
of the shared verification support. Optimizing-consumer promotion remains open.
