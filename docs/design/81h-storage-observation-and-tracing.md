# 81H — Storage observation and a separate tracing module

> **Status:** Executed Rust experiment, 2026-09-18, macOS arm64.
> Builds on checkpoint `b7461cc1` (81F/81G). A separately loaded tracer computes
> reachability over the existing seed allocation domain. No reclamation occurs.
> The original native SeedApi carrier is byte-for-byte unchanged; Id gains a
> message protocol for observing storage and a scoped observation phase.

## 1. Result and boundaries

**Reachability can be computed above Id without sharing its private Rust heap
state.** Id reports storage facts it owns. Upper modules describe their own
payloads and roots. The tracer owns descriptions, work queues, visited sets and
the reachability calculation. It neither allocates another object universe nor
reconstructs Id's dictionaries from private layouts.

| Component | State and responsibility |
| --- | --- |
| Seed | Allocations, logical payload extents, authoritative core roots/edges, observation exclusion |
| Integer | Initialized-instance provenance, raw payload description, module-held roots |
| Object-model | Private-behavior reuse bookkeeping and module-held roots; the reuse map does not root its owners |
| Pair fixture | Two declared reference slots per Pair, representation identities, fixture roots |
| Tracer dylib | Per-run graph, description validation, visited set, work queue and result |
| Host probe | Explicit application roots, provider selection, native method/code lifetime and report capture |

The tracer is loaded **after** the seed, Integer, object-model library and test
graph exist. Reports refer to those original identities. There is no copying or
relocation of Birnam objects. Copied native fact arrays/work buffers are temporary
and expire at the observation boundary.

The sources are [seed](../../experiments/id-seed/seed/src/lib.rs),
[observation carriers](../../experiments/id-seed/observation-abi/src/lib.rs),
[tracer](../../experiments/id-seed/trace/src/lib.rs) and
[independent probe](../../experiments/id-seed/trace-probe/src/main.rs).
The new shared crate contains C-layout records, status constants and carrier
helpers, with no heap, tracing or object implementation. It is an interface
definition, not a semantic runtime beneath Id. Implementation crates depend only
on the two experimental interface crates. The original semantic probe still
depends only on the original ABI crate.

## 2. Id's storage-observation contract

The Vtable root receives an ordinary, replaceable **`withStorageView:`** method.
Its single operand is a native Method visitor. No SeedApi field or native seed
export is added; the selector is obtained through normal interning.

The visitor receives a callback-scoped `StorageView` using the existing native
Bytes operand carrier. The view contains immutable arrays of:

- Allocation identity, exact requested payload extent, and whether the seed or
  an upper owner is responsible for describing the payload.
- Core roots: the four existing root handles and all interned Symbols. This
  experiment retains Symbols strongly; it does not claim production weak
  interning behavior.
- Core strong edges: every object's attached behavior, default-behavior parents,
  and selectors stored in default method dictionaries.

Allocation enumeration does **not** root every allocation. Native method entry
and data pointers are not Oops and are not guessed to be references. Method-data
owners must report their object references explicitly. Symbol byte buffers and
default dictionary storage remain seed-owned native storage, not independent
Oop allocations in the report.

The seed adds one stored logical byte count per allocation and an observation
flag. It does not add object-layout registries for Integer/Pair, mark bits, queues,
reachability policy, reclamation or a collector handle.

## 3. Observation phase and native invocation exceptions

Observation begins through a top-level send at an explicit host safepoint. The
default method requires send depth one; invocation from inside another method is
rejected. All heap/state activity is on the seed's owner thread.

While the phase is active:

- Ordinary sends and behavior reassociation return `BUSY` (`0x200`).
- Captured direct entries for allocation, interning, delegation, method
  installation/removal and parent mutation also return `BUSY`. Saving a method
  address cannot bypass the seed's mutation guard.
- Recursive observation is rejected. Seed destruction is rejected while the
  enclosing call remains active. Behavior reads remain available.
- Providers and report callbacks obey a trusted read-only contract for object
  payloads and module state. They may allocate native scratch buffers, which do
  not enter the observed Id heap.

The seed copies its facts and releases all registry borrows before calling the
visitor. Scoped cleanup restores ordinary operation when visitors/providers or
report callbacks return errors. Native code must not unwind across this ABI.

Raw writes by arbitrary machine code are outside enforcement, as in the other
experiments. This is a single-threaded observation boundary, not concurrent GC,
OS suspension, native stack capture or a production safepoint implementation.

Two native invocation exceptions are explicit:

1. **Prepared provider entries.** The host resolves provider handles through
   ordinary messages before the phase, then passes their pinned Method pairs.
   During observation they are invoked directly so lookup cannot allocate,
   mutate or recursively collect. Replacing provider selection remains a matter
   for the preceding ordinary execution phase.
2. **The tracer's host entry.** `birnam_trace_v1(SeedApi*, TraceRequest*)` starts
   at the host boundary and sends `withStorageView:`. It adds nothing to SeedApi.
   The experiment does not make a tracing send from an arbitrary active method
   into a safepoint. Integration with Machine/native root discipline is later
   work; this native entry is not a permanent language-interface decision.

