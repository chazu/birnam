# Why the Oop-only native slice is slower

2026-10-23, runtime revision `4c8e5937`. The regression reproduces. Its principal
causes are extra metadata/liveness work per send and construction of temporary
Names, not the cost of passing an Oop. GC additionally processes more objects and
more retirement metadata. No runtime change is retained by this investigation.

[Evidence and replay sources](../measurements/oop-id-slowdown-diagnosis-20261023.json)
contain quiet comparisons, diagnostic patches, counters, phase timers and native
sample summaries. Raw samples are in `/tmp/oop-slowdown-20261023`.

## Reproduced symptom and experimental boundary

The same native C workload, four fresh processes per variant in repeated ABBA
order, reproduces approximately +34% allocation/rooting, +52% retained-name
interning, +87% fresh-name production and +46% GC. These are thread CPU timings.
The candidate worktree was clean at the start; its runtime and installed baseline
library hashes remain unchanged afterward.

Instrumentation and bypasses were built in separate temporary worktrees/prefixes.
Removing roots or validation is an explanatory control, not an accepted fix.
Likewise, mutating an immutable Name's private bytes is only a lower-bound probe.
No diagnostic library was installed or promoted into the candidate.

This benchmark calls Id directly from optimized C. It does **not** exercise the
many TinyCC-generated helper layers that produced the original Logue packet-copy
profile. It therefore charges the new runtime machinery while capturing little
of the potential benefit from eliminating those generated-code copies. Its
regression is real, but is not a prediction of a completely migrated Logue.

## 1. The slice added work around every send

A counted run of 300,000 retained-name interning calls reports:

| Operation | Per call |
| --- | ---: |
| Id sends / binding operations | 1 |
| Behavior queries | 3 |
| Allocation-record lookups | 4 |
| Method-record reads | 1 |
| Name-owner/shape validation | 1 |
| Invocation-root references written | 5 |
| Id allocations | 0 |

The three behavior queries inspect the receiver, its behavior and the successful
result. They consult the allocation registry. Name validation additionally checks
the Name-owner registry, allocation extent and stored length. Method invocation
fetches code, context and owner from another map. Ordinary dictionary lookup and
canonical name hashing occur as well.

Several of these are new compared with v1. The old binder reads valid receiver
headers directly; the new path repeatedly queries allocation records. Old method
dictionary entries contain the native implementation; the new dictionary points
to a Method object whose **payload is empty**, with implementation state held in
`State.methods`. The v1 caller contract also leaves invocation protection to its
caller; the new floor does additional protection itself. These are implementation
and responsibility changes, not requirements of an Oop argument/result ABI.

Native sampling corroborates this. In the retained-name loop, v1's largest leaf
is name hashing (1,374 of 2,824 samples). V2's largest leaf is `State::invoke`
(940 of 3,446), followed by hashing, `State::behavior` and `State::name_data`.
Inlining means a function leaf is not an exact cost attribution to one statement.

Quiet ablations of the v2 runtime give these approximate retained-name savings:

| Diagnostic control | Saving per call |
| --- | ---: |
| Bind using valid-object headers instead of allocation-registry queries | 4.4 ns |
| Omit successful-return registry query | 2.1 ns |
| Omit invocation-root bookkeeping | 2.6 ns |
| Read known-valid Name storage without owner/extent checks | 4.3 ns |
| All four controls together | 17.8 ns |

The combined control runs at about **36 ns**, versus approximately **54 ns** for
its paired v2 baseline and **34 ns** for v1 in the reproduction. This accounts for
most of the retained-name regression. Individual savings are not additive: code
generation, interactions and run variation affect the combinations.

These controls do not establish that the checks/roots can be removed safely.
They identify where to remove duplicated work or improve representation while
preserving the checks' purpose.

## 2. Fresh names do substantially more work

For 8,192 new 64-byte names, the candidate reports:

- 16,384 Id sends: one Name construction and one interning call per input.
- 16,384 Id allocations: a temporary Name and a canonical Symbol per input.
- 524,288 byte-valued Oop arguments: 64 for every name.
- 606,208 root-reference writes, including Method/context/owner protection.

The caller expands 64 bytes into 512 bytes of Oop arguments. The invocation-root
writes add 592 bytes per name, before counting the owned name bytes, canonical
Symbol storage and allocation metadata. Immediates do not require tracing as heap
objects, but this root representation still copies them into the invocation list.

