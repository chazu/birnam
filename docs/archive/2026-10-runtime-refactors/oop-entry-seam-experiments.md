# Id and Execution entry experiments

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

**Direction superseded:** the user rejected the shared private implementation
packaging. The passing experiments remain evidence about the regression, not
the chosen architecture. See the [continuation-aware Id proposal and formal
checkpoint](oop-continuation-id.md).

2026-10-24. Isolated experiments against `5ec962ff`, on macOS ARM64.
The preferred candidate passes the custom-lookup tail regression without changing
Id's public contract or giving up bounded, interleaved Machine execution.
**Production integration is not implemented.** The tracked runtime and mandatory
failing gate remain unchanged; dependent migration remains held.

[Evidence, source patches, probes and build recipes](../../measurements/oop-entry-seam-20261024.json).
The experimental builds live under `.cargo-target/seam-experiments/`.

## Recommendation and contract

Share the private implementation that selects and enters a Method between Id
and Execution. Keep their existing public interfaces. Resolve once, then either
transfer into the selected registered source frame or enter the exact selected
native declaration. Preserve the original Call/TailCall information locally.

The regression is avoidable complexity at the implementation boundary: Execution
currently guesses whether it can recognize a callee from the kind of lookup,
then loses tail-call information through its native fallback. Neither custom
lookup nor stackless execution requires that split. The existing frame replacement,
registration, capability and code-lifetime algorithms can be reused.

Before:

```text
TailCall -> custom lookup is not classified by Execution
         -> synchronous boundary -> ordinary send -> current lookup
         -> registered source entry -> nested drive -> non-tail frame push
```

Preferred experimental sequence:

```text
Call/TailCall -> the same current lookup used by ordinary send
             -> Method owner retains selected Method, context and code owner
             -> Execution recognizes the selected registration
                -> source: existing frame push/replacement
                -> native: synchronous boundary, enter saved native declaration
```

There is no second lookup on the selected Method, no replay of custom lookup,
and no new Id selector, Protocol field, application protocol or payload catalog.
The private transfer callback is a local Rust continuation; it is not stored,
published as a service, or exposed to language code. The Method owner still reads
and validates its own representation. Execution owns registrations and frames;
TCC continues to own generated code mappings. Other native owners stay separate.

## Alternatives tested

| Candidate | Result | Decision |
| --- | --- | --- |
| Carry a scoped tail hint through the existing native fallback | A lookup callback entering source on the same receiver with the same arity fails. A genuine native callback alone did not expose it. | Reject: a source entry during lookup is not necessarily the pending call's selected callee. More identity flags would complicate the same ambiguous path. |
| One private call-entry implementation, existing Block invocation | Standard native checks and additional callback/interleaving probes pass; custom tail frames stay at 2. | Valid repair candidate. |
| Same implementation, direct private entry for an already captured Method | Same checks pass; removes the temporary argument Sequence and reflective lookup used internally by Block/step invocation. | Preferred: reuse the owner entry operation and remove displaced work. |

The third candidate retains explicit language `Method invokeOn:` behavior. It
changes internal entry of an already captured Method, whose established meaning
and code lifetime must be retained. Future ordinary sends still use current lookup.
This is the simplest safe candidate tested, not proof that every possible smaller
implementation is impossible.

## Acceptance evidence

- The original mandatory source test passes unchanged against the candidate:
  default and custom lookup each retain **2 frames** at both 8 and 24 calls.
  The original runtime retains 9 and 25 under custom lookup.
- All eight native acceptance modes pass, including the negative dispatch
  contract, custom/global lookup, allocation policy, failure containment, real
  compiler/TCC execution, GC, entered-code replacement and code retirement.
- A native callback enters source and returns to its caller correctly. Attempting
  to drive the running Machine recursively is rejected.
- A custom lookup enters source on the pending target's receiver with the same
  arity, then selects the target. Both effects occur exactly once.
- Two Machines alternate one-transition budgets for **1,000,002 transitions**,
  retaining independent state and exact effect counts. GC and TCC checkpoints run
  every 2,048 transitions; final results survive collection.
- Two processes run the additional probes concurrently, each completing 8,002
  transitions. The existing rejection of cross-thread Id entry also passes.
- All **43** combined Id/engine unit tests pass. The independent C caller and
  the System input failure test pass.
- Public Id and Execution export sets match their existing manifests exactly.
  Mixing the old Id with the candidate Execution is refused before Execution
  publication, and the original Id remains usable afterward.

The million-transition and complete native-mode runs were repeated after adding
the mixed-pair initialization guard. The concurrent-process and timing runs
precede that guard; it changes initialization only. Receipts identify the actual
binaries. These checks preserve the general concurrency approach; they do not
qualify the still-unported suspension, cancellation, recovery or worker APIs.
They do not demonstrate multiple thread-hosted runtime domains.

## Cost and packaging

Two fresh processes per variant, in ABBA order, each take five thread-CPU samples
of the existing small synchronous source probes. Setup and explicit GC are outside
the timer. Comparison is with the current Oop branch, not the old complete runtime.

| Probe | Current ns/call | Candidate ns/call | Id allocations/call, current → candidate |
| --- | ---: | ---: | ---: |
| Identity | 356 | 88 | 1 → 0 |
| Increment | 4,304 | 3,993 | 4 → 3 |
| Captured addition | 2,048 | 1,818 | 2 → 1 |

Removing the internal argument Sequence saves one Id allocation per invocation.
These small results are encouraging, not a full-application performance claim.

The concrete prototype uses a private implementation dylib and two thin public
views. Id still exports only `birnam_id_bootstrap_v2`; Execution still exports
only `birnam_native_module_v2`. The private image exports bootstrap plus immutable
Execution declaration arrays and its initializer for linking the views. It does
not export a resolve/enter service or a mutable function table.

This changes physical packaging: **Id and Execution must ship as a matched pair**.
The initializer checks the supplying message function belongs to the same private
implementation before accessing private state. Production packaging must enforce
that closure, including relocatable loading and supported-platform symbol rules.

The measured binaries total 758,088 bytes (690,992 private implementation plus
33,456 and 33,640 for the public views), versus 1,056,832 for the two original
libraries, about 28% smaller combined. However, an Id-only process now loads
724,448 bytes instead of 494,000, about 47% more native file content. Bootstrap's
measured object population is unchanged: 56,805 before collection, 2,917 after,
53,888 reclaimed. Native file size is not a resident-memory measurement.

## Next integration boundary

Promote the shared private entry operation and its ownership arrangement into
the normal source/build structure, deleting the preliminary lookup classification
and internal argument forwarding it replaces. Preserve the public export audit,
mixed-pair refusal, callback counterexamples and original tail regression as
mandatory tests. Then qualify clean builds, relocation and applicable platforms
before lifting the production hold and resuming the remaining compiler/control
migration. No concurrency restriction is justified by these experiments.

This checkpoint records experiments only. It does not install a runtime, complete
the migration or authorize a completion push.
