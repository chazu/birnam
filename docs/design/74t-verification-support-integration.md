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

## Repeated resource evidence on the integrated main baseline

`74t-main-repeated-memory.json` completes three alternating enabled/disabled
pairs for all five consumer configurations, with both admitted-shape and refused
controls: 60 normal-release processes, 420 explicit collection/host checkpoints
and 30,720 checked wrapper invocations. Calls after cache eviction also pass.
The independent audit revalidates all coordinates, generated phase inputs,
result/identity-check completion, normal process exits, the full 75-command
production proof, current sources and every published installation artifact.
This uses the integrated `ef00b7a5` support baseline and the matching
`/tmp/birnam-74y-main-baseline-prefix` installation; held counted-index,
String-cleanup and scoped-control changes are absent.

The following are enabled-minus-disabled footprint growth from each process's
own setup checkpoint, in rounded MiB. Each cell retains all three paired
observations for the admitted source shape:

| Consumer | Final steady checkpoint | Sampler references released |
| --- | --- | --- |
| Unboxed numeric | -13.0, +0.4, -0.1 | -12.0, 0.0, 0.0 |
| Loop invariants | +6.7, +6.2, +4.8 | +5.6, +4.3, +3.3 |
| Discarded clones | +1.2, -7.4, +6.6 | +1.3, -7.4, +6.7 |
| Virtual fields | -0.1, -0.3, +0.4 | 0.0, -7.7, +0.4 |
| Closures | -0.6, +0.5, -6.8 | -0.4, +0.5, -5.9 |

Refused controls also vary. These whole-process samples do not establish an
attributable memory saving, activation in the normal sampler or leak freedom.
Clearing sampler references still permits independent source-factory and
compiler/runtime roots. All observations remain `qualified: false`.

The consistent loop footprint increase was investigated using the retained
reports, rather than treating it as optimizer-owned memory. In
`74t-main-loop-memory-accounting.json`, all three admitted pairs have identical
rounded malloc allocated totals at setup (50.0M), both steady checkpoints
(91.7M and 91.8M) and release (91.6M). Allocator fragmentation and dirty/swapped
totals differ. This supports allocator/page accounting as an explanation for
the footprint difference; it does not isolate the cause of that difference.

The same-source, shorter diagnostic loop control separately reports two private
units and 81,920 relocated bytes with the flag either off or on, equal logical
allocation charge, and a 400-byte enabled cache. Its independent-invocation
refusal control similarly retains three units and 131,072 relocated bytes in
both modes. Those shorter controls do not prove ownership stability throughout
the long normal-floor memory run. Repeated full teardown and general compiler
metadata accounting remain broader resource obligations.
