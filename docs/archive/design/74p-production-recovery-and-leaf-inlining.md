# 74P — Production recovery and guarded leaf inlining

Status: complete; qualified timing deferred by user,
2026-09-12.

## Outcome

Implement the first production reconstruction operation and connect it to a
normal-path optimizer. Preserve ordinary Id semantics, the public IdApi/Method
ABI, descriptor-v2 bytes, images, owner-thread execution and native-entry barriers.
60P supplies the completed V2 control substrate. 74O supplies feasibility tests,
not production authority; this implementation must replace that distinction with
checked ownership and real driver integration.

## Bounded first consumer

Recognize a compiler-lowered, single-PC portable BlockBody whose only operations
are entry receiver/argument/capture definitions and whose terminator returns one
of those values. This includes Id methods implemented by projection blocks and
identity closures. After 64 calls, inline the return in the caller's driver:
retain the resolved active implementation, select its compiler-defined entry
slot, and commit the caller's pending Call edge without constructing a callee
Frame or invoking its StepMethod and canonical return microcode.

Target lookup remains ordinary and runs on every call. No method-name whitelist,
result cache or inference about arbitrary native code is used. Descriptor bytes
must match the admitted compiler output. Code ownership stays acquired until the
return or recovery completes. Registry eviction withdraws optimization without
invalidating an acquired activation.

The guard covers canonical Machine slot/return operations, lookup machinery and
resumable classification. Ineligible or mutated microcode recovers the original
callee at PC zero before it executes. That lets ordinary overridden operations
observe the concrete callee. The leaf has no interior callback or source-level
allocation. Preserve baseline resume-token and return-index boxing; if collection
could occur during those allocations, recover first. This avoids moving GC or
finalizer observations across an absent Frame. No source-level object allocation
is removed, and no existing identity is replaced.

This is deliberately an entry-guarded projection inline, not general inlining,
unboxing, scalar replacement or mid-body speculation. Production recovery may
admit other compiler PCs; this consumer needs only the entry recovery point.

## Recovery admission

A recovery request owns an already acquired registered callee and binds it to
one Machine, drive epoch, exact caller Frame/descriptor, pending Call edge and
result destination. It cannot import a raw descriptor pointer or executable
address from Id. The callee descriptor comes from the owned registration.

The shared floor operation validates the complete recipe before mutation:

- owner, Machine lifecycle, caller identity/epoch and exact pending Call;
- active acquired generation, descriptor identity and available pin capacity;
- callee PC, destination/source bounds, unique assignments and every live root;
- existing entry barriers and a single reconstructed child with the original
  caller and return destination retained.

Only after all checks pass may it install the child and transfer the acquired
entry to a Frame pin. Failure leaves Frames, pending edges and ownership intact.
This atomicity applies to the floor transfer; ordinary effects during recipe
decoding are retained. Transfer executes without Id sends, Birnam allocation, GC
or foreign callbacks.
Ordinary return/capture/restore/retirement subsequently own the reconstructed
Frame through their existing paths.

Expose the mechanism as the ordinary, replaceable Machine message
`recoverAt:with:` during an authorized recovery request. Its first argument is a
resume PC. Its second is an Id-described slot vector answering `size` and `at:` for each
physical destination, including explicit nil values. Decode and root these
values before the atomic transfer; revalidate authority after all Id callbacks.
The request is scoped and one-shot. Outside that scope, or with stale authority,
the operation rejects. Id policy constructs the values and chooses the resume
point; the floor checks representation and ownership, not language policy.

The optimizing consumer and the ordinary method use the same checked transfer.
The consumer can use an owned native recipe directly; the ordinary message must
also be tested with a recipe produced by Birnam code. No extra IdApi field,
external shim, image section or serialized StepAction kind is introduced.

## Implementation sequence and acceptance

1. Move the checked reconstruction core into production and add registration/pin
   admission. Keep full-state negative tests from 74O and extend ownership tests.
2. Add the scoped ordinary Id recipe operation and its rejection/GC checks.
3. Derive the projection plan from portable compiler IR, retain it through
   registration, and invoke it on hot registered Calls in the real driver.
4. Test baseline versus optimized results and identity, Machine/lookup overrides,
   target replacement, captures, GC thresholds, generation retirement, invalid
   recipes, wrong Machine/epoch and one-shot use. Exercise compiler-generated
   artifacts and ordinary Id recipe construction, including source-independent
   compilation. Retain V2 capture/restore and native participation regressions.
5. Measure a warmed projection-heavy fixture with independent baseline/control
   and optimized runs. Separate warmup, steady execution, allocations and actual
   inline/recovery counts. Host-gated timing must be accepted before claiming a
   speed benefit; correctness and implementation completion are separate facts.
