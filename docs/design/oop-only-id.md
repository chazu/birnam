# Oop-only Id

Decision: accepted user direction, 2026-10-22. Highest-priority runtime work.
Status, 2026-10-23: the native bootstrap-to-GC slice is implemented and
committed as `4c8e5937` on `codex/oop-only-id-slice` in the separate worktree
`/Users/chazu/dev/zig/birnam-oop-id`. Its qualification report is
`docs/design/oop-only-id-slice-20261023.md` in that worktree. Cold loading,
independent C/Rust/TinyCC callers and precise-GC checks pass. Full-path timings
regress; the slice is not a merge/install candidate or a complete language
migration. This main worktree and the installed runtime remain pre-cutover.
The initial ownership question is revised by the Piumarta comparison below.

## Required end state

Every receiver, selector, argument and result crossing an ordinary Id message is
an Oop. `Operand` and `Outcome` are implementation mistakes to remove, not abstractions
to optimize, rename, hide behind pointers, or box into one universal variant object.
Actual values and methods have ordinary object identity and message behavior.

The semantic shape is:

```text
send(receiver: Oop, selector: Oop, arguments: sequence of Oop) -> Oop
```

User clarification: raw allocation sizes and C-string interning are acceptable
only if a performant strict boundary cannot be achieved. No such necessity has
been established. Do not treat these exceptions as authorized defaults or assume
Piumarta's variadic calling convention is required.

A native implementation can borrow an array of Oop references plus a count. That
physical argument-array convention is not an alternate category of message value.
Raw machine state remains permissible inside the implementation that owns it and
at actual foreign calls. It must not become a second inter-module service API.

Remove carrier adaptation and success-result envelopes. Do not retain the old ABI
as a production compatibility path. Rebuild native modules, generated code and
images explicitly; reject incompatible executable artifacts before entering them.
Historical fixtures can document a rejected format without loading it as production.

This decision supersedes the raw Word/Bytes/Entry/Protocol carrier allowance in
Designs 83/84/90 and the October ABI-simplification documents. It does not revoke
functional ownership, live message dispatch, or language semantics.

## Preserved obligations

- Current dispatch, inheritance, custom lookup, DNU, replacement, reflection,
  ordinary identity and aliasing. A selected active call retains its entered code;
  a later dependency send observes the current meaning.
- The distinction between an ordinary Error value, a failed operation, absence,
  language nil, and control transfer. Removing status packets must not silently
  turn propagation into a normal result, or make ordinary Error data abort a call.
- Id below upper-language policy; modules own their representations and interact
  by messages. No shared catalog of foreign layouts, cached service entries,
  forward-filled service tables, or alternate dispatcher.
- Cold bootstrap, Siphon's bounded initial handoff, module loading and reentry,
  initialization/publication behavior, and code residency after partial failure.
- Explicit reachability, allocation/construction order where observable, byte and
  native resource lifetime, callback reentry, collector/finalizer operation,
  method generation ownership and image restoration.
- Existing stackless frames, slots, checkpoints, suspension, recovery,
  cancellation and cleanup. Language continuations must not be moved onto the
  Rust/C call stack. Private synchronous native calls are not continuations.
- Existing local scheduling and process-worker concurrency, owner-thread rules,
  transport semantics, callback boundaries and independent worker heaps.

Carrier helpers are replaceable implementation mechanisms, not perpetual source
semantics. Redesign/delete their actual consumers together. This does not permit
bypassing a source-level operation that remains in the new implementation.

## Current implementation and why a signature edit is insufficient

The baseline is `a271e4c5`. [Inventory](../measurements/oop-only-id-inventory-20261022.json)
records tracked file references and the existing release-validation dependency
plan. Counts are source occurrences, not work estimates or dynamic costs. Most
references in Siphon are integration tests of production modules.

