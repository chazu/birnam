# Oop-only Id: bootstrap-to-GC qualification

2026-10-23. Isolated branch: `codex/oop-only-id-slice`, based on `a271e4c5`.
Worktree: `/Users/chazu/dev/zig/birnam-oop-id`.

**The requested native vertical slice works. The performance gate does not pass.**
It cold-starts Id, loads separately compiled Rust and C modules, and performs real
precise collection with Oop-only messages. It is not a completed language migration,
a full-workspace build, or a replacement for the installed runtime.

[Evidence](../measurements/oop-id-slice-20261023.json) records qualification logs,
source and library hashes, boundary audits, timing runs and limitations.

## Implemented boundary

The compiled slice replaces the v1 ABI in `birnam-abi`, `birnam-id`, Siphon, Roots
and the collector. There is no v1 compatibility dispatcher or Operand adapter in
that closure. It reuses Id's allocation records, Symbol canonicalization, ordered
dictionaries and metacircular dispatch, and ports the collector's inventory,
marking, ordered retirement, detachment and finalization sequence.

- Arguments are borrowed arrays of Oops; successful calls return one Oop. Native
  method objects have ordinary identity, behavior, context and owner references.
  Dictionaries contain Oops, with callable/opaque and weak-key bookkeeping kept
  distinct. Code pointers occur in native loader metadata and Id-owned Method
  implementation records, never as a kind of Id argument.
- Nonnegative size/count values use tagged immediates. Allocation limits are checked
  against the actual allocator. This does not retag existing language Integers or
  claim full-width Word/Float adaptation has been migrated.
- Immutable Name objects own their bytes. Native clients construct dynamic names
  by sending byte-valued Oops; retained names are interned directly. Loaded-file
  literals are created from descriptor data during loading. Ordinary calls have
  no C-string/byte-span argument exception.
- Native failures are messageable Fault objects, represented with a failure-reference
  tag. The tag remains part of allocation identity through retirement/detachment.
  Kind and detail are Oops. There is no success envelope. Ordinary object results
  remain data; the existing language Error module is not yet migrated or qualified.
- Id protects receiver, selector, arguments, selected Method, context and owner in
  explicit owned invocation-root storage. Nothing here moves language continuations
  onto the native stack. The old stackless execution engine itself is not yet ported.
- The initial graph is constructed directly. Siphon performs one native bootstrap
  handoff; subsequent module loads are Name messages through Id. Native descriptors
  contain immutable literals and code declarations. Initializers receive Oop values
  and the Id linkage protocol. Incompatible/null initializers are rejected before
  entry. Entered native libraries remain resident after partial initialization.

A small reference-only Sequence object currently supplies bootstrap literal tables,
loader/invocation argument objects and fixture state. It is Id-owned storage in this
slice, **not a migrated language Vector**. Its placement and replacement by the
appropriate ObjectModel/Execution protocols remain an architectural review item
before treating this as the final minimal kernel. No upper String/Integer policy
or foreign representation layout was added to Id.

## Acceptance evidence

`bash scripts/oop-id-slice-test.sh` passes in the normal target and a fresh source
build target. Runtime entry sets the installed-module search variables to nonexistent
paths and uses only explicit library paths. The exercised closure contains Id,
Roots, Collect, the independent C modules and a vendored-TinyCC caller. No source
compiler image, installed language prefix or hidden bootstrap fallback is used.

The production Id unit suite has 11 passing checks: retained Symbol/dictionary
oracles, binding epochs, lookup hints, weak/opaque shadowing, and Fault allocation
identity. The independent C declaration checks the ABI layout and is used by both
a separately loaded module and a standalone C caller. Rust is the primary cold
acceptance driver; TinyCC compiles another caller against that same independent
header.

Runtime checks pass for:

- Repeated owner-thread bootstrap, native loading, and Oop size/name construction.
  Oversized allocations, NUL/out-of-range bytes and wrong byte values fail without
  allocating; empty-name interning remains canonical.
- Collection from an entered native method, with receiver and argument protection;
  128 finalizers; harmless nested collection during finalization.
- Removing an entered Method's binding and collecting while its otherwise-unrooted
  context remains in use. Both Method and context survive.
- Intern-method replacement across collection and restoration using a captured
  Method; custom metacircular lookup and DNU with all 19 arguments forwarded.
