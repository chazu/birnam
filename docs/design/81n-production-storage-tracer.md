# 81N — Tracing described production storage

> **Status:** Implemented; focused checks recorded below. Follows storage
> observation checkpoint `e55c604d`. This promotes the 81H graph tracer into a
> normally loaded production module. The automatic collector's existing mark
> path remains in Id.

## Boundary choice

Three interface designs were compared using the design-an-interface workflow.

The scoped design exposes `trace_described(view, request) -> status`. A caller
composes observation, description validation and tracing through synchronous
visitors. Temporary indexing, graph storage, visited bits, traversal and the
result partition are hidden in one call. The existing callback scopes supply
the lifetime, with no additional handle or teardown protocol.

The streaming design exposes `trace_live(source)` with source callbacks for
ordered roots, first-mark admission and reached-object edges. It could replace
the existing collector's worklist, but must preserve LIFO order, immediate mark
admission and then-current layout/hook lookup for each reached object. Legacy
providers and hooks may call back into Id; eagerly invoking them for every
allocation or placing them under 81M's observation exclusion changes behavior.
That design requires a separate live-GC boundary and floor adapters.

The persistent design exposes `create`, `configure_roots`, `run` and `destroy`.
It could reuse native buffer capacities, but adds session handles, runtime
binding, stale-root rejection and error-state cleanup. Each run would still need
fresh facts: allocation identity does not prove that a payload, behavior or root
set remained unchanged. Retained results would be historical observations.

The scoped design is selected for this next seed promotion. It consumes the
81M boundary directly without introducing another lifetime protocol or changing
current collection semantics. The production migration is a loaded service over
real storage, not a claim that automatic GC now uses it.

## Implemented responsibility

`birnam-trace` depends only on the no-state `birnam-abi` crate. The host and
installer inventory load it after `birnam-storage`, before Integer. It exports
one graph function plus the usual stateless module initializer and Symbol-borrow
declaration. There is no implementation link back to Id or storage and no hidden
heap, cached object registry or retained IdApi.

The tracer validates graph membership and builds an address-to-index map and
adjacency lists. It traverses combined core/provider roots through core and
declared payload edges. Every physical allocation is reported exactly once as
`reached` or `not_reached`, ordered by allocation identity and qualified by the
runtime and observation identities. Reassociation cannot replace a provider's
payload description; raw pointer-shaped Integer bits do not become edges.
Default-vtable owner/parent/selector edges come from Id's authoritative facts.

The request is strict by default. Diagnostic callers explicitly accept named
coverage gaps, and those omissions remain in the report. Native roots, uncalled
legacy callbacks, incomplete payload coverage and deferred identities are not
silently filled in. Enumeration and permanence do not make every allocation a
root. A permanent allocation can be not reached in this declared graph and still
be retained by collection policy. No report authorizes reclamation.

The [native contract](../../abi/trace-v1.md) specifies scope, identities, status
propagation and completeness. Id's replaceable `withStorageView:` lookup remains
the entry to observation. Provider selection happens before exclusion; the
tracer does not invoke messages or payload callbacks. GC marks, live accounting,
deferred journals, finalizers, native lifetimes, IdApi v4 and image bytes are
unchanged. There is no new Birnam-language implementation claim.

## Focused evidence

The Rust oracle tests all 512 directed three-vertex graphs with all eight root
sets, comparing the worklist result with independent transitive closure. They
also cover duplicate roots/edges, empty graphs, malformed carriers, foreign
endpoints, identity order, permanent facts, deferred accounting, coverage bits,
negative callback errors and nested independent calculations.

The independent C fixture loads the actual Id, storage, Integer and tracer
dylibs. The tracer is loaded after the test objects exist. A second graph oracle
uses repeated edge scans and fixture-owned slots to verify the complete reported
partition over the observed production heap. Cases include shared references,
rooted/unrooted cycles, changing root scopes and payloads, a Pair reassociated to
Integer behavior, raw Integer address bits, strong default-vtable owner edges,
stable allocation identities, malformed graphs and strict gap rejection.
Replaceable observation lookup is exercised on every run. Nested observation
returns BUSY; provider/report failures restore normal Id access. Legacy callback
counters remain zero, finalizers do not run and allocation counts are unchanged.

Run `mise run storage-tracing`. No release or installation validation is needed.

Final focused results: four tracer tests (including 4,096 oracle comparisons),
four ABI tests, one storage-validator test and four Id storage tests pass. The
independent C fixture passes against the debug dylibs; ABI/storage/tracer Clippy
passes with warnings denied, and the host compiles. The tracer dylib exports
exactly its three declared symbols, uses its declared `@rpath` install name and
links only the system library. Its sole normal Cargo dependency is `birnam-abi`.

## Remaining migration

Automatic collection still uses `gc::Marker` and `Runtime::run_mark_phase`.
Moving it requires production representation providers and native/Frame/code
root coverage, or the separately reviewed live streaming adapter above. The 81I
collector promotion also needs transactional retirement of weak and upper
metadata before detach/reuse, including native finalizer and code obligations.
The current report service is the graph-calculation prerequisite; these remaining
obligations cannot be replaced by accepting diagnostic gaps.