| Area | Current carrier dependency | Required migration |
| --- | --- | --- |
| `birnam-abi/src/id_protocol.rs` | Six Operand kinds; status/detail/Operand Outcome; method, send and initializer signatures | One Oop value domain, no successful-return packet; independently specified C contract |
| `birnam-id/src/id/vtable.rs` | Dictionary value array mixes native-entry and object Operands | Object-valued bindings and methods; preserve ordered reflection, weak entries, mutation and lookup |
| `birnam-id/src/id/id_dispatch.rs` | Custom lookup returns native entry or Unit; DNU copies Operand arguments | Lookup/method objects, Oop argument forwarding, current custom lookup and entered-code ownership |
| Id allocation/interning | Allocation sizes are Words; selector names are borrowed Bytes | Bootstrap object inputs available before upper modules |
| Siphon/Id/native loader | Path is Bytes; Id initialization returns a Protocol carrier; failures are status/detail | Explicit one-time Id handoff and object-only module initialization/loading; failure residency preserved |
| Roots/Collect/ObjectModel | Root tokens, counts, extents and identities are Words; snapshots are Bytes | Owned object protocols and explicit collector work state; no recursive allocation/collection trap |
| Value owners | `bits`, `bytes`, constructors and queries expose/consume native carriers | Ordinary object results and inputs; native interpretation stays with the responsible owner |
| Execution | Signed Method adaptation, carrier branches, captured native entries, native failure conversion | Object Method invocation, removal of carrier-only adaptation, explicit object/control failure handling |
| Generated C and wrappers | Prelude duplicates packet ABI; helpers return/extract packet layers | Oop-only calls/results and actual-FFI-specific conversion; no general Operand wrapper |
| Stackless admission | Byte metadata, raw entry/context and identity carriers | Retain verified descriptors, logical frame layout, checkpoints and entered-generation lifetime |
| Image/Generation/Artifact | Native entries, metadata, relocation and compiled ABI assumptions | Updated executable admission and explicit rebuild; independent format/relocation checks |
| Host/Isolate | Load paths, diagnostics, callbacks and worker initialization use carriers | Real CLI/worker startup through the new boundary and preserved diagnostics/control behavior |

Live signatures currently include:

```text
Method(context, receiver, Operand*, count) -> Outcome
Message(context, receiver, selector, Operand*, count) -> Outcome
Initialize(protocol, Operand*, count) -> Outcome
```

The initial metacircular graph installs native method entries before upper modules
exist. Its allocator needs an extent; interning needs byte contents; loading needs
names/paths and initializer ownership. Roots and GC also need numerical tokens
and extents before the full value stack is available. These dependencies must be
resolved in the object model, not hidden in a new `RawValue` or `NativeResult` type.

## Bootstrap ownership question, revised after source comparison

The initial question asked whether Id may own a minimal set of real bootstrap
objects for machine words, bytes and native methods, with Integer/String and
other language policy remaining above Id, or whether all value representations
must remain above Id. This was premature as a binary ownership choice: direct
construction of the initial object graph and ownership of the resulting value
behaviors are distinct decisions. The user requested an explanation and comparison
with Piumarta; neither initial option has been selected.

This matters because the existing organizing rule says Id knows no upper language,
and representations belong to their functional owners. Its raw-carrier exception
currently supplies the very data used to construct/load those owners. Removing
the exception while preserving the other rule is not a mechanical rewrite.

If minimal bootstrap objects are allowed, the design still must establish:

1. They are genuine messageable objects with identity, behavior, tracing and
   replacement semantics, not addresses of temporary Operand structs.
2. How their public protocols support native consumers without decoding another
   owner's private payload or exporting a replacement raw service interface.
3. How allocation sizes, root tokens and collector enumeration avoid a dependency
   on recursively allocating the objects needed to perform allocation/collection.
4. How native method objects retain callable code/context and remain separate from
   language Methods only where an actual semantic distinction requires it.

If they are not allowed, the initial object construction/loading order needs a
concrete replacement that does not secretly expand Siphon into an upper-runtime
loader or introduce a parallel primitive API. No such replacement is claimed yet.

The next design step is a concrete minimal bootstrap, distinguishing private seed
construction from ordinary post-bootstrap Id messages. Do not assume removing
carriers requires adding Word/Bytes language policy to Id, or imitate a raw native
service table to avoid that decision. Failure representation and the exact C
initializer/handoff signature also remain to be settled before freezing the ABI.

### Piumarta source comparison