- Tracing failure and allocating tracers aborting before retirement. Swallowed
  invalid visitor, core-edge and census emissions also stop collection. Retry works
  after correcting the tracer.
- Finalizer failure leaving detached storage unreleased and the collector in a
  failed state. This is checked in a separate process.
- Partial initializer publication: after failure and GC, an already published
  method remains callable. Wrong ABI and null initializers are rejected.
- Invalid contexts for core, Roots and GC methods producing Faults rather than
  interpreting arbitrary object payloads as native state. Cross-thread Id entry
  is rejected.
- A vendored-TinyCC caller creating/interning a name, retaining it and its Symbol,
  collecting, checking identity, and releasing its roots through actual Id.

The test work exposed and corrected missing sticky callback-failure handling,
unsafe context interpretation, and the need to retain a Fault's reference tag
independently of its retired metadata. The original Id owner-thread cache and
collector's lazy parent-walk scratch behavior were retained.

Follow-up: [causal slowdown diagnosis](oop-id-slowdown-diagnosis-20261023.md)
separates per-send metadata/rooting work, temporary Names and GC housekeeping.
It retains no diagnostic bypasses.

## Performance result

The final comparison uses the same C workload, four fresh processes per variant,
in ABBA order repeated twice. Timers use thread CPU. Libraries are hashed before
and after the runs. Automatic collection is disabled; compilation/loading is outside
these timers. No confidence interval or Logue speedup is claimed.

| Requested workload | Installed v1 | Oop-only slice | Change |
| --- | ---: | ---: | ---: |
| Allocate zero-payload objects; retain every eighth | 41.93 ns/object | 57.15 ns/object | +36% |
| Intern a retained 64-byte name | 33.81 ns/call | 51.70 ns/call | +53% |
| Produce and intern a new 64-byte name | 151.81 ns/name | 315.99 ns/name | +108% |
| Collect the resulting garbage | 10.13 ms | 15.14 ms | +49% |

These are full primitive-path observations, unlike the earlier isolated transport
and representation microprobes. The candidate performs invocation protection and
uses Method objects; its fresh-name case creates a Name from Oop bytes before
interning, while v1 consumes a raw byte span. Consequently it also leaves more
objects for GC. The evidence includes heap counts so the GC difference is not
misrepresented as a same-population collector comparison.

Two measured simplifications reduced the initial candidate costs: removing duplicate
pre-binding argument-behavior queries while retaining receiver/result and operation
shape checks, and sharing the Id object as core primitive context instead of creating
one context object/registry entry per primitive. The native caller's live-Oop borrow
contract remains explicit. Neither change made this candidate match the baseline.

The correct conclusion is **functional feasibility, with an unresolved performance
regression**. These results do not prove that raw-value exceptions are necessary,
and no exception has been adopted. They also do not justify calling the strict
boundary fast enough for the complete runtime yet. Profile the remaining dispatch,
root bookkeeping and dynamic-name construction costs before a wider cutover.

## Scope still outstanding

This branch deliberately targets a native closure. Other workspace crates still
refer to the removed ABI, so the complete workspace/language is not buildable here.
Unreferenced legacy implementation files and old upper-level tests remain migration
backlog; their presence is not a hidden adapter in the tested closure. The source
and export audit covers files reachable by the three built runtime libraries.

Not delivered or qualified by this slice: automatic collection/accounting policy,
all old Id introspection/observer entrypoints, incoming/outgoing language adapters,
source compilation, stackless frames/recovery/cancellation, full Error semantics,
images/artifact rebuilding, reclaimable JIT-generation lifetime, process workers,
or Logue. Native library residency is preserved, but it is not proof of reclamation
for code mappings. Existing boxed numeric identity contracts remain untouched in
the installed runtime.

Keep this checkpoint isolated. Do not merge or install it as a working-language
replacement, weaken the existing upper-level tests to fit it, or count the earlier
microbenchmarks as passing the full performance gate.

## Reproduce

```sh
bash scripts/oop-id-slice-test.sh
python3 scripts/oop-id-slice-bench.py --baseline "$HOME/.local/opt/birnam"
BIRNAM_OOP_TARGET="$PWD/.cargo-target/oop-slice-fresh" \
  bash scripts/oop-id-slice-test.sh
```

No install or push was performed. The original `birnam` worktree and installed
prefix remain the continuation and comparison baseline.