A diagnostic control reuses one Name and updates its bytes privately between
requests. It is **not** a legal implementation of the immutable Name contract;
it isolates production of the input object from canonicalization:

| Fresh-name workload | Median |
| --- | ---: |
| v1, raw bytes | 159 ns/name |
| v2, construct then intern | 320 ns/name |
| v2, diagnostic reused input | 183 ns/name |
| Reused input plus the four send-overhead controls | 167 ns/name |

Most of this regression is the extra representation/construction path. This is
why optimizing interning's hash function alone would miss the main issue. The
production design needs real name objects at the producer boundary, or an
object-only bulk construction path that avoids unnecessary intermediate objects;
it must still honor replacement and lifetime semantics.

## 3. GC pays for both the extra objects and extra machinery

The original workload leaves v1 with 45,693 dead objects, and v2 with 53,886.
The 8,192 temporary Names explain nearly all that difference.

| Quiet GC control | Reclaimed objects | Time |
| --- | ---: | ---: |
| v1 | 45,693 | 9.75 ms |
| v1 plus 8,192 ordinary garbage objects | 53,885 | 11.51 ms |
| v2 | 53,886 | 14.75 ms |
| v2 with diagnostic reused Name | 45,695 | 12.62 ms |
| Reused Name plus send-overhead controls | 45,695 | 10.87 ms |

The added ordinary objects are not identical to Names, so this is a population
control rather than an exact decomposition. Both directions nevertheless show
that roughly 1.8–2.1 ms of the observed gap follows the additional garbage.

Separate phase-only timers locate the other costs:

| Phase | v1 | v2 |
| --- | ---: | ---: |
| Census delivery and sorting | 1.58 ms | 3.66 ms |
| Complete marking, including census | 2.85 ms | 5.24 ms |
| Preparation/index retirement | 0.29 ms | 1.61 ms |
| Complete sweep, including preparation | 7.27 ms | 9.70 ms |

The rows overlap and must not be added. These instrumented phase runs are separate
from quiet comparisons. After accounting for population, the per-object release
loop is not the largest unexplained problem; census delivery and preparation are
more revealing targets.

Counters show 231,757 Id sends during v2 collection, 697,972 behavior queries,
574,005 allocation-record reads, and 1,560,302 invocation-root-reference writes.
The same small per-send costs therefore accumulate during collection.

Two concrete housekeeping regressions were introduced in the slice:

1. Id now performs a stable sort of allocation records, then the collector sorts
   again. V1 Id emitted its snapshot without sorting; the collector supplied
   creation ordering. Removing the new Id sort saves about 2% in this workload.
2. For each dead object, Id attempts deletion from four separate metadata maps.
   That is 215,544 probes, including many guaranteed misses for ordinary objects.
   A diagnostic implementation chooses between removing dead keys and scanning
   the smaller metadata map, using capacity to avoid scanning a huge sparse table.
   It saves about 4%. Merely moving the RefCell borrows outside the loop produces
   no clear gain. Combining the sort/removal changes saves about 3% in these runs.

These are worthwhile simplifications, but are smaller than the overall dispatch
and population costs. They do not by themselves explain or fix a 46% regression.

## Consequences for the design

The evidence supports keeping the Oop-only goal. It rejects the inference that
removing a packet type automatically reduces total runtime cost. In this slice,
packet removal was accompanied by new registries, validation and lifetime work.

The most useful follow-ups are:

- Put native Method state in its owning object, or otherwise eliminate the extra
  map fetch after a binding has already been admitted. Keep custom lookup,
  replacement, context validation and entered-code lifetime intact.
- Consolidate Name and allocation metadata checks. The Name registry already
  stores the constructor's length; the implementation currently revisits separate
  sources of the same shape facts. Preserve malformed-object and retirement checks.
- Represent invocation protection using explicit owned frame information and the
  caller's stable argument storage, rather than blindly copying every argument
  again. The C borrow lifetime must remain synchronous; logical continuations
  remain in the stackless runtime's owned frames. Do not simply disable rooting.
- Avoid producing disposable Names around already available data on every request.
  Measure the actual producer/interner boundary, including dynamic input and GC.
- Remove the redundant census sort and make metadata retirement proportional to
  the smaller relevant set, with creation-order/finalizer and sparse-batch tests.

No bypass control is retained. The candidate's runtime remains at `4c8e5937`;
this investigation adds diagnosis and evidence only. Full source-language,
stackless, image, worker and Logue qualification are still outstanding.