Inspected the author's [Open, extensible object models paper](https://piumarta.com/software/cola/objmodel2.pdf),
especially sections 3.1–3.3 and Appendix B, and the author's
[2007-09-18 source snapshot](https://piumarta.com/software/cola/idst-20070918.tar.gz),
especially `object/id/libid.c`, `object/id/id.h` and `object/st80/SmallInteger.st`.
This describes those sources, not every later Id/COLA version.

The paper constructs the initial object/symbol/vtable graph directly and installs
the essential methods progressively. Ordinary sends become available during that
process. It also assigns behavior to tagged small-integer references and nil;
being an object does not require a separately allocated payload for every value.

The implementation has direct Oop method returns and variadic native arguments,
without Operand/Outcome packets. However, primitive calls accept raw `size_t`,
`const char *` and function pointers, including after bootstrap. Its `__libid`
interface also exports native helpers. Consequently it is not evidence that a
strictly Oop-only, message-only inter-module boundary has already been solved.
Adopt the small explicit bootstrap lesson; do not import the broader raw interface
or claim it satisfies the user's stronger Birnam requirement.

### Fixed Oop calling convention and performance candidates

A fixed C signature borrowing `const Oop *` plus a count and returning one Oop
works with the pinned Rust 1.95 and the vendored TinyCC on this Apple ARM64 host.
An isolated probe checks Rust-to-TCC and TCC-to-Rust values at arities 0, 1, 4, 8,
17 and 64. It uses prebuilt argument arrays; the production runtime must retain
its explicit argument/frame ownership, including across suspension. There is no
requirement to implement language continuations using C varargs or native stacks.

[Probe evidence and sources](../measurements/oop-id-call-feasibility-20261022.json)
record six alternating timing pairs. Median transport-only CPU is approximately
1.76 ns/call for Oop return versus 4.85 ns/call for the existing packet shape.
These deliberately small echo probes contain no Id lookup, GC, effects, errors
or allocation/interning. They establish calling-convention feasibility, not a
qualified runtime speedup. The pinned Rust compiler separately rejects variadic
function definitions with E0658; newer online Rust documentation describes newer
support and must not be substituted for the pinned toolchain's capabilities.

Two strict-boundary candidates have now received focused probes; full production
integration and measurement remain outstanding:

- **Allocation sizes:** a tagged, nonnegative size immediate can be an actual Oop
  with ordinary behavior, requiring no wrapper allocation. The target's valid
  object extents are bounded by allocator limits. This must be integrated with
  dispatch, GC, reflection and images; casting a number to an otherwise invalid
  pointer is not an implementation. Do not silently change the identity contract
  of existing heap Integers or truncate full-width Word data. Reused fixed-size
  objects are another option for constant allocation shapes.
- **Interning:** accept an immutable name object and perform canonicalization in
  the owner of its byte representation. Return a Symbol Oop to Id. Existing name
  objects and compiled name literals can be reused; normal sends already carry
  Symbols and need no new name object. Hashing/comparison remain necessary, but
  packet conversion and a wrapper allocation per request are not inherent. Do
  not obtain speed by decoding another owner's private String/Bytes payload.
  The ownership/construction path, dynamic-name fallback and lifetime behavior
  need to be specified before claiming this works in the production module graph.

Neither candidate is evidence that the full strict runtime has been delivered.
Measure real allocation and interning workloads and the complete language before
concluding that a raw argument exception is necessary or that an alternative is
fast enough.

### Focused probe results

[Report](oop-only-id-probes.md) and
[evidence](../measurements/oop-id-design-probes-20261022.json) use Id's production
allocator, symbol table, bootstrap and dispatch, with test-only immediate receiver
recognition. All 20 Id unit tests pass in debug and release, including five new
semantic probes. Timing is an explicitly ignored, separately invoked test.

No consistent allocation penalty was observed for tagged sizes. Existing name
objects are close to C-string inputs in interning cost, while the stronger raw
pointer-plus-length control remains faster by a few nanoseconds for short names.
Creating a fresh Name wrapper on each interning hit roughly doubles its cost.
Therefore retain the strict boundary as the target, use actual retained name
objects, and reject per-call wrapper construction as the default implementation.

These probes do not establish full-language readiness. In particular, test-local
Name/Id ownership permits direct allocation-metadata validation; moving that work
across a module boundary requires ordinary messages or owner-local metadata and
must be measured. Production GC/reflection/image recognition of immediates, the
strict loader and complete stackless/concurrent execution remain unqualified.

## Implementation sequence and acceptance boundaries

1. **Object contract and independent oracle.** Specify bootstrap objects, lookup
   answers, absent/unbound behavior, failure/control propagation and the one-time
   native handoff. Write the independent C declarations/fixture before changing
   the production declarations. The fixture must use production Id; a toy second
   dispatcher is not acceptance evidence.
2. **Id/Siphon vertical cutover.** Object-valued dictionaries, methods, dispatch,
   allocation, interning and actual native loading. Exercise Id with upper modules
   absent, custom lookup/DNU replacement, nested sends and loader failures. Reject
   old module descriptors before calling a mismatched function pointer.
3. **Memory and core owners.** Roots, Namespace, collection, object behavior,
   primitive values and Bootstrap. Exercise explicit collection during construction,
   nested callbacks, metadata retirement and upper-module loading. Do not normalize
   all allocation changes as acceptable without checking observable identities.
4. **Execution and generated code together.** Remove incoming/outgoing carrier
   adapters and replace native signatures where they describe only the deleted
   carrier convention. Keep actual C FFI signatures where needed. Preserve logical
   frame plans, calls, suspension, handlers, recovery and active generations. Rebuild
   authored modules; migrate the independent authored prepared-call implementation.
5. **Persistence, host and workers.** Artifact/image admission, explicit rebuild,
   relocation, native ownership, CLI/project operations, Isolate startup and worker
   transport. Rebuild the complete candidate in a separate prefix.
6. **Deletion and complete qualification.** Remove carrier exports, all production
   references and obsolete fixtures/helpers. Verify the final source and generated
   ABI have only Oop message values; a grep alone is not sufficient. Run the full
   preservation matrix and repeat Logue profiling on the qualified candidate.

Intermediate commits/checkpoints must state their actual coverage. A floor-only
pass is not a working language. Do not install a partially migrated prefix or
report a completed cutover while upper callers still depend on removed symbols.
The existing installed prefix remains the comparison and rollback baseline.

## Validation matrix

Use the existing validation plan captured in the inventory, extending fixtures
for the new protocol without weakening their language assertions:

| Obligation | Existing checks to retain and extend |
| --- | --- |
| Actual Id and independent ABI | Id/ABI unit suites; `id-protocol-v1` independent C fixture migrated to the new ABI; `scripts/siphon-test.sh` |
| Real module closure and memory | `scripts/native-message-test.sh`; module registry, collection, owner-root, callback and publication-failure native suites |
| Language/malleability | Full language suite; current lookup/replacement/getter/factory and Error-value oracles; headless Logue/state checks |
| Stackless execution | `stackless-s2`, portable-code independent oracle, `execution_machines`, native task-control fixture, authored prepared-call reference |
| Loading and retained code | Execution Method/binding tests; Generation suites; partial-load failure; module-registry retention; native lifetime/relocation checks |
| Images | Independent image-format reader; mapped and explicitly rebuilt images; recovery conflicts, pending work and retained native code |
| Concurrency | Installed `host-tasks`, task-control cancellation/cleanup, Store-independent task tests, process workers and native callback/owner-thread checks |
| Installed behavior | Installed contract; `host-cli`, `host-image`, `host-interaction`, `host-generation`; real Termbox Logue movement/floor redraw |
| Cold bootstrap | Release plan `cold-source`, isolated archive and dylib audits; no dependence on the currently installed prefix or stale prebuilt artifacts |
| Performance | Quiet paired Logue/microprobe CPU, wall time, GC pauses and allocation counts; no instrumentation in timing runs |

Pre-migration check run on 2026-10-22:

```sh
MISE_TRUSTED_CONFIG_PATHS="$PWD" CARGO_TARGET_DIR=.cargo-target/simplifications \
  mise exec -- cargo test --offline --locked -p birnam-abi -p birnam-id --lib
```

Both suites pass. The recorded baseline log is included in the inventory. This
checks the current carrier implementation only. No new ABI, loader, concurrency,
image or stackless migration has been validated yet.
