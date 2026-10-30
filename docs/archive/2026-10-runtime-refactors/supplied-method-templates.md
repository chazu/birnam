# Supplied method templates

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-15. Extends the [authored construction owner](method-invocation-owner.md)
to precompiled artifact activation and image reconstruction. The prior receipt
remains evidence for the source-only slice, not this integration.

## Construction contract

`Reflect methodForTemplate:implementation:` is an ordinary construction message.
The cold seed sends `withImplementation:` to the supplied template. After the
existing explicit bootstrap cutover, the authored Reflect method resolves current
`MethodInvocation` and sends the same message there. Its default implementation
sends the template's current `withImplementation:`. Source arity-based construction
also uses this hook after choosing its template.

Each binding gets its own Method, implementation, registration and lifetime. The
selected template owns code, not another owner's service Method. Future invocation
continues to send to the current implementation protocol. Construction does not
publish a method dictionary entry or bypass Generation admission. Existing entered
calls, recovery and executable-owner retention remain the constructor's contracts.
There are no method-identity guards or automatic fallback when policy is replaced.

## Artifacts

The native-object capability is now `id-object-v3`. Adapter rows contain arity,
object bytes and an explicit synchronous Boolean. Both the native project planner
and authored module planner reject older contracts before activation. Existing
artifacts must be rebuilt; no backwards compatibility is intended.

A resumable adapter is loaded lazily on its first use in an activation and
initialized once with nil. Later definitions bind the same selected template through
the current Reflect receiver's construction hook. The source arity cache cannot
substitute its code. An explicitly synchronous adapter continues to load a separate
Unit per definition and initialize it with that definition's implementation; its
Unit-local mutable state and native failure behavior are unchanged.

The activation retains adapter bytes and any selected templates through the normal
GC visitor. Nested activation scopes have independent state. A loader or initializer
may reenter construction: the already chosen outer template serves that call, a
nested cache winner remains cached, and no callback is replayed. This is temporary
activation ownership, not a permanent Rust publication service or a global cache.
When activation ends, bindings retain their code through Execution ownership.

## Images

Image plans and inventories keep their existing format and per-binding backend
requests. Mapping/rebuilding deduplicates only named resumable adapters with the
same arity and **identical selected payload bytes**. The payload is relocatable object
bytes when mapped and emitted C when rebuilt. Different payloads at the same arity
remain separate. Body code is not deduplicated.

A shared adapter Unit initializes with nil during staging. Preparation then sends
`Reflect methodForTemplate:implementation:` for each logical binding. The existing
prepare/finish/install/cancel boundary remains intact. Errors stop preparation;
publication still happens only through the existing image installation protocol.
Staged references protect templates through collecting callbacks. The existing image
operation reentry exclusion remains in force.

This intentionally reduces loader and initializer calls for identical selected
adapters. Custom construction policy belongs at the per-binding hook; neither a
particular mapping count nor one Unit initializer per definition is promised.
Backend emission remains live and in its existing order; image saving still emits
and stores the existing per-binding objects. This slice reduces resident mappings,
not the serialized bundle's duplicated object records or backend emission work.

The pre-existing synchronous failure/image mismatch remains separate work: image
plans already reconstruct resumable named adapters. This change neither fixes nor
reclassifies that mismatch. Arbitrary custom native forwarding programs and parked
frames still lack a portable image recipe.

## Qualification

The native `artifact_templates` regression probe covers supplied code while source
emission is disabled, sixteen independent Method identities and receiver/arguments,
current construction policy and Error identity, and collecting reentrant loading.
It also checks both image modes with identical and deliberately distinct payloads
at the same arity. All backend requests still occur; only identical code shares.
Cancelling staging releases the selected mappings. The same probe runs against the
previous source-only runtime as a baseline.

The artifact workload retains sixteen unary definitions. Resumable activation uses
2 TCC units / 288 KiB including the program body, versus 17 units / 768 KiB before.
Forwarding code alone falls from 16 units / 512 KiB to 1 unit / 32 KiB. The explicitly
synchronous workload remains at 17 units / 672 KiB. These are TCC-owned allocation
counters, not process RSS or the language heap. No invocation speedup is claimed.

The full language suite passes 826 assertions and its scratch, Reflect, inspector,
Cells, slots, module registry, LSP and Globals postchecks. Fresh processes load
source-free mapped and rebuilt images. Installed host and artifact checks cover
receipt integrity, relocation, source startup, dependencies and inert preflight.
Sixteen native integration binaries pass with precise GC, including compiler,
Generation, publication, suspension/recovery, materialization and image ownership.
Twenty successive image resaves keep a fixed live set. The codegen and project-build
unit suites pass 82 tests (one pre-existing integration test remains ignored there).

Lean models exact payload admission and exclusion of synchronous state sharing.
TLA+ checks selected code, nested activation ownership, no replay and entered-code
retention, with deliberately incorrect variants required to fail. The complete gate
checks 296 Lean theorems, rejects 74 mutations and gets all 176 expected TLC outcomes. These are abstract
contract checks, not a proof of the Rust, generated C or complete runtime.


## Performance and remaining costs

Seven alternating fresh-process samples compare with the preceding source-only
factory build. Host samples use unmodified production prefixes; native probes use
separate prefixes with TCC allocation counters enabled. All qualification workloads
had finished before timing. Filesystem caches are warm.

| Median | Source-only factory | Supplied templates |
| --- | ---: | ---: |
| Load artifact with 16 resumable unary definitions | 7.118 ms | 3.370 ms |
| Additional TCC units, including program body | 17 | 2 |
| Additional TCC-owned native allocation | 768 KiB | 288 KiB |
| Installed host: startup image, compile/print 42, exit | 4.465 s | 4.130 s |
| Host peak RSS | 1,339.2 MiB | 1,336.0 MiB |

The artifact workload is **2.11 times faster**, and the complete host workload is
about **7.5% faster**. Peak RSS is essentially unchanged; reduced retained code is
not evidence of a corresponding peak process-memory reduction. The synchronous
control measured 7.090 ms versus 6.494 ms, with identical unit/allocation counts;
its construction path was not optimized, so no synchronous speedup is claimed.

The [receipt](../../measurements/supplied-method-templates-20261015.json) contains source
hashes, prefix provenance, raw samples and qualification output. Reproduce timing
with `scripts/supplied-template-bench.py`; it accepts the host prefixes, corresponding
metrics-module directories and the compiled `artifact_templates` probe executable.
The installed user runtime has not been changed.

Remaining mechanical opportunities are deduplicating identical records in the
serialized image bundle and shortening temporary buffer lifetimes during image
loading. Neither is implemented here. Any bundle change must preserve selected
payload identity and per-binding ownership rather than substituting by arity.
The separate uniform-error-result design remains the prerequisite for removing
explicitly synchronous adapter construction. Store publication migration can then
resume without inheriting that unresolved distinction.
