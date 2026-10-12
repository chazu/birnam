# Execution selector access — 2026-10-11

The [application profile](application-cpu-profile.md) exposed heavy namespace and
admission work inside JSON string decoding. This investigation retained a small
optimization to retrieving Execution's own selector Symbols. It did not remove
namespace reads, cache their results or weaken admission checks.

## Retained change

Execution already interns and retains its fixed names at construction. Its
[`Names` inventory](../../rust/birnam-execution/src/names.rs) now gives direct
access to eight frequent names: `at:`, `Integer`, `behaviorEpoch`,
`nativeBindingsMatch:`, `usesDefaultAllocation`, `usesDefaultLookup:`, `bind:to:`
and `canOmitIndexing`. Other names use the existing HashMap. A byte comparison
selects a retained Symbol, avoiding repeated randomized hashing on those accesses.

The original map remains immutable and authoritative. Root enumeration retains
its existing membership and iteration order. The extra array is 64 bytes per
Execution instance on this 64-bit host, not per String, object or method. No
additional heap allocation is needed for the array.

Only selector identities are duplicated. Every namespace read and target send
still consults the current world. Permission queries, post-callback fences,
boxing, object identity and recovery paths remain intact. There is no speculative
method/value assumption to invalidate; existing deoptimization continues through
the same guards. This is private owner storage, with no new native protocol,
public ABI, JSON special case or new dependency on Rust for eventual self-hosting.

## Rejected and deferred approaches

An experiment used the existing `BindingGuard` protocol to batch Integer's
lookup and two conversion-binding checks. It retained current allocation policy,
the final epoch fence and ordered fallback. Native replacement checks and the
formal suite passed. But measured CPU did not improve convincingly: full-workload
median changed by -0.3%, while unescaping changed by +0.4%. The batch implementation
was removed; fewer sends alone did not justify keeping it. Its patch and initial
evidence are recorded for reference.

Caching the Integer namespace value was deferred. The current namespace lookup
can invoke replaceable `symbolBytesOf:` behavior; namespace rebinding also need
not change the behavior epoch. Simply sharing or hoisting those reads would
therefore require stronger owner-certified conditions than epoch equality.
The existing frame-call tests explicitly exercise rebinding during admission.

Further Integer/Word box elimination remains compiler-region work. The current
[conversion observability tests](integer-word-conversion-investigation.md),
rerun here, demonstrate that replaced constructors can retain temporary objects
and replaced getters can change the result. The existing guarded frame-call
optimization already omits eligible index construction. Removing the remaining
conversions locally would not establish the required identity/recovery contract.

## Measurements

The final comparison uses baseline `52965d84` and a prefix differing only in
`libbirnam_execution.dylib`. Both use the same compiled profiling project and
50-record fixture. Each phase runs in ABBA order, with two processes per variant,
one warm-up and three timed iterations per process. These are medians of the six
thread-CPU samples per variant; startup, input I/O and preparation are excluded.

| Workload | Baseline | Retained change | Reduction |
|---|---:|---:|---:|
| Full parse/aggregate/render | 2,019 ms | 1,972 ms | 2.3% |
| Unescape prepared string bodies | 1,653 ms | 1,622 ms | 1.9% |

An earlier isolated comparison on top of the batching experiment measured 2.7%
and 3.2% reductions respectively. The final standalone comparison above is the
delivery measurement. Individual process timings vary, including one full-workload
pair favoring the baseline. Treat this as a modest local improvement, not a
guaranteed speedup across programs or machines, or a solution to the decoder's
remaining allocation and dispatch costs.

Every compared phase result and complete report matched the independent Python
oracle. Measurements used ordinary GC with the fixed 16 MiB default. Timing
prefixes contain neither dispatch instrumentation nor TCC resource counters.

## Validation

The inventory unit test checks every Execution name using a separately allocated
byte key, unknown-name fallback and unchanged root enumeration order. The added
Integer admission regression covers exact contexts, replaced query services,
original errors, stateful permission changes at a fixed epoch, mutation during
permission evaluation and restored conversion behavior. It also passes against
the unmodified Integer owner; the batch experiment is not required by the test.

Existing compiler checks cover namespace/factory replacement, collecting guards,
fallback after a positive grant, observable frame slots and target replacement.
The saved-image check exercises source-free mapped and rebuilt images, protocol
withdrawal and fresh target lookup. Application acceptance includes valid and
invalid fixtures, project tests and the scripted edit–rebuild–run cycle.

Lean's retained-name/current-dispatch and root-edge lemmas apply to this storage
change; admission, recovery and stale-name TLA+ cases remain relevant regression
checks. These verify abstract contracts, not the Rust implementation itself.
Exact results, tool/source/runtime hashes, comparison samples, the rejected patch
and reproducible driver sources are in the
[measurement record](../measurements/execution-name-access-20261011.json).

Final validation passed 23 Execution unit tests, six native test binaries,
source-free mapped/rebuilt image checks, application acceptance and the edit cycle,
172 Lean theorem checks, 32 rejected Lean mutations and all 118 TLA+ configurations
(including required counterexamples). The compiler fixture ran in a separate
prefix with TCC's required resource counters enabled.

The untimed `duration_ms` census still contains exactly 91,509 sends. Every
selector count and the complete selector/Symbol-key event sequence match the
baseline. This supports the intended change: cheaper local name access with
unchanged message traffic, rather than suppression of observable operations.

No user installation or release was made. Relational migration remains on hold.
