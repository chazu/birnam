# 74J — Guarded Machine sends inside the existing runtime

Status: authorized by the user's 2026-09-11 clarification; implemented and
correctness-verified. The original longer E1 measurement remains unqualified
after two host-gate rejections. The combined 74J/74K/74L implementation later
meets the separate portable-fixture criterion in [74L](74l-guarded-default-lookup.md).
The protocol below preceded this change and its original measurements.

## Problem and scope

The user permits changes to runtime internals while preserving the public API,
ABI, full malleability and expressibility in Id-layer semantics. 74I's public
interface counterexamples therefore do not require stopping all runtime work.

The first production component is a guarded implementation of the existing
`oopAt:` and `oopAt:put:` sends inside `Runtime::send_abi`. Generated code and
native clients continue to issue exactly the same messages with the same
operands. The runtime may execute the canonical operation directly after a
non-effectful check, otherwise it follows the existing resolver and invocation
path. No new native helper, IdApi field, language selector, Frame layout,
descriptor, image hash or artifact registration contract is introduced.

This promotes guarded execution and immediate fallback for ordinary compiled
code. It does not by itself complete optimized native-tier admission from
74F/74G, or qualify 74H read caching. Runtime-owned native-tier admission is
subsequently implemented in [74K](74k-runtime-owned-native-tier.md); 74H read
caching remains deferred.

## Semantic obligations

- Preserve the send's argument-buffer validation and outer dispatch-read
  lease, including retirement processing when it ends.
- Optimize only the canonical Machine receiver and canonical read/write and
  lookup implementations. A private vtable, override, removal, inheritance
  change or custom lookup must retain ordinary dispatch and its effects.
- Do not cache Frame contents, operands, method results or a mutable Frame
  pointer. Index boxes and their allocation order remain unchanged.
- Retain canonical arity/type/index validation and every owner, capability,
  lifecycle, Frame-generation and slot-kind check.
- A method address registered as a resumable carrier must still pass through
  the ordinary resolver, even if its address equals a canonical floor method.
  Registration state is a separate dependency from dictionary mutation.
- A saturated dictionary epoch must not preserve an earlier cached verdict.
- The optimization is equivalent to the original Id send. It adds no
  primitive behavior that the original send cannot express.

## Validation and measurement protocol

Compare optimization enabled/disabled under a test-only control using the
same live Frame and native arguments. Prove activation independently of
answers. Cover valid and invalid access, mutation and restoration, native and
resumable replacement, custom lookup effects, private receiver vtables,
epoch saturation, GC mutation and native retirement. Retain existing ordinary
compiler, same-frame recovery and public-ABI tests. Full repository verification
must pass before promotion is reported as qualified.

Measure ordinary release builds of the baseline and candidate through the
same normal compiled source and existing IdApi. Use seven fresh-process pairs
with alternating order, the existing host preflight and interval gates, and
identical warmup/work. Include no-slot/ordinary-send controls, canonical slot
access and invalidated fallback. Keep instrumentation out of timed builds.
Claim a benefit only when the median improvement exceeds both variants'
observed sample ranges; a regression beyond that spread prevents default
promotion. Preserve all samples and unsuccessful host-gated attempts. Do not
change eligibility or comparison rules in response to the timing results.

## Implementation and correctness results

`Runtime::send_abi` now tries the canonical Machine access after validating
the argument buffer and entering its dispatch-read lease. The new
`runtime/guarded_dispatch.rs` module checks the selector and delegates to the
existing guarded boxed-index access. Every miss continues through the original
resolver. Wrong arity returns nil directly only after canonical eligibility
has been proved; an override still receives the original arguments.

The guard checks resumable registration independently of dictionary mutation.
A canonical function address registered as a resumable carrier is ineligible,
including an unpublished or retiring registration. The dictionary cache also
rechecks its conditions at a saturated mutation epoch. Neither check changes
the registration carrier or any execution-state layout visible to native code.

Three focused tests pass. The first compares ordinary and optimized dispatch
for nine live-Frame cases: canonical access, invalid operands, read and write
overrides, removed/inherited read, private receiver vtable, custom lookup,
epoch saturation, and GC mutation. It records guard hits separately from
answers and checks exact Frame identity and lookup/override effects. The other
tests cover registration without dictionary mutation, including invocation
through a real registered resumable replacement at a canonical function address.

The complete `birnam-id` suite passes (128 passed, seven ignored). The 74F,
74G and 74H correctness runners pass in debug and release; 74I's twelve ordinary
native-client processes also pass. These reruns preserve the original recovery,
precise-GC, registration and native-unload checks. Full `mise run verify`
passes, including workspace tests, normal native prefixes, two independent
fixture-free cold archives and relocated runtimes.
The [verification receipt](../../measurements/74j-verification.json) records the
tested source hashes, command summaries and four retained regression receipts.

The E2 matrix has identical baseline/candidate output in all ten rows. This is
parity evidence: its two pre-existing abort cases still abort, and its
activate/rollback fixture does not report successful activation in either
prefix. It is not a new claim of successful activation from that fixture.

The older correctness runners now hash `execution.rs` as implementation input
instead of requiring its entire contents to remain frozen. Its only 74J change
is a private, non-mutating registration-presence query. The actual ABI, image,
descriptor and registration compatibility authorities remain byte-identical.

## Measurement execution

The runner is `scripts/internal-guarded-sends.py`. It compares a normal release
prefix built from `5e242e70` with a normal release prefix built from this change.
Both use `cargo build --workspace --locked --offline --release`. The generated
fixture imports the existing `birnam_send`; the floor export inventories agree
exactly. Canonical and native forwarding-lookup fallback modes run at 16,384
and 65,536 iterations, with an ordinary-send control at 100,000 iterations.

The [first attempt](../../measurements/74j-internal-guarded-sends-rejected-1.json)
was rejected by the existing host interval gate during pair three: median idle
CPU was 11.44%, with 193 of 240 samples below 30%. All completed program checks
passed, but the attempt supplies no qualified performance comparison. The same
seven-pair protocol was repeated without changing thresholds or code. The
[second attempt](../../measurements/74j-internal-guarded-sends-rejected-2.json) was
also rejected: pair two had median idle CPU of 18.39%, with 183 of 238 samples
below 30%. Timing qualification remains pending a quiet host; no performance
benefit or default-promotion acceptance is claimed from either attempt.