6. Run debug/release tests, frozen-authority checks and the full production gate;
   record commands, source hashes, counts and measured results. Update tracker,
   changelog and current design navigation. No commit/push is implied by this task.

Do not expand to general inlining until this slice has both a correct recovery
contract and measurements identifying its actual benefit or bottleneck.

## Implementation decisions

The production core lives in `execution/recovery.rs`; the old 74O module retains
only test snapshots. Registration acquisitions are checked before transfer and
become Frame pins only after complete admission. The scoped Machine operation
roots decoded values in reverse-release order and revalidates the caller after
Id sends. The first consumer uses an owned dense native recipe through the same
engine; an independently compiled captured Id block supplies an ordinary recipe
in the integration test.

The inline does not cache target lookup. It avoids a callee Frame, StepMethod
entry and canonical return dispatch. It preserves PC/slot boxing and the
reflective scratch ReturnAction. A persistent microcode guard miss reconstructs
once, then uses baseline entry until dictionary mutation permits another attempt.
A collection-boundary miss is retried after collection rather than permanently
withdrawing the optimization.

The executable test fixture compiles both caller and callee from portable
CodePrograms and registers them through production staging. It covers argument
and capture identity, exact allocation-byte equality, overridden return/lookup,
private Machines, GC observing the reconstructed callee, current target changes,
non-entry recovery, an Id-authored captured recipe, one-shot scope, malformed
recipes, stale authority, capture/restore and generation retirement. A separate
installed-prefix fixture materializes actual Birnam source-independent code,
warms it, evicts the compilation cache and collects before checking identity again.

Timing uses one release test binary with independently selected baseline/inline
records, identical compiler artifacts and an unoptimized control body. It records
warmup and steady execution separately and checks actual hit/recovery counters.
`python3 scripts/production-recovery.py --measure --verify` runs all checks; the
repository host gate requires AC power and acceptable competing load. Battery
measurements do not qualify a speed claim.

## Lifetime regression exposed by integration

The installed-prefix lifecycle gate initially retained one extra native Unit.
Root tracing identified a pending retirement generation ID on the conservative
native stack: JIT IDs were the exact Unit payload address. Adding the recovery
selector changed allocation layout enough to expose this existing self-rooting
hazard, even with the optimizer disabled. JIT generation keys now tag the aligned
Unit address with bit zero set, so the scalar key cannot match an Oop payload
address. Generated carriers, staging, cache eviction and rebuilt images use the
same key. Explicit Unit references and generation leases remain the ownership
authority; mapped generation IDs and serialized artifact formats are unchanged.
The unchanged portable lifecycle smoke now passes with production recovery enabled.

## Verification record

`python3 scripts/production-recovery.py --verify` completed with exit zero.
[The receipt](../../measurements/74p-production-recovery.json) records source hashes,
frozen ABI/descriptor/image authorities and complete command output:

- All three production integration tests pass in debug and release, with ten
  matching result rows in each profile.
- The complete workspace passes: 458 tests, zero failures, 12 intentionally
  ignored experiments. Native-prefix, image/lifecycle, V2 control, two independent
  cold archives and relocated-runtime gates also pass.
- Argument and capture fixtures each record 16 warmed inline hits, with identical
  baseline/optimized allocation totals of 11,648 bytes. These are work/identity
  checks, not a wall-clock speed claim.
- New Rust files pass formatting checks; the Python runner and shell hook pass
  syntax checks; `git diff --check` is clean.

[The earlier timing attempt](../../measurements/74p-timing-blocked-battery.json)
was rejected by the host gate before any timing samples: the machine was on
battery. That receipt describes an earlier source snapshot, before the lifetime
fix; it is retained only as evidence of the timing blocker. The temporarily
paused `rk` process was resumed. No qualified speedup is claimed.

Implementation and correctness work are complete. The user explicitly requested
completion without qualified timings; `birnam-06p` is closed on that basis. The
seven paired projection/fallback/control measurements remain a future performance
follow-up, with no speedup claim. Run
`python3 scripts/production-recovery.py --measure --verify` on a qualified host
before broadening the optimizer. This implementation and the 74Q roadmap were
subsequently committed as `d15a40ce` before beginning Stage 1; no push was requested.

## Follow-on roadmap

[74Q](74q-optimization-roadmap.md) records the subsequent exploratory timing
evidence and proposes the staged optimization roadmap. Qualified performance
acceptance remains deferred; the next proposed batch broadens small-callee
inlining and compiler-generated recovery plans.