The storage method itself remains replaceable: the probe wraps it and verifies
the wrapper is invoked for tracing. The default provider lifetime continues to
pin code/data through seed shutdown, rather than providing dynamic retirement.

## 4. Upper payload descriptions and root ownership

`ProviderSink` accepts roots and complete payload descriptions. A description
names an allocation, its exact logical extent, and aligned byte offsets holding
strong Oop references. Zero in such a slot means absence. Unlisted bytes are raw.

The tracer validates allocation membership, extent equality, unique ownership,
offset alignment/bounds/uniqueness, and reference-target membership before using
the graph. Every nonempty upper-owned allocation needs exactly one description,
including allocations that may turn out to be unreachable. A missing description
returns `INCOMPLETE` (`0x201`); malformed/conflicting descriptions return `INVALID`.
No report is published on those failures. A provider cannot hide a rejected
description by ignoring the sink's failure status. Empty payloads need no slots;
seed-owned payload references are already supplied by Id.

Integer publishes `storageProvider` on its factory. It roots the factory,
Integer behavior and cached constructor selector. Its provenance set supplies
descriptions of eight raw bytes with no reference slots; membership in that set
does not make an instance live. The object-model library publishes
`objectModelStorageProvider` on Vtable, roots its cached selector, and treats its
private-owner reuse map as weak bookkeeping for this report. Active private
behavior is reached through Id's actual object-header edge.

The Pair fixture owns two Oop slots in each of its allocations and roots its
behavior so its construction service remains usable. Descriptions attach to
the representation's owning identities, independently of their current behavior.
The probe reassigns a Pair to Integer behavior and still follows its two Pair
slots. Conversely, an Integer containing the bits of a dead Pair's address does
not retain that Pair.

These descriptions cover the current experiment's Id allocations. They do not
yet describe native external buffers, executable ownership, weak-reference or
ephemeron processing, image relocation, or arbitrary restored objects. Provider
and application-root completeness remains the caller's contract; a reachability
report is relative to those declared roots, not every pointer physically held on
the native stack.

## 5. Executed evidence

Run the expanded isolated suite:

```sh
mise run id-seed-experiment
```

The [driver](../../scripts/id-seed-experiment.py) retains fresh offline source
snapshots, Rustfmt/Clippy checks, original ABI hashing, dependency/export audits,
and exact mapped-image checks. The trace process maps only its probe, seed,
Integer, object-model and tracer as non-system images. The tracer exports only
`birnam_trace_v1` and has no linked production Birnam implementation.

| New case | Observation |
| --- | --- |
| Existing storage facts | Original allocation identities/extents and core behavior/parent/binding edges appear, with explicit core roots |
| Shared references, cycles and raw words | Exact live/dead partition includes live cycles and shared children, excludes a dead cycle and address-shaped integer payloads |
| Changing root scopes | Adding/removing application/provider roots changes the next report, without retaining a prior run's marks |
| Behavior reassociation | Pair references survive reassociation with Integer behavior; representation authority stays with the Pair provider |
| Missing/invalid descriptions | Missing providers, duplicate claims, bad extents/offsets/targets and ignored sink errors fail without a success report |
| Phase guards | Ordinary sends, direct captured mutators, reassociation, nested observation and destruction cannot mutate/end the observed seed |
| Error cleanup and nesting | Provider/report failures restore the phase; a tracing attempt inside another send is rejected and later sends still work |
| Replaceable storage method | Wrapping `withStorageView:` remains observable |
| Ordered shutdown | Seed retires before module state and loaded code |

**All nine tracing cases pass in debug and release**, alongside the existing
19 full semantic cases and 11 Integer-only cases in each profile. Formatting,
Clippy with warnings denied, source-carrier hash and dependency audits pass.

Five mutation controls are detected: the three original 81F/81G controls plus
two tracer variants that root every allocation or omit all declared payload
edges. Each new variant passes the storage-facts check and fails the exact
reachability partition. These variants are confined to disposable snapshots.

The successful local receipt is
`.cargo-target/id-seed-experiment/20260918-110536-uz0vwasy/receipt.json`.
It records checkpoint `b7461cc1` plus exact source/binary hashes. That checkpoint
contains 81F/81G; this proof's new sources are identified by the receipt hashes.
Production exhaustive/release gates were not run for this separate opt-in
workspace; no production runtime, installer or ABI source changed.

## 6. What this settles and what follows

This supplies a bounded part of I3 (identities/extents/core roots/edges) and I4
(explicit observation phase and providers), plus a tracing subset of L4/L5.
It does not supply I3 reclaim/detach/index retirement, production allocation
safepoints, automatic native roots, ownership acquisition or a complete collector.
Storage and Symbols remain retained until shutdown, and no performance claim or
Birnam implementation claim follows from the Rust prototype.

The next separate proof is reclamation: establish how a selected unreachable
allocation is retired, remove seed and upper-module indexes before address
reuse, protect active roots/code, and demonstrate that a later allocation cannot
inherit stale Integer/private-behavior metadata. Reachability alone does not
authorize freeing objects with native resources or unretired method bindings.
