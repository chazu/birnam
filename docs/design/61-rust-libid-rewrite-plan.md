# 61 — Rust libid Floor Rewrite Plan

> **Status:** R0 through R7 are complete as of 2026-07-23. The Rust floor is the
> verified default, the Zig oracle remains available, and the R8 observation
> window/retirement checkpoint is pending. This plan
> replaces the
> implementation of
> `libbirnam_id.dylib` with Rust while preserving the current
> synchronous `IdApi`, object representation, generated-code ABI, image format,
> and higher runtime rungs. It does not authorize implementation by itself.
>
> Builds on [32 — Refound on libid](32-refound-on-libid.md),
> [40 — The Image](40-the-image.md),
> [42 — Object-Canonical, Relational-Derived](42-object-canonical-relational-derived.md),
> [46 — Benchmarks](46-benchmarks.md),
> [54 — Relocatable Bundles](54-relocatable-bundles.md), and
> [60 — Stackless Native Execution](60-stackless-native-execution.md).

## 1. Outcome

The completed rewrite has one externally visible effect: the runtime library
found as `libbirnam_id.dylib` is built from Rust rather than `src/id/id.zig`.
The Zig siphon still performs one raw `dlopen`, resolves `id_init`, and obtains
the same append-only C-layout `IdApi`. Existing Zig modules and compiled Birnam
artifacts load without recompilation or source changes attributable to the
floor's implementation language.

The replacement is complete only when:

- the Rust floor exports the same `id_init` symbol and complete `IdApi` layout;
- a committed language-neutral ABI manifest and frozen pre-rewrite native
  fixture prove append-only compatibility independently of either implementation;
- a clean source checkout cold-builds with the declared Zig and Rust toolchains,
  without any prebuilt Birnam binary, image, compiled module, or other Birnam
  artifact;
- the current object payload and `object[-1]` vtable representation are preserved;
- both floors pass the same external conformance and downstream suites;
- images produced by either floor save, map, rebuild, relocate, and boot through
  both floors, including a frozen pre-rewrite image;
- installed and source-free project/application artifacts remain relocatable;
- Alien callbacks, root providers, trace hooks, finalizers, and module reloads
  continue to cross the ABI correctly;
- benchmark comparison stays within Design 46's existing regression thresholds;
- the Rust floor becomes the default only after an explicit dual-floor gate;
- the Zig implementation remains available as an oracle through the cutover
  checkpoint and is retired in a later, separate commit.

## 2. Scope boundary

### 2.1 In scope

This plan rewrites the floor currently owned by:

- `src/id/id.zig` — allocation, objects, vtables, symbols, dispatch, bootstrap,
  value types, environment, GC, layouts, finalizers, reflection helpers, and the
  `id_init` table;
- the Rust mirror of `src/id/idapi.zig` required to implement that table;
- build, test, CI, install, distribution, documentation, and benchmark plumbing
  needed to ship the Rust dylib;
- a backend-neutral test seam that can run higher-rung Zig tests against either
  floor during migration.

Phase R0 introduces a committed language-neutral manifest for the already
public ABI. The manifest freezes field names, order, offsets, sizes, callback
signatures, constants, and a versioned fingerprint. `src/id/idapi.zig` remains
the Zig spelling consumed by higher rungs, but both it and Rust must verify
against the manifest. Changing the manifest is an explicit ABI change and is
not part of the rewrite merely because both implementations can be regenerated.
The manifest is build/test evidence only: it is never installed, loaded by the
runtime, consulted for artifact selection, or treated as a second runtime ABI
authority.

### 2.2 Out of scope

The rewrite does not initially port:

- `src/id/main.zig` or the Zig siphon;
- Store, relational, PEG, language, TCC, codegen, prelude, collections, errors,
  isolates, Alien, image, project, or artifact-inspection modules;
- authored Birnam modules;
- libtcc or generated Birnam C;
- the image on-disk format;
- the compiled-unit or candidate ABI;
- arm64 macOS platform support policy;
- shared-memory threading;
- a moving or fully precise collector;
- stackless dispatch, continuations, resumable methods, or the Design 60 ABI;
- changes to language-visible object, method lookup, prototype, GC, or error
  semantics.

The Rust rewrite and Design 60 are sequenced, not combined. Rust parity lands
under the existing synchronous Method ABI first. A later stackless slice can
then use the verified Rust floor as its base without losing the current Zig
runtime as a differential oracle during the language rewrite.

## 3. Contracts that must not drift

### 3.1 Binary ABI

- `Oop` remains a nullable opaque pointer.
- `Method` remains an unsafe C-callable function pointer with semantic shape
  `unsafe extern "C" fn(Oop, *const Oop, usize) -> Oop` in Rust.
- A null argument pointer is valid exactly when the argument count is zero.
  Generated C already emits this representation. A nonzero count with a null
  pointer is a contract violation.
- `IdApi` retains exact field order, size, alignment, function-pointer calling
  conventions, and nullable-pointer behavior.
- `IdApi` stays append-only; the Rust migration does not insert fields or reuse
  trailing space.
- Module initialization remains `module_init(api, prior_state) -> next_state`.
- `id_init` is the one required exported floor entrypoint.
- The siphon loads one floor and calls `id_init` exactly once for that floor's
  process-lifetime runtime. Repeated calls are outside the compatibility
  contract and must not be used as reset, isolation, or parity machinery. Tests
  that require a fresh runtime use a fresh process.
- No Rust panic unwinds across any C, Zig, generated-C, hook, finalizer, or module
  boundary.

### 3.2 Object representation

- An `Oop` points at the first payload byte.
- The preceding machine word contains the vtable pointer.
- Allocation layout is exact on the supported 64-bit target: block-base
  alignment is 16 bytes, the vtable header is 8 bytes, the payload begins at
  `base + 8` and is therefore 8-byte aligned, total size is
  `round_up(8 + requested_payload, 8)`, and reported payload size is
  `total_size - 8`. Payload bytes are zero-initialized.
- Higher modules remain free to cast their own payloads to C-layout structs.
- Object identity and selector identity remain pointer identity.
- `null` remains internal absence and is distinct from the language's `nil`.
- `Oop`-typed values may contain raw Method/code pointers in facts and
  dictionaries. Public `vtableOf`, `isSmallInt`, `isFloat`, and `isWord`
  operations must first prove exact payload membership in the heap registry;
  unknown or raw code pointers return null/false without dereferencing
  `object[-1]`. `vtableOwner` likewise remains total for arbitrary raw values.
- Ordinary `send` retains its existing valid-receiver precondition and uses a
  private unchecked header path. It does not add a heap-membership lookup to the
  dispatch hot path; sending to an arbitrary non-object remains a caller contract
  violation rather than a total reflection operation.
- Integer, Float, Word, String, Boolean, nil, Symbol, and prototype objects keep
  their current language behavior and ownership rules.

### 3.3 Dispatch and MOP

- Metacircular bootstrap bottoms out only for `lookup` on the metavtable.
- An object's vtable dictionary is the canonical method authority. For a direct
  binding, the Store mirror is derived state and may contain the exact tracked
  method, `#primitive` as a presence-only marker for an unreified binding found
  by backfill, or be absent. With no direct binding it must be absent. It may
  never contain a different method or a stale `#primitive` marker.
- After bootstrap, supported MOP mutation is Store-arbitrated. Raw floor
  add/replace/remove operations are bootstrap/private escape hatches, not a
  second supported public mutation path.
- Vtable dictionaries preserve direct addition, replacement, and removal
  semantics. Direct removal reveals the inherited parent binding naturally; it
  must not copy an inherited pointer into the child or install a tombstone.
- Delegation order and inherited-method revelation remain unchanged.
- `doesNotUnderstand:` remains overridable redispatch.
- `ensurePrivateVtable`, vtable ownership, parent mutation, vtable enumeration,
  selector enumeration, and mutation hooks remain behaviorally identical.
- Once Store has registered it, the floor calls the mutation hook before
  changing the dictionary for direct replacement or removal. The hook is a pure
  mirror notification: it may not send or allocate, and only Store's
  implementation mutates the fact log.
- A Store-tracked addition orders its effects as mutate-dictionary,
  assert-new-fact. A Store-tracked replacement orders its effects as
  retract-old-fact, mutate-dictionary, assert-new-fact. A Store-tracked removal
  orders them as retract-old-fact, remove-dictionary-entry. Raw replacement or
  removal after Store mount retracts the old mirror through the hook, performs
  the dictionary mutation, and deliberately leaves the mirror absent. Raw
  addition has no prior mirror to retract and remains absent.
- Any existing `#primitive` mirror classification is derived from the current
  direct binding; replacing or removing a primitive method invalidates the old
  classification immediately. A tracked install then asserts the exact new
  method, while a raw install leaves the mirror honestly absent.
- Store initialization and image boot backfill every direct selector on every
  currently registered real type vtable that lacks an exact tracked fact with
  `#primitive`. This includes the seed vtables and excludes internal
  dictionaries. Raw additions after backfill may remain honestly absent until a
  later tracked install or explicit backfill.
- Store's mutation authorizer runs before any tracked dictionary or fact change;
  rejection leaves both unchanged. Its mutation observer runs exactly once only
  after the tracked dictionary mutation and mirror update have completed.
- Enumeration order remains stable. Rust hash-table iteration must not leak into
  image, reflection, mirror-backfill, or test order.

### 3.4 Memory and image support

- The collector continues to find native C/TCC-held roots conservatively.
- Root providers, trace hooks, exact heap layouts, code slots, finalizers, and
  permanent objects retain their current contracts.
- Collection remains non-reentrant and refuses to sweep without a stack bottom.
- Finalizers run exactly once and may not allocate id objects.
- `objectPayloadSize`, `exportedNameOf`, and `slotKind` continue to provide the
  image layer's current relink/swizzle inputs.
- The environment and interner keep their values and symbols rooted.

## 4. Rust implementation decisions

### 4.1 Repository and artifact shape

The initial crate lives at `rust/birnam-id/` and builds a library named
`birnam_id` as a `cdylib` only:

- `libbirnam_id.dylib` is the production drop-in floor;
- every cross-language conformance fixture dynamically loads that dylib, so each
  test process contains exactly one floor runtime, heap, interner, and canonical
  object universe.

The crate begins with no third-party dependencies. Rust standard-library
collections, allocation APIs, C types, and explicitly declared macOS functions
are sufficient for the current target. Any later dependency requires a recorded
reason, committed lockfile change, license review, and either repository
vendoring or a deliberate change to Birnam's offline/reproducible-build policy.

The Rust toolchain is pinned alongside Zig in `mise.toml` and CI. Cargo output
uses a repository-specific ignored target directory so `mise run clean` can
remove all generated state without broad or ambiguous paths.

### 4.2 Internal module boundary

The crate should separate audited unsafe mechanisms from object-system policy:

```text
abi.rs       IdApi mirror, exported entrypoints, ABI descriptor
raw.rs       Oop, Method, pointer/function conversion, object[-1]
heap.rs      aligned blocks, allocation metadata, permanence, reclamation
vtable.rs    method dictionaries, delegation, ownership, layouts
symbol.rs    symbols, string storage, interning
dispatch.rs  bind, send, DNU, bootstrap primitives
values.rs    Integer, Float, Word, String, nil, Boolean, prototypes
env.rs       import/export namespace and reverse-name lookup
gc.rs        conservative roots, tracing, providers, hooks, finalizers
loader.rs    dlopen/dlsym adapter
runtime.rs   process-global state and bootstrap lifecycle
lib.rs       crate surface and id_init export
```

The exact file split may evolve, but these concerns must not collapse into one
large unsafe module.

### 4.3 Unsafe and reentrancy rules

The crate enables `unsafe_op_in_unsafe_fn` as a denied lint. Every unsafe block
states the invariant that makes it valid.

The process runtime is single-thread-owned during this rewrite. Record the owner
thread at initialization and diagnose cross-thread entry in debug/test builds.
Do not add a global mutex: sends, hooks, root providers, trace hooks, finalizers,
and module callbacks can re-enter the floor, so a lock held across foreign code
would deadlock.

The runtime may use a deliberately audited `UnsafeCell` or raw global pointer.
No Rust reference or collection guard may survive any call to:

- a resolved Method;
- `doesNotUnderstand:`;
- a method-overwrite hook;
- a root provider;
- a trace hook;
- a finalizer;
- `dlopen`, `dlsym`, or a module initializer.

Operations take short borrows, copy the raw values they need, release the borrow,
make the foreign call, then reacquire state. This is the central soundness rule
of the rewrite.

The floor dylib and every module supplying live Method, hook, provider,
finalizer, or layout pointers remain loaded for the lifetime of those pointers.
The floor itself is process-lifetime and is never `dlclose`d after `id_init`.

The GC mark callback passed to a root provider or trace hook is valid only for
that synchronous callback invocation during an active collection. Foreign code
must not retain or invoke it later. The callback may re-enter only the collector's
mark operation; the collector exposes no broader mutable runtime borrow while a
provider or hook runs.

GC-managed objects are raw runtime objects, not ordinary Rust-owned `Box<T>`
values. Their lifetime is controlled by the floor's block registry and collector.
Rust destructors must not be relied upon for language object cleanup; registered
finalizers remain the explicit cross-language mechanism.

Every runnable `cdylib` profile, including development/Debug, aborts on panic.
Exported functions do not expose Rust unwinding to foreign callers. Pure Rust
unit-test targets may retain unwind assertions when useful, while FFI integration
tests exercise abort behavior separately.

Nullable C data pointers remain raw pointers in Rust, not references or
`Option<*mut T>`. Nullable callbacks may use `Option<unsafe extern "C" fn(...)>`.
Code must branch to `&[]` for a null zero-length argument buffer and must never
call `slice::from_raw_parts` with null, even at length zero.

### 4.4 Determinism and data structures

Ordered vectors remain the authority for method dictionaries, exports, interned
symbols, and vtable enumeration wherever order is observable. Hash maps may
accelerate lookup, but they are indexes over the ordered representation and must
never define enumeration or serialization order.

The allocation registry is address-keyed and provides expected-O(1) exact payload
membership. Allocation, sweep, and finalizer iteration order are not public
behavior and tests must not make them observable. If an auxiliary vector is used
internally, it does not become a new ordering contract. A linear scan of all
allocations is not an acceptable implementation of checked raw-`Oop` predicates.

The first implementation should follow the Zig allocation and dispatch shapes
closely. Data-structure redesign, inline caches, alternate hashing, compacting
GC, and tagged immediates are post-parity work requiring their own benchmarks and
decisions.

## 5. Working-state and commit policy

Every commit in Phases R0–R4 must satisfy:

- the existing Zig floor still builds and remains the default;
- `zig build test-id` passes unless the commit changes only isolated Rust
  scaffolding before Rust is wired into Zig;
- the phase's Rust tests and ABI checks pass;
- `git diff --check` passes;
- no partially implemented Rust floor is installed as `libbirnam_id.dylib`;
- a failed Rust experiment can be removed without reverting unrelated runtime
  work.

Once the dual-floor gate exists, each semantic port commit must run the smallest
relevant Rust-floor downstream test plus the Zig oracle equivalent. Full
`zig build verify` remains required at phase boundaries and before cutover.

## 6. Phase R0 — Baseline and contract freeze

**Goal:** establish the evidence against which parity is judged before adding
Rust production code.

### R0.1 — Record the boundary and non-goals

- Land this plan and its design-index links.
- Confirm the rewrite target is `libbirnam_id.dylib`, not all of `src/id`.
- Update Design 44, the design goals, and repository-facing architecture text to
  state the governing invariant precisely: cold bootstrap starts from source
  using declared non-Birnam toolchains and requires no Birnam artifact; it is
  not restricted to Zig. Record Zig and Rust as the toolchains required by this
  plan.
- Record that Design 60 begins only after synchronous Rust-floor parity.
- Record the current arm64 macOS-only constraint.
- Inventory every higher-rung test that imports `id.zig` or calls `id_init`
  directly, grouped by file, so the adapter work has a finite migration ledger.

**Gate:** documentation links resolve; no build behavior changes.

#### R0.1 execution record — 2026-07-22

The rewrite boundary and non-goals above are now reflected in Design 44, the
repository design index, and README architecture text. The build already rejects
anything except arm64 macOS at configuration time. The finite higher-rung test
migration ledger is:

| Test root | Exported `id_init` calls | Other private `id.zig` dependency |
|---|---:|---|
| `alien_test.zig` | 2 | — |
| `codegen_test.zig` | 1 | — |
| `collection_test.zig` | 1 | — |
| `error_test.zig` | 1 | — |
| `image_test.zig` | 2 | — |
| `isolate_test.zig` | 1 | — |
| `lang_test.zig` | 1 | — |
| `peg_test.zig` | 6 | — |
| `prelude_test.zig` | 3 | — |
| `relational_test.zig` | 1 | — |
| `store_test.zig` | 14 | one direct `id.bind` assertion |
| `tcc_test.zig` | 7 | — |
| **Total** | **40** | **one assertion** |

R0.4 owns conversion of these twelve roots to the selected-floor adapter. The
direct `id.bind` assertion remains floor-private or is replaced by an external
send/lookup behavior test; it cannot pass through `IdApi` as written. This table
is the migration ledger, not a promise that future test roots may import the Zig
floor directly.

### R0.2 — Freeze a language-neutral ABI manifest

- Add `abi/idapi-v1.json`, a committed manifest outside Zig and Rust source that
  records the current
  `IdApi` field names and order, field offsets, table size/alignment, Method and
  callback signatures, pointer nullability, slot constants, target assumptions,
  evidence-schema version, and deterministic fingerprint. This does not add a
  runtime version field to `IdApi`.
- Add a Zig test that computes the same inventory from `idapi.zig` and compares
  it with the committed manifest.
- Require append-only additions to update the manifest explicitly and receive a
  compatibility review; regenerating both language definitions does not by
  itself establish compatibility.
- Mark the manifest as build/test evidence only. Build, install, runtime loading,
  artifact selection, and release packaging must not consume or include it as a
  runtime manifest.
- Add a focused `test-id-abi` build step and include it in `test`.
- Verify with a temporary insertion/reordering mutation that the unchanged
  manifest rejects the change.

**Gate:** the committed manifest exactly describes the current Zig ABI, an
unreviewed source mutation fails before any function pointer is invoked, and a
runtime/install smoke proves no code or installed artifact depends on the JSON
file.

#### R0.2 execution record — 2026-07-22

`abi/idapi-v1.json` now freezes the 54-field, 432-byte arm64 macOS table and its
supporting callback types under fingerprint
`b04684b3107927c86b41e613e941df1694fb7b52ec401e423620fcc174a0dee2`.
`src/id/idapi_abi_test.zig` derives field names, order, offsets, sizes,
alignments, nested C-callable signatures, top-level nullability, target facts,
and slot constants from the compiled Zig types and compares each item before
checking the fingerprint. `zig build test-id-abi` is part of `zig build test`.

A temporary `dlopen`/`dlsym` source reorder was rejected on the first mismatched
field before runtime initialization; restoring the declarations returned the
gate to green. The relocated install smoke also rejects either the JSON or its
test-only Zig carrier if they appear in the installed tree, then boots and uses
that tree without checkout-relative evidence.

### R0.3 — Freeze old-binary ABI and image fixtures

- Under `abi/fixtures/idapi-v1/`, compile a small C or native module once against
  the pre-rewrite table and keep the binary fixture plus reproducible source.
  Exercise early and late `IdApi` fields, callbacks, a zero-argument send, and
  module state threading.
- The fixture dynamically loads the selected floor dylib; it must not link a
  floor implementation statically or load both floor implementations into one
  process.
- Store a representative pre-rewrite image in the same versioned fixture area,
  containing canonical prototypes, user objects, code slots, layouts, exported
  identities, and deferred objects.
- Record fixture hashes and prohibit rebuilding them as part of ordinary parity
  tests. A separate documented refresh is an ABI/image compatibility event.
- Treat both frozen binaries as compatibility evidence only. Cold bootstrap,
  ordinary build, install, runtime selection, and stage1 must succeed when the
  fixture directory is unavailable; compatibility tests opt into it explicitly.

**Gate:** the current Zig floor loads the frozen native fixture and frozen image
from a clean build; rebuilding current Zig modules is not accepted as a
substitute for this gate.

#### R0.3 execution record — 2026-07-22

`abi/fixtures/idapi-v1/` now contains the frozen arm64 macOS evidence and its
reproducible source. The C client dynamically loads one selected floor, calls
`id_init` exactly once, and covers early and late table entries, null
zero-argument buffers, scalar and string values, vtable reflection and private
ownership, layouts, GC callbacks, environment lookup, and module state. Its
companion dylib is loaded solely through the selected floor's `dlopen` and
`dlsym` entries. A clean manual source rebuild produced byte-identical native
artifacts.

The image records canonical prototypes, exported scalar/text/collection values,
a private receiver and compiled method, an Integer method, and a deferred block
which closes over a recorded global. Both mapped boot and `--rebuild` return the
frozen probe sequence `41, 73, 42, 42, 3, 9` through the installed runtime.

The committed artifact hashes are:

| Artifact | SHA-256 |
|---|---|
| `idapi-v1-client.arm64-macos` | `1f9161bfef1e5765dd2d949010656677ea4817be00409489ee387f9245ac8e53` |
| `idapi-v1-module.arm64-macos.dylib` | `8193e8895b07a5ae579b8a76948a06cd50acb3a6e9dddca4a8bea96d29dcfa51` |
| `pre-rewrite.image` | `b74815c9e8b7daee87a79a5c9134620855f1536c2b9fdaebba2a755bdda9ca51` |

`zig build test-id-abi-fixture` is the explicit compatibility gate and never
rebuilds the evidence. For the independence proof, the entire fixture directory
was moved outside the checkout, `zig build install` completed into a new
isolated prefix (including stage1), and that prefix booted `birnam`; the fixture
directory was then restored. It is therefore absent from cold-bootstrap,
ordinary-install, runtime-selection, and stage1 dependency closure.

### R0.4 — Add floor-behavior conformance tests

Move or duplicate the current externally observable `id.zig` tests into a
backend-neutral suite. Cover:

- metacircular bootstrap and user dispatch;
- direct removal revealing inheritance;
- DNU override and the exact current forwarding cap of 16 original arguments;
- full-width Integer, Float, Word, String, Boolean, and nil behavior;
- prototype ownership and private-vtable splitting;
- parent changes and direct selector enumeration;
- Store-arbitrated addition, replacement, and removal, including exact
  retract/mutate/assert ordering; raw replacement/removal leaving an honest
  absent mirror; raw addition remaining absent; inherited revelation after
  direct removal; immediate `#primitive` invalidation; real-type backfill to
  honest `#primitive` presence at init and after image boot; authorizer rejection
  before any change; and exactly-once post-commit observer notification;
- import/export and reverse exported-name lookup, including duplicate aliases
  and same-name rebinding so the current deterministic name choice is frozen;
- payload size and slot-kind behavior;
- safe predicate behavior for Method/code pointers and arbitrary aligned
  non-object addresses stored in `Oop`-typed positions, including total
  `vtableOwner` behavior;
- zero-argument sends represented by `(args = null, count = 0)`, plus rejection
  of `(args = null, count > 0)` in a child process;
- exactly one `id_init` call per loaded-floor process, with fresh-process tests
  providing isolation rather than repeated initialization;
- root providers, trace hooks, finalizers, permanents, manual GC, and auto-GC.

Tests assert public behavior through `IdApi` wherever possible. A floor-private
unit test is allowed only for an invariant with no external observation.

Replace the direct exported-`id_init` setup used at 40 call sites across the 12
higher-rung test files with a once-cached API per test worker or a fresh worker
process. Zig-floor private tests may call internal `boot`/private setup, but a
repeated exported `id_init` call is not parity evidence.

**Gate:** the conformance suite passes against the Zig floor without weakening
the original behavioral coverage. Unsafe public predicates and `vtableOwner`
are fixed in Zig with expected-O(1) exact-address membership before the Rust
implementation begins, every shared worker calls exported `id_init` once, and
repeated initialization is not added as a compatibility promise.

#### R0.4 execution record — 2026-07-22

`src/id/floor_adapter_test.zig` now owns the selected-floor process boundary:
it opens exactly one dylib, requires the diagnostic identity `zig-id-v1`, calls
`id_init` once, caches the resulting table, and retains the dylib handle. A
negative test proves that an unexpected identity is rejected before
initialization. All 40 setup calls in the twelve-root migration ledger now use
that adapter; the one private `id.bind` assertion was replaced by public vtable
lookup behavior.

`src/id/conformance_test.zig` exercises the external table in fresh processes
and covers dispatch, removal and inheritance, DNU and its 16-argument cap,
boxed values and singletons, prototypes and private families, vtable parents and
enumeration, environment aliases and rebinding, payload/layout behavior, null
argument-buffer rules, exact object/vtable provenance predicates, and the
collector lifecycle. The collector group includes root providers, conservative
and precise trace behavior, finalizers, permanents, stack-bottom refusal,
manual collection, and auto-collection. The null/nonzero argument violation is
proved by expected abnormal termination in a child process.

The Store conformance group freezes the complete MOP consistency law: tracked
addition/replacement/removal ordering, honest raw-mutation absence, inherited
revelation, `#primitive` invalidation and backfill, internal-dictionary
exclusion, authorizer rejection before mutation, exactly-once post-commit
observation, and post-image-load backfill. The existing downstream suites also
run through the selected floor rather than importing the implementation.

The initial exact membership repair exposed generic pointer hashing on the
dispatch hot path: the LSP dependency index no longer completed before its
protocol synchronization point. The registered-address maps now use exact
equality with a multiplicative aligned-address hash. This preserves expected
O(1) total predicates while returning the original ten-second LSP smoke to
green.

The closing gate passed:

- `zig build verify`: 255 native tests passed, 3 skipped; 328 self-hosted tests,
  26 REPL goldens, install/project/source-fallback/image/LSP smokes, and all
  other verification steps passed;
- `zig build test-id-abi-fixture`: the frozen native client, module, mapped
  image, and rebuilt image passed;
- `zig build -Doptimize=ReleaseSafe test-id-conformance`: passed, including the
  expected child-process rejection;
- `zig build -Doptimize=ReleaseSafe test-store`: passed.

### R0.5 — Capture compatibility and performance baselines

- Run `zig build verify` from a clean build.
- Run the ReleaseSafe benchmark suite under Design 46 conditions and retain the
  development result long enough for the Rust comparison.
- Add a native ReleaseSafe baseline for exact object membership, exact vtable
  membership/`vtableOwner`, `vtableOf`, `isSmallInt`, `isFloat`, and `isWord`
  over several heap and vtable populations. Record both time per call and
  scaling slope; the repaired Zig implementation must be expected O(1) before
  it becomes the Rust comparator.
- Record binary size and dynamic dependencies of the installed id dylib.
- Save representative outputs for image save/map/rebuild, source fallback,
  Alien callback, isolate worker boot, and relocated install smoke.
- Record current allocation/live-count results from GC stress tests.

**Gate:** the baseline is reproducible on a second clean run within the existing
benchmark noise model; unexplained red gates are resolved before Rust work.

#### R0.5 execution record — 2026-07-22

Three clean, non-quick Design 46 development runs were made in ReleaseSafe on
an Apple Mac15,12. The first-to-third comparison is fully green: the earlier
second-run macro reds returned to +2.0% for stage1 compilation and -0.2% for the
primes script, while the surrounding one-shot GC measurements agree within
0.3%. The second run's isolated GC value is retained as the observed outlier,
not selected as the reference. The complete third run is committed at
`bench/baselines/zig-id-r0.5-design46.json`.

`zig build -Doptimize=ReleaseSafe bench-id-membership` now runs one
selected-floor executable against 1,035, 16,384, and 131,072 registered
vtables (5,162 through 265,236 live objects). Two independent five-repetition
runs are aggregated in
`bench/baselines/zig-id-r0.5-membership.json`. Median net time at the
smallest and largest populations was:

| Operation | Small ns/call | Large ns/call | scaling exponent |
|---|---:|---:|---:|
| `objectPayloadSize` | 1.800 | 4.174 | 0.2092 |
| `vtableOf` | 1.928 | 4.163 | 0.1904 |
| rejected `vtableOf` | 1.919 | 1.314 | -0.0960 |
| `vtableOwner` | 4.003 | 10.996 | 0.2565 |
| `isSmallInt` | 2.766 | 2.668 | -0.0090 |
| `isFloat` | 2.357 | 2.692 | 0.0339 |
| `isWord` | 2.638 | 2.710 | 0.0067 |

The exact-address algorithms remain hash-table O(1); the positive object and
vtable paths show the recorded sublinear cache-size effect rather than linear
registry scanning. These values, including their min/max bands, are the Rust
comparison contract.

The installed ReleaseSafe `libbirnam_id.dylib` is 307,760 bytes with
SHA-256
`698f1b2e17cd2e833034231b2d4722133c0c86820b1f94a6848564a3c5ba5711`.
Its install name is `@rpath/libbirnam_id.dylib` and its sole dynamic
dependency is `/usr/lib/libSystem.B.dylib`.
`bench/baselines/zig-id-r0.5-compatibility.json` records the complete
machine-readable binary, frozen fixture, source-fallback, Alien callback,
isolate, relocated-install, and GC evidence.

The public GC count probe recorded manual collection
42 -> 2,042 -> 42 with all 2,000 objects reclaimed, 1,000/1,000 finalizers
called with the live count returning to 42, and automatic collection limiting
6,000 allocations of 4,096-byte payloads to a live delta of 1,911.

The ReleaseSafe audit exposed one pre-existing test-link defect before the
baseline was closed: Zig eliminated exported fixtures and Codegen artifact
shims reached only by `dlsym` names. TCC-linked test executables now publish
their declared symbols dynamically, and Alien/Codegen retain name-only exports
at the compiler boundary. Alien's 42 tests, TCC's 7, and Codegen's 49 pass in
both Debug and ReleaseSafe. The full `zig build -Doptimize=ReleaseSafe test`
and final `zig build verify` gates pass.

## 7. Phase R1 — Rust crate and ABI shell

**Goal:** produce an isolated Rust artifact whose data layout is proven compatible
but which is not yet selected by normal builds.

### R1.1 — Pin and isolate the Rust toolchain

- Add Rust to `mise.toml` and CI using one pinned toolchain.
- Add the repository-specific Cargo target directory to `.gitignore`.
- Extend `mise run clean` to remove that exact generated directory.
- Add `mise run rust-check` and `mise run rust-test` tasks.
- Commit `Cargo.lock` even while the crate has no external dependencies.
- Run Cargo verification with `--locked --offline` after the pinned toolchain is
  installed. Any future dependency must not silently turn bootstrap into a
  network operation.
- Add a clean source-bootstrap smoke that starts with no Birnam binary, image,
  compiled module, or generated floor artifact and builds using only the pinned
  declared Zig and Rust toolchains.

**Gate:** the old `mise run build` and `mise run verify` are unchanged; the new
Rust tasks work `--locked --offline` on a clean checkout, and the cold-build smoke
proves that adding Rust did not introduce a network or prebuilt Birnam dependency.

#### R1.1 execution record — 2026-07-22

Rust 1.95.0 is pinned beside Zig 0.15.1 in `mise.toml` and CI. Cargo uses the
repository-local ignored `.cargo-target` directory, which `mise run clean`
removes explicitly. `Cargo.lock` is committed, and both `rust-check` and
`rust-test` run `--locked --offline` against the dependency-free, inert
`birnam-id` workspace member.

`mise run bootstrap-smoke` exports a clean source archive into a temporary
directory, removes the generated ABI-fixture evidence directory, rejects any
pre-existing Birnam executable, image, compiled module, or dynamic library,
then checks and tests the Rust workspace offline and builds the complete Zig
toolchain from source. This passed with the pinned toolchains. The unchanged
`mise run build` and full `mise run verify` gates also passed, including source
fallback, project, REPL, LSP, image, isolate, and relocated-install coverage.

### R1.2 — Create the crate without production wiring

- Create `rust/birnam-id/Cargo.toml` with a `cdylib` output only.
- Set the library name so Cargo emits `libbirnam_id.dylib` on macOS.
- Add crate lints, abort-on-panic settings for every runnable cdylib profile,
  including development/Debug, and module skeletons.
- Export only a diagnostic crate/version symbol at this point, not a usable
  `id_init`.

**Gate:** `cargo test` and a direct dylib symbol probe pass; Zig still installs
the Zig floor.

#### R1.2 execution record — 2026-07-22

The dependency-free workspace member now builds `birnam_id` exclusively as a
`cdylib`, producing `.cargo-target/{debug,release}/libbirnam_id.dylib`. Its
planned internal module boundaries exist, but its only exported Birnam symbol
is the diagnostic `birnam_id_rust_crate_identity`, which answers
`rust-id-shell-v1`. It deliberately does not export `id_init` or the
selected-floor identity.

Development and release profiles set `panic = "abort"`, and a compile-time
guard rejects any non-test dylib build whose panic strategy is not abort. Pure
Rust unit tests retain the test harness's unwind behavior. The direct Zig
loader probe opens and calls both debug and release Rust dylibs, verifies their
identity, and proves `id_init` is absent. The same smoke runs an ordinary Zig
build and proves that `zig-out/lib/libbirnam_id.dylib` still exports `id_init`
and does not contain the Rust-shell identity. `cargo test --locked --offline`
and the complete `rust-shell-smoke` pass locally and are wired into CI.

### R1.3 — Mirror primitive ABI types

- Define `Oop`, Method, callbacks, slot constants, module initialization, and
  `IdApi` in `abi.rs` with C representation.
- Represent nullable object/data pointers exactly as raw pointers. Use `Option`
  only for nullable function pointers with a guaranteed C-compatible nullable
  representation.
- Avoid Rust references and Rust enums in the public ABI.
- Declare foreign callable functions as `unsafe extern "C" fn`.
- Add Rust compile-time/runtime assertions for pointer width, sizes, alignments,
  and field offsets on the supported target.

**Gate:** both Rust and Zig descriptors equal the committed R0 manifest exactly.

#### R1.3 execution record — 2026-07-22

`abi.rs` now mirrors all 54 fields of the 432-byte `IdApi` v1 table with
`#[repr(C)]`. `Oop`, argument buffers, layout arrays, C strings, and opaque
state use raw pointers; every callable type is `unsafe extern "C" fn`; and
`Option` appears only for the nullable method-overwrite callback. The eight
supporting callback/initializer types, four slot constants, pointer width,
`c_int` width, table size/alignment, and every field offset have compile-time
assertions plus a Rust runtime layout test. A target guard rejects builds that
would silently apply this arm64 macOS manifest to another ABI.

Rust check/test, debug and release `cdylib` builds, the direct loader smoke, and
the independent Zig manifest verifier all pass. No `id_init` implementation or
normal-build selection was introduced.

### R1.4 — Add a cross-language ABI probe

- Export a diagnostic ABI descriptor from the Rust dylib without adding it to
  `IdApi`.
- Have the Zig `test-id-abi` probe dynamically load the Rust artifact and compare
  all layout entries and constants with the committed manifest.
- Make the test depend on building the Rust artifact, but keep the normal
  runtime linked to the Zig floor.
- Verify that deliberately reordering one local Rust field makes the test fail
  before any function pointer is called.

**Gate:** exact layout parity is mechanically enforced against an immutable
expected fingerprint in both local and CI verification. This phase does not call
`id_init` or run the frozen old-client fixture.

#### R1.4 execution record — 2026-07-22

The Rust shell exports `birnam_id_rust_abi_descriptor_v1`, a pointer-free
build/test descriptor reporting the Rust compiler's concrete supporting-type
layouts, all 54 field offsets/sizes/alignments/nullability/kinds, target facts,
slot constants, and frozen fingerprint. It is a separate diagnostic symbol,
not an `IdApi` field.

`zig build test-id-abi` now builds the Rust artifact with Cargo
`--locked --offline`, dynamically loads that artifact from `.cargo-target`,
asserts that `id_init` is absent, and compares the descriptor entry-by-entry
with `abi/idapi-v1.json`. It also exercises a reordered descriptor and rejects
it. A literal negative proof temporarily swapped the local Rust `dlopen` and
`dlsym` fields: compilation failed at the frozen `dlopen` offset assertion
before a dylib or function pointer could be used. Restoring the order returned
the test to green. The combined gate is part of ordinary `zig build test` and
therefore CI; full `zig build test` and the debug/release Rust shell smoke pass.

### R1.5 — Establish FFI failure policy

- Add exported-boundary helpers that abort rather than unwind on invariant
  failure.
- Add a test-only exported function that intentionally triggers a panic in a
  child process and proves no unwind crosses the ABI.
- Document which failures remain fatal, matching current Zig floor behavior.

**Gate:** the child process terminates predictably; the test runner remains
healthy and reports the correct failure.

#### R1.5 execution record — 2026-07-22

`boundary.rs` centralizes fatal exported-boundary checks in
`require_invariant`/`fatal_invariant`; neither path can return or unwind. Normal
debug and release dylibs remain `panic = "abort"`. A disabled-by-default
`ffi-test-hooks` feature adds two test-only C symbols: one panics deliberately,
and one enters the explicit fatal-invariant helper. The ordinary-artifact smoke
proves those hook symbols are absent without the feature.

`test-rust-ffi-abort` builds the isolated hook dylib offline, invokes each hook
from a separately compiled Zig child executable, and requires SIGABRT status
134 plus the expected diagnostic. Both children abort while the parent gate
stays healthy; the step is part of ordinary `zig build test`.

The parity policy is deliberately narrow. Allocation or runtime-metadata OOM,
corrupt heap/registry/layout state, owner-thread or reentrancy violations,
exhausted fixed runtime capacity, invalid ABI preconditions such as a null
nonzero argument buffer, and dispatch that reaches the floor's fatal default
`doesNotUnderstand:` remain process-fatal as they are in the Zig floor.
Ordinary absence, failed predicates, and language errors already represented by
Birnam objects remain recoverable values and must never be converted into Rust
panics. Internal Rust/Zig panic wording is diagnostic, not a compatibility
contract.

## 8. Phase R2 — Heap, vtables, and dispatch proof

**Goal:** boot and dispatch through a Rust floor without yet claiming complete
value, GC, image, or downstream parity.

### R2.1 — Implement raw aligned blocks

- Implement the exact layout formula: 16-byte-aligned base, 8-byte vtable
  header, payload at `base + 8`, 8-byte payload alignment, total size rounded to
  8 bytes, reported payload size equal to total minus the header, and zero fill.
- Track base, payload, total size, permanence, mark state, and optional finalizer
  metadata in a runtime-owned address-keyed registry. Do not expose allocation,
  sweep, or finalizer iteration order as behavior.
- Implement audited conversion between allocation base, payload `Oop`, and the
  preceding vtable slot.
- Add property-style tests over payload sizes including zero and unaligned sizes.
- Test base and payload addresses separately, header offset, reported size, and
  writable payloads with a C-layout fixture compiled outside Rust.

**Gate:** allocation-layout tests pass under Debug and release optimization; no
GC or automatic reclamation is enabled yet.

#### R2.1 execution record — 2026-07-22

The private Rust `Heap` now allocates zeroed blocks with a 16-byte-aligned base,
an 8-byte vtable header, an 8-byte-aligned payload at `base + 8`, and total size
rounded to 8 bytes. Its exact-address `HashMap` registry owns base and payload
pointers, total size, permanence, mark state, and optional finalizer metadata;
registry iteration is not exposed. Checked operations establish exact
membership before the audited payload/header conversion can dereference the
preceding word. Reported payload size includes alignment padding exactly as the
Zig floor does.

Property-style Rust tests cover requested sizes 0 through 1,025, overflow,
zero/unset headers, permanent and collectable metadata, foreign and null
addresses, vtable round trips, zero fill, and writes throughout the padded
payload. They pass in debug and release optimization. A feature-gated test hook
keeps each allocation in an isolated local heap while a separately compiled Zig
C-ABI callback inspects it without a surviving Rust borrow. That external
fixture checks 264 zero, aligned, unaligned, and page-boundary sizes against
both debug and release dylibs. Ordinary artifacts contain no test hook.

There is still no runtime-global Rust heap, collector, automatic collection, or
public allocator. `Heap` teardown only frees private harness ownership without
running language finalizers; the eventual process-lifetime runtime will not be
dropped while objects are observable. Rust debug/release tests,
`test-rust-heap-layout`, and the combined `zig build test` gate pass.

### R2.2 — Implement raw vtables and ordered dictionaries

- Implement vtable creation, direct method lookup, overwrite, growth, direct
  removal, and parent traversal.
- Preserve insertion/enumeration order independently of any lookup index.
- Represent native Method addresses without creating Rust references to foreign
  code or data.
- Implement the exact-membership mechanism and checked public `vtableOf` and
  `vtableOwner` behavior. Keep header dereference private; reflection uses the
  checked path, while dispatch uses its valid-receiver precondition without
  adding a membership lookup.
- Unit-test parent chains, overwrites, removals, empty tables, growth boundaries,
  identical-selector pointer semantics, Method pointers, and aligned non-object
  addresses.

**Gate:** pure Rust vtable tests cover every branch of the current primitive
dictionary behavior, and membership-index consistency is checked across
allocation and reclamation.

#### R2.2 execution record — 2026-07-22

Rust vtables now retain the Zig floor's 80-byte C-layout payload while runtime
records own their ordered selector and native-method arrays. A logical capacity
grows 0 -> 4 -> 8 by doubling, and raw `size`, `tally`, key/value pointers,
parent, and owner views stay synchronized. Direct add, exact overwrite, ordered
removal, direct lookup, parent-chain lookup, parent repointing, ownership, direct
selector enumeration, and append-only vtable enumeration are implemented.
Selector equality is pointer identity; removal reveals inherited behavior and
does not leave a tombstone.

Ordered vectors remain the only enumeration authority. Separate heap and
vtable `AddressMap` indexes use the same exact aligned-address multiplicative
hasher as the repaired Zig baseline and are consulted only for provenance and
reflection. Checked `vtableOf` and `vtableOwner` reject null, Method/code,
aligned foreign, mapped/unmapped, and misaligned carrier values before any
dereference. The crate-private unsafe dispatch path reads `object[-1]` without
a membership lookup only under an explicit exact-live-object precondition.
Allocation/reclamation tests prove the heap index loses membership before
storage is freed.

Native Method pointers are converted to inert dictionary words and back only
inside the audited `MethodAddress` wrapper; no Rust reference is formed to
foreign code or data. Debug and optimized Rust suites pass 16 tests covering
raw layout, empty dictionaries, growth boundaries, ordering, overwrites,
removals, inheritance, parent changes, identical-selector semantics, Method
addresses, owners, arbitrary carrier values, and reclamation consistency. A
strict all-target Clippy pass is also clean.

### R2.2a — Prove membership scaling early

- Add a ReleaseSafe microbenchmark for exact membership, `vtableOf`, and
  `vtableOwner` on registered objects, Method/code pointers, and arbitrary
  aligned non-objects at several heap populations. R3.2 extends the same gate to
  the scalar predicates once those types exist.
- Report time per predicate call and test the scaling slope, not only one
  absolute result. Runtime must remain expected O(1) with respect to live
  allocation count.
- Compare the result with the Design 46 Zig baseline. A threshold breach or
  heap-size-dependent slope is investigation work before further semantic
  porting, not a gate deferred to R6.

**Gate:** membership-backed reflection remains flat across the selected heap
sizes and meets Design 46's existing regression threshold, or an explicit
investigation and accepted performance decision lands before R2 continues.

#### R2.2a execution record — 2026-07-22

`bench-rust-id-membership` runs inside the optimized Rust dylib so one outer
loader call is excluded from the timed loops. It measures exact object
membership, checked `vtableOf`, and checked `vtableOwner` for registered
objects/vtables, Method/code addresses, and distinct aligned foreign addresses
at 1,026, 16,384, and 131,072 vtables. Each sample uses 524,288 calls, five
repetitions, loop-overhead subtraction, and the median. The gate rejects a
scaling exponent above 0.35 for any row. Registered membership, `vtableOf`, and
`vtableOwner` also compare against the corresponding R0.5 Zig per-population
maximum plus Design 46's 10% micro threshold.

Three independent runs are recorded in
`bench/baselines/rust-id-r2.2a-membership.json`. Median registered-operation
ranges were 0.977-1.746 ns for membership, 1.231-1.860 ns for `vtableOf`, and
1.378-2.753 ns for `vtableOwner`. Every absolute comparison stayed below the
Zig threshold. The largest observed maximum scaling exponent across all seven
registered/rejection rows was 0.2615 (`vtableOf.method`), below 0.35; the three
registered-operation median exponents were 0.0941, 0.0830, and 0.1369.
Scalar predicate rows remain correctly deferred to R3.2.

### R2.3 — Implement symbols and interning

- Allocate permanent Symbol objects with stable NUL-terminated names.
- Return the same pointer for repeated interning of equal bytes.
- Keep distinct symbol bytes and uninterned String bytes as separate concepts.
- Treat Symbol names as arbitrary non-NUL bytes; never require UTF-8 or call
  `CStr::to_str` on the interning path.
- Preserve stable interner enumeration/root order.
- Test empty, ASCII, non-UTF-8, punctuation/selector, keyword, and repeated names.

**Gate:** symbol identity and C-string lifetime survive unrelated allocations.

#### R2.3 execution record — 2026-07-22

The Rust interner allocates permanent 8-byte C-layout Symbol payloads whose raw
field points to separately owned, stable, NUL-terminated boxed bytes. Its
ordered record vector is the sole identity/enumeration/root-order authority;
an exact address map only validates reverse lookup. Equal byte sequences return
the same object without appending, embedded NUL is rejected, and all comparisons
operate on bytes. The implementation never calls `CStr::to_str` or otherwise
requires UTF-8.

Debug and optimized tests cover empty, ASCII, punctuation, unary and keyword
selectors, explicit non-UTF-8 bytes, repeated names, embedded-NUL rejection,
foreign carriers, and stable enumeration. An anchor's object identity and raw
C-string pointer survive 4,096 additional symbols plus unrelated collectable
heap allocations. All 20 Rust tests and the strict all-target Clippy gate pass.

### R2.4 — Implement bind and send

- Implement the metavtable `lookup` bottom-out.
- Implement ordinary bind through `lookup` dispatch.
- Invoke C-ABI Method pointers without retaining runtime borrows.
- Treat null `args` as an empty slice only when `count == 0`; never construct a
  Rust slice from null. Abort on null with a nonzero count.
- Implement DNU redispatch and the current cap of 16 forwarded original
  arguments, in addition to the leading selector.
- Add reentrant methods in Rust and Zig fixtures that send back through `IdApi`.

**Gate:** recursive/reentrant sends work without a mutex, aliasing assertion, or
borrow surviving the foreign call.

#### R2.4 execution record — 2026-07-22

The R2 harness now owns its heap, vtables, interner, and dispatch globals in a
single-thread-owned `UnsafeCell` runtime. Operation-specific accessors copy raw
objects and `MethodAddress` values out of runtime state; every collection/state
access ends before a Method is invoked. A thread-local dynamic-extent pointer
makes only the currently entered runtime available to synchronous callbacks.
There is no mutex, `RefCell` borrow flag, or Rust reference/collection guard
across foreign code. Non-owner-thread and overlapping-runtime entry are fatal
invariants.

`bind` bottoms out `lookup` on the metavtable and performs ordinary binding by
sending `lookup` to the receiver's vtable. `send_abi` preserves a null/zero
argument pair without constructing a Rust slice, rejects null/nonzero, invokes
copied C-ABI Method pointers, and redispatches overridable
`doesNotUnderstand:` with the original selector plus at most 16 original
arguments. Rust tests prove metacircular lookup, the distinct null and non-null
zero-length paths, DNU capping, and a Rust Method that reenters send.

A feature-gated external harness installs a Zig Method. That Method reenters the
active Rust dispatcher and reaches an inner Rust Method in both debug and
optimized dylibs; the inner Method answers 4,242 only when it receives the
preserved null/zero buffer. All 25 debug/release Rust tests, feature-enabled
Clippy, `test-rust-dispatch-reentry`, the ordinary-artifact no-test-hook check,
and combined `zig build test` pass. The dispatcher remains internal: there is
still no Rust `id_init` or public `IdApi` table.

### R2.5 — Implement the metacircular bootstrap

- Create the metavtable, root object vtable, internal symbol/environment
  dictionaries, and value-family vtables in the current deterministic order.
- Install the irreducible bootstrap methods in the current order.
- Exercise `addMethod` metacircularly for the minimal remaining methods required
  by the R2 bind/send/DNU proof. Value and prototype protocols assigned to R3
  remain deferred, but their later installation must preserve the current direct
  selector order.
- Allocate the final canonical nil/true/false objects and Object, Integer,
  Float, Word, String, and Symbol prototype identities with their final payload
  shapes. Later phases may complete behavior and publish exports but must never
  replace these pointers or their family vtables.
- Freeze the current lifetime classes: vtables, Symbols, nil/true/false, Object's
  prototype, and Symbol's interned prototype are permanent; Integer, Float,
  Word, and String prototypes are ordinary collectable objects kept live by the
  export environment once R3.1 publishes them.
- Bootstrap through an internal harness entry at this phase. Do not construct or
  export the public `IdApi` table yet.

**Gate:** the backend-neutral bootstrap/dispatch/DNU tests pass against the Rust
floor in an isolated harness; the complete vtable creation order and canonical
identity snapshot are recorded for later phase gates.

#### R2.5 execution record — 2026-07-22

The internal Rust harness now performs the one-way bootstrap in the Zig floor's
deterministic order. It creates `VtableVt`, `ObjectVt`, `SymbolVt`, the internal
Symbol-list and environment dictionaries, then Integer, String, Float, Word,
Nil, and Boolean family vtables. `VtableVt` describes itself and delegates to
`ObjectVt`; every other header and parent relation is checked against the
frozen 11-entry order. All vtables retain the permanent 80-byte raw shape.

The first six interned selectors are `lookup`, `addMethod`, `removeMethod:`,
`allocate`, `delegated`, and `doesNotUnderstand:`. Bootstrap installs the first
three directly on `VtableVt`, then sends `addMethod` through the metacircular
dispatcher to install `allocate` and `delegated` on `VtableVt` and the default
overridable DNU handler on `ObjectVt`. Tests use those messages to create a
delegated family, install and remove a native Method, allocate an instance,
dispatch it, and override DNU. No runtime borrow or collection guard crosses a
Method call.

The canonical nil, true, false, Object, Integer, Float, Word, String, and Symbol
identities now have their final vtables and payload shapes. Vtables, Symbols,
the three singletons, Object, and Symbol are permanent. The four boxed value
prototypes are collectable; their initial values are zero or empty bytes. The
owner map matches the prototype model exactly, including true (not false) as
the Boolean family owner. All listed identities are distinct and later phases
must extend them in place.

`bench/baselines/rust-id-r2.5-bootstrap.json` records the symbolic vtable,
selector, direct-method, canonical-object, lifetime, payload, and owner snapshot
against implementation commit `0dabfb9`. All 31 debug and optimized Rust tests,
strict all-target/all-feature Clippy, and combined `zig build test` pass. The
bootstrap remains internal and the ordinary Zig floor remains selected: Rust
still exports neither `id_init` nor an `IdApi` table.

### R2.6 — Publish the complete-shaped but spike-only API table

- Construct the full `IdApi` table in the exact current layout.
- Wire only implemented operations to real functions.
- Wire unsupported operations to named fatal stubs that identify the missing
  field; never return plausible dummy values.
- Export `id_init` as the one public transition from uninitialized dylib to the
  already-specified runtime/API construction. Each harness process calls it once;
  no reset or repeated-call semantics are implemented.
- Keep the floor handle alive for the process and do not install or select this
  dylib in ordinary Birnam commands.

**Gate:** the minimal cross-language harness can load, initialize, intern,
install a fixture method, send, and observe the answer; calling one unsupported
field fails explicitly in a child process. The canonical identity snapshot from
R2.5 is unchanged. The frozen old-client fixture remains deferred until every
field it exercises is implemented.

#### R2.6 execution record — 2026-07-22

The isolated Rust dylib now exports `id_init` and identifies itself as
`rust-id-spike-r2.6`. Initialization performs one bootstrap transition, leaks
the owning Bootstrap and full API table for process lifetime, and publishes the
runtime pointer with release/acquire ordering. A second initialization is a
fatal invariant; there is deliberately no reset or repeat-call behavior. Every
public runtime operation still enforces the bootstrap thread as its owner.

The table has the exact frozen 54-field shape. Its ten object fields publish the
R2.5 canonical identities. Fifteen callable fields use implemented mechanisms:
`intern`, `proto`, `method`, `alloc`, `send`, `symbolName`, `vtableOf`,
`objectPayloadSize`, `setOwner`, `setParent`, `vtableCount`, `vtableAt`,
`vtableSelectorAt`, `vtableParent`, and `vtableOwner`. The other 29 callable
slots are non-null, field-specific fatal stubs; none returns a plausible dummy
value. Value constructors remain intentionally fatal until R3.2 even though
R2.5 already fixed their internal prototype shapes.

The frozen-manifest descriptor test now requires the spike's `id_init` export
while still comparing every compiler-derived type and field entry. The ordinary
artifact probe requires the R2.6 identity, descriptor, and entrypoint in debug
and optimized dylibs and continues to prove that no feature-gated test hook
leaks. An ordinary Zig build still installs the Zig floor under the production
library name; the Rust spike remains outside the install prefix and is never
selected implicitly.

`test-rust-id-spike` compiles a Zig client solely against the frozen `IdApi`.
Before mutation it checks the 11-entry vtable order, parents, singleton
families, prototype owners, and canonical payload extents from the R2.5
snapshot. It then proves stable interning, creates and enumerates a delegated
vtable, installs a Zig Method, allocates an instance, and observes 4,242 through
Rust send in debug and optimized builds. A separate process calls the
unsupported `gcCollect` slot and terminates with SIGABRT after naming that exact
field. All 32 debug/release Rust tests, strict all-target/all-feature Clippy,
the updated shell smoke, and combined `zig build test` pass. The frozen
old-client fixture remains deferred exactly as planned.

## 9. Phase R3 — Complete non-GC floor behavior

**Goal:** implement every non-collector `IdApi` operation and run the lower
downstream rungs while collection remains disabled.

### R3.0 — Implement a collector-metadata shell

- Implement `gcSetStackBottom`, `gcSetAuto`, `gcSetPrecise`,
  `gcRegisterRootProvider`, `setTraceHook`, `setFinalizer`, `setLayout`, and
  `slotKind` as real, state-preserving operations before loading downstream
  modules. These are module-initialization contracts even before collection.
- Keep automatic collection disabled regardless of recorded preference.
- Make manual `gcCollect` safely return zero without marking or sweeping.
- Retain registered providers, hooks, finalizers, and borrowed static layouts so
  R4 can activate them without modules having to register again.
- Match capacity, replacement, unknown-layout, and validation behavior that can
  be observed without collecting.

**Gate:** focused Zig/C fixtures can register the maximum supported mix of root
providers, hooks, and layouts, while a private Rust fixture attaches a finalizer
to an internal collectable block, without hitting a fatal stub. Metadata remains
queryable where the ABI permits, and a forced collection reports zero without
invoking callbacks or reclaiming anything. R3.5/R3.6, after the required value
and environment phases, prove registration by the real Store, PEG, prelude, and
collection modules.

#### R3.0 execution record — 2026-07-22

The runtime now retains the stack-bottom address, requested automatic-GC state,
precise-heap flag, and an ordered maximum of 32 root providers. Per-vtable trace
hooks and borrowed static layout pointers replace in place and synchronize the
raw 80-byte vtable view. `slotKind` reports `slot_unknown` before a layout is
set, then uses the exact prefix/rest rule. A null kinds pointer is accepted only
for a zero-length prefix. Collectable blocks retain replacement finalizers;
permanent and unknown carriers ignore finalizer installation.

All ten metadata callables now use real implementations:
`gcSetStackBottom`, `gcRegisterRootProvider`, `gcCollect`, `gcLiveCount`,
`setTraceHook`, `gcSetAuto`, `setFinalizer`, `setLayout`, `gcSetPrecise`, and
`slotKind`. Requested auto/precise state is recorded but never activates a mark,
trace, callback, finalizer, or sweep. `gcCollect` always returns zero and
`gcLiveCount` remains an exact heap-registry count. Initialization also records
raw-tail layouts for the six built-in value/singleton families without changing
any R2.5 identity.

`test-rust-gc-metadata` drives the table from an external Zig fixture in debug
and optimized builds. It verifies built-in layouts, unknown new vtables,
prefix/rest replacement, live-count growth, 32 retained providers, and disabled
collection with provider, trace, and finalizer counters all untouched. Separate
processes prove that a 33rd provider and a null/nonzero layout fail with exact
SIGABRT diagnostics. The private Rust heap test attaches finalizer metadata to
a collectable block and proves permanent/foreign no-op behavior. All 35
debug/release Rust tests, strict all-target/all-feature Clippy, the R2.6 spike
gate (now using `mkSmallInt` as its still-fatal field), and combined
`zig build test` pass.

### R3.1 — Environment and loader

- Implement ordered import/export storage and `isExported`.
- Implement reverse exported-name lookup by pointer identity.
- Publish the canonical objects created in R2.5 under the current names without
  replacing any object or vtable. Freeze current behavior for duplicate aliases
  and same-name rebinding.
- Implement `dlopen`/`dlsym` pass-through with the current lazy-load behavior.
- Test reload of a tiny fixture module and preservation of its opaque state.
- Through the public table, prove floor reflectivity with `api.import("Object")`,
  the expected canonical exports, and reverse exported-name lookup.
- Use the Rust floor's own `dlopen`/`dlsym` entries to reopen and inspect the
  selected id dylib and its diagnostic identity symbol. Retain the original
  handle and do not call `id_init` again.

**Gate:** the Rust floor loads a Zig `module_init` fixture entirely through its
own `IdApi` loader entries and can reflect and reopen its own dylib without
creating a second runtime.

#### R3.1 execution record — 2026-07-22

The runtime now owns an ordered export environment independent of hash
iteration. Exporting a new Symbol appends one entry; rebinding the same name
replaces its value without moving it. Import follows the Zig floor by interning
the queried bytes before lookup. Reverse lookup scans current entries from the
front by pointer identity, so the first current alias wins. Consequently, an
earlier name rebound to another object becomes that object's reverse authority,
while the old object's next surviving alias becomes authoritative. A null-valued
entry imports as null and `isExported` reports false, matching current behavior.

Initialization publishes the unchanged R2.5 objects under the 14 current names
in order: `Object`, `Integer`, `Float`, `Word`, `String`, `Boolean`, `Vtable`,
`Symbol`, `SymbolVt`, `ObjectVt`, `IntegerVt`, `FloatVt`, `WordVt`, and
`StringVt`. Their prototype/family pointers are not recreated. `import`,
`exportObj`, `isExported`, and `exportedNameOf` now use the ordered environment.
`dlopen` and `dlsym` are transparent platform-ABI pass-throughs, including
caller-selected lazy mode and null results for load/resolution failures.

`test-rust-env-loader` validates every canonical import and reverse name before
mutation, confirms the R2.5 11-vtable snapshot, and freezes alias, rebinding,
and null-value behavior. It builds a tiny Zig module, loads it twice exclusively
through the Rust table, and threads the same opaque state pointer through the
second `module_init`, where its value advances from one to two. The client also
reopens the Rust floor through its own loader, resolves and calls the diagnostic
identity, resolves `id_init`, retains the handle, and deliberately does not call
initialization again. Debug and optimized fixtures, all 37 Rust tests, strict
all-target/all-feature Clippy, and combined `zig build test` pass.

### R3.2 — Integer, Float, and Word

- Complete the already-created families' C-layout payload behavior,
  constructors, predicates, and accessors without replacing their R2.5 vtables
  or prototypes.
- Preserve the historical `SmallInt` API names while keeping Integer boxed.
- Test signed full-width boundaries, f64 bit patterns used by current tests, and
  every Word bit pattern used by Alien pointer/unsigned paths.
- Extend the R2.2a ReleaseSafe matrix to `isSmallInt`, `isFloat`, and `isWord`
  over several heap populations and compare with the repaired Zig baseline.

**Gate:** the backend-neutral scalar conformance group passes against both
floors, canonical identities are unchanged, and scalar predicates remain
expected O(1) within Design 46's threshold.

#### R3.2 execution record — 2026-07-22

The historical SmallInt API now constructs ordinary boxed i64 objects in the
existing Integer family; Float and Word likewise use their R2.5 f64/u64 shapes.
Constructors allocate collectable blocks without replacing the canonical
prototypes or any family vtable. Accessors preserve all payload bits. The three
predicates perform exact heap membership before reading the header, then compare
the copied family identity; null, Method/code, aligned foreign, singleton, and
cross-family carriers return false without an unsafe dereference.

`test-scalar-conformance` is one frozen `IdApi` client run in separate processes
against the installed Zig floor and debug/optimized Rust floors. It covers i64
minimum through maximum, positive/negative zero, 1.25, infinity, a quiet-NaN
payload, and Word values spanning zero, signed-range top, high-bit-set Alien
pointer/unsigned cases, and u64 maximum. Every value round-trips, carries the
expected unchanged vtable, rejects the other two predicates, and leaves the
published prototype/owner identity intact.

The optimized membership fixture now reports ten rows, adding `isSmallInt`,
`isFloat`, and `isWord` at 1,029, 16,384, and 131,072 vtables (5,125 through
265,211 live objects). Every row remains slope-gated at 0.35; scalar rows also
use the R0.5 Zig per-population maximum plus 10%. Three post-commit runs are
recorded in `bench/baselines/rust-id-r3.2-scalar-membership.json`. Median scalar
costs span 1.478-2.340 ns/call; median exponents are 0.0756, 0.0870, and 0.0667.
The largest exponent anywhere in the ten-row matrix was 0.1377. All 37 Rust
tests, strict all-target/all-feature Clippy, both-floor scalar conformance, and
combined `zig build test` pass.

### R3.3 — String ownership

- Complete copied byte ownership and stable `(ptr,len)` access on the R2.5
  String family without replacing its vtable or canonical empty prototype.
- Register the String finalizer metadata in the R3.0 shell even though sweeping
  is not yet active.
- Preserve embedded NUL and non-UTF-8 bytes; do not model a Birnam String as Rust
  `str`.
- Test empty, binary, embedded-NUL, and large strings from Zig callers.

**Gate:** byte-for-byte string tests pass without UTF-8 assumptions or allocator
ownership crossing the ABI.

#### R3.3 execution record — 2026-07-22

The Rust floor now copies every public String input into a Rust-owned
`Box<[u8]>` indexed by the existing String object's exact address. Empty input
accepts a null pointer without constructing a slice from it; non-empty null
input is fatal. Byte access validates exact String membership, returns the
stable boxed allocation pointer and length, and never creates a Rust `str` or
otherwise interprets the bytes as UTF-8. The canonical empty String and every
new String retain their R2.5 family, prototype, and owner identities.

Initialization attaches the real String cleanup callback to the canonical
prototype, and construction attaches the same callback to every new String.
The callback releases the matching boxed bytes through the owning runtime.
This is retained metadata only: R3.0 collection remains disabled, so no
finalizer is invoked and no String is reclaimed yet. The callback's activation
and exactly-once rule remain R4 work.

`test-string-conformance` is one frozen `IdApi` client run in separate processes
against the installed Zig floor and debug/optimized Rust floors. It proves an
empty String remains distinct from the canonical prototype, copies embedded
NUL and non-UTF-8 bytes, gives two equal inputs independent ownership, preserves
an observed byte pointer across 4,096 unrelated String allocations, and copies
a patterned 1 MiB input before the caller overwrites its buffer. All 37 Rust
tests, strict all-target/all-feature Clippy, both-floor String conformance, the
evolving unsupported-field spike gate, and combined `zig build test` pass.

### R3.4 — nil, Boolean, and prototypes

- Complete nil and Boolean behavior on the canonical objects and family vtables
  allocated in R2.5; do not construct replacement singletons.
- Complete `proto`, `alloc`, clone/new floor protocol, ownership, private-vtable
  splitting, and parent mutation on the existing canonical prototypes.
- Preserve String's deep-copy clone/new behavior so cloned Strings do not share
  or double-free their owned byte buffer.
- Verify the R3.1 export names and family/vtable ownership without rebinding them
  to newly allocated prototypes.

**Gate:** prototype, family extension, singleton specialization, and inherited
method tests pass identically under both floors, and every canonical object and
family pointer still equals the R2.5 identity snapshot.

#### R3.4 execution record — 2026-07-22

The Rust bootstrap now installs `clone`, `new`, and `==` in the Zig floor's
direct-selector order immediately after `doesNotUnderstand:`. `clone` and its
`new` alias make a collectable shallow copy of the receiver's complete padded
payload with the same current vtable and no copied finalizer metadata. Identity
answers the existing canonical true or false object by pointer comparison.
String overrides both creation selectors, copies its arbitrary bytes into new
Rust-owned storage, returns to the canonical String family just like Zig, and
attaches fresh finalizer metadata so no allocation is shared or double-owned.

`ensurePrivateVtable` now validates exact floor membership before access. Raw
vtable objects remain their own method-install targets; a family prototype or
singleton that already owns its family returns that family unchanged; any other
object receives one permanent child vtable delegating to its former family and
owning only that object. Repeated calls are idempotent. This preserves the
intentional Boolean rule: true owns the shared Boolean family, while specializing
false splits only false. Existing `proto`, `alloc`, `setOwner`, and `setParent`
operations continue to mutate the ordered vtable authority without replacing a
canonical prototype, singleton, family, or export.

`test-prototype-conformance` runs one frozen `IdApi` client against the installed
Zig floor and debug/optimized Rust floors. It checks direct-selector order,
payload-copy extent and bytes, `clone`/`new`, String storage independence,
identity on ordinary/nil/Boolean receivers, prototype and raw-vtable extension,
ordinary instance isolation, false-only Boolean override, parent and owner
mutation, and all 14 canonical exports before and after mutation. All 39 Rust
tests, strict all-target/all-feature Clippy, the evolving unsupported-field
spike gate, all earlier cross-floor gates, and combined `zig build test` pass.

### R3.5 — MOP enumeration and mutation hook

- Register every created vtable in deterministic creation order.
- Implement vtable count/index, direct selector index, parent, and owner queries.
- Preserve the complete Design 42 consistency law: dictionary canonical; Store
  mirror exactly tracked, honest `#primitive` presence, or absent; never stale
  or falsely present; Store arbitration for supported post-bootstrap mutation;
  and raw floor mutation bootstrap/private only.
- Invoke the mutation hook before direct replacement or removal and without
  retaining Rust borrows. Keep it pure: no send or allocation; Store alone may
  mutate the fact log.
- For tracked addition prove dictionary mutation → assert. For tracked
  replacement prove retract → dictionary mutation → assert. For tracked removal
  prove retract → dictionary removal. For raw replacement and removal prove
  hook-driven retraction followed by mutation with no replacement mirror
  assertion; for raw addition prove that the mirror stays absent.
- Prove that direct removal reveals inheritance without a copied parent binding
  or tombstone, and that replacement/removal invalidates stale `#primitive`
  classification.
- Prove init/image backfill marks every direct selector on real type vtables with
  `#primitive` when no exact tracked fact exists, excludes internal dictionaries,
  is idempotent, and restores the real-type mirror after image boot. A raw addition
  after backfill remains absent until a tracked install or later backfill.
- Prove Store's mutation authorizer runs before any tracked fact/dictionary
  change and that rejection preserves both exactly. Prove its observer runs once
  only after the dictionary and mirror contain their completed post-mutation
  state; raw/private mutation does not notify it.
- Test a hook that re-enters safe read-only floor operations and verify the
  current prohibition on allocation/send from the hook remains documented.

**Gate:** the addition/replacement/removal matrix passes through both
Store-arbitrated and raw/private paths, observes the exact ordering above, and
never leaves a stale method or `#primitive` fact under the Rust floor. Init and
post-image backfill, authorizer rejection, and post-commit observer ordering all
pass without widening `IdApi`.

#### R3.5 execution record — 2026-07-22

The Rust runtime now stores the optional method-overwrite hook and invokes it
only when a direct binding already exists. Replacement and removal first copy
the callback and its raw arguments out of runtime state, end every Rust borrow,
invoke the foreign hook, and only then reacquire state to mutate the ordered
dictionary. Addition and missing removal remain hook-free; clearing the hook is
immediate. A Rust test re-enters vtable count/index, direct-selector, parent,
owner, and method lookup reads from the callback and proves the old binding is
still authoritative during both replacement and removal. Send and allocation
remain prohibited by the documented hook contract.

Every vtable remains registered in creation order. The two internal dictionary
vtables now also expose the symbol interner's ordered keys and the export
environment's ordered key/value pairs in their raw C-layout views, matching the
Zig floor. Symbols and the environment remain the Rust-side identity/order
authorities; synchronization changes neither their lookup rules nor any object
identity. This closes a discrepancy found by the Store backfill test: populated
internal dictionaries are now observable and explicitly excluded rather than
being accidentally empty.

The existing 17-test Store suite passes unchanged against `rust-id-v1` in both
debug and optimized dylibs. It covers tracked add/replace/remove ordering, raw
add/replace/remove behavior, exact inheritance recovery, primitive-marker
invalidation and idempotent backfill, authorizer rejection, completed-state
observer notification, and internal-dictionary exclusion. A separate
process-isolated fixture performs the real `imageLoadBegin`/`imageLoadEnd:raw:`
Store reset and proves `backfillMirror` reconstructs primitive presence under
both Rust profiles and the default Zig floor. The selectable identity is test
diagnostic only: ordinary installation still selects Zig.

All 40 Rust tests, strict all-target/all-feature Clippy, both Rust Store profiles,
the image-reset fixture, all prior cross-floor gates, and combined
`zig build test` pass. `setMethodOverwriteHook` was the final fatal non-GC table
slot, so the evolving R2.6 unsupported-field child gate is retired; collection
remains the deliberately inert R3.0 shell until R4.

### R3.6 — First downstream rung matrix

- Add a test-floor adapter capable of loading a selected dylib, asserting a
  diagnostic floor-identity symbol, and retaining its handle for the test
  process lifetime.
- Replace direct `id.zig` imports in a small first set of external tests with the
  adapter; do not delete Zig-floor private tests.
- Give each floor an independent install prefix, build/cache directory, stage1
  output, temporary image area, and process environment. Neither branch may
  write the other's `zig-out/lib` or shared module set.
- Run Store, PEG, language, prelude, and an explicitly named non-GC collection
  subset first with the Zig floor and then with the Rust floor. Split GC/layout
  collection cases into a separately named group; do not skip them conditionally
  based on the selected floor.
- Allow stack-bottom registration to be recorded by R3.0, while collection
  remains inert until R4.

**Gate:** both backends pass the selected lower-rung non-GC tests from clean processes;
every process asserts the intended floor identity; independent outputs prove the
two runs cannot overwrite each other; the normal `test` and installed runtime
still default to Zig. The deferred GC/layout group is visibly assigned to R4,
not reported as passing or silently skipped.

#### R3.6 execution record — 2026-07-22

The existing language-neutral test-floor adapter now selects either
`zig-id-v1` or the Rust dylib's new `rust-id-v1` diagnostic identity before
calling `id_init` once and retaining the handle for the process. Store, PEG,
language, prelude, and collection tests all already imported this adapter rather
than `id.zig`; the Zig floor's private tests remain unchanged. Publishing the
Rust identity does not install or implicitly select Rust: the ordinary prefix
still contains the Zig floor.

Collection coverage is now split by capability instead of conditionals.
`test-collection` contains 19 non-GC behavior tests. The two tests that require
precise tracing, reclamation, and live-graph survival are in the explicitly
named `test-collection-gc` group, remain part of the normal Zig gate, and are
assigned to R4 for Rust. Stack-bottom registration stays real in the non-GC
group while the R3.0 Rust collector remains inert. No test examines the selected
identity to decide whether to skip.

`test-rust-lower-rungs` creates fresh Zig-floor and Rust-floor roots. They have
different install prefixes, local and global Zig caches, stage-1 output trees,
and temporary roots; Rust also uses a fresh Cargo target. Each Rust profile is
copied under its isolated prefix and selected by absolute path. The matrix runs
59 tests per target: Store 17, image-reset MOP backfill 1, PEG 6, language 6,
prelude 10, and non-GC collection 19. `zig-id-v1`, Rust debug, and Rust release
all pass in separate processes. The harness prints each selected dylib digest,
asserts the output trees are distinct and complete, and checksum-guards the
ordinary installed Zig floor against mutation.

The isolated matrix is wired into combined `zig build test`. Strict Rust Clippy,
all prior cross-floor gates, the default Zig-only tracing/sweeping collection
group, and the post-change combined gate pass.

### R3.7 — Prove ID-floor artifact preflight

- Run the Design 54 pointer-free artifact inspector through the selected Rust
  floor before Store, Codegen, Module, or any artifact initializer is available.
- Reject wrong descriptor kind, dependency identity, target/ABI, native
  requirement, and dependency closure at this preflight boundary. Validate a
  root identity only when the invoking operation supplies an expected root
  identity; a path-only root launch does not invent one from its filename.
- Exercise rejection in a harness where Store and Codegen cannot be resolved and
  the candidate initializer is a sentinel that records or aborts if called. A
  rejected artifact must not run the sentinel.
- Preserve the successful path as pointer-free inspection followed by explicit
  later activation; inspection must not acquire executable authority merely
  because the Rust implementation can load a dylib.

**Gate:** every incompatible fixture is rejected by `libid` plus the artifact
inspector alone, before higher rungs or initialization, while one compatible
descriptor reaches (but does not execute) the later activation boundary.

#### R3.7 execution record — 2026-07-22

The siphon's closure policy is now isolated in `artifact_preflight.zig`. It
consumes only `IdApi` values returned by the pointer-free inspector. A path-only
root calls `walkRoot` without an expected identity, so the host does not derive
authority from a filename. Operations that already possess an independently
selected identity pass it explicitly. Every dependency edge still verifies
that the descriptor identity equals the required Symbol before recursion.

Dedicated selected-floor tests construct descriptors for wrong ABI, wrong
target, unavailable native requirements, wrong kind, explicit root-identity
mismatch, dependency-identity mismatch, and an incomplete closure. The
compatible fixture reaches the post-preflight activation seam while `Store`,
`Codegen`, and `Module` remain unresolvable. The activation-authority test uses
a real dylib whose `module_init` and `__birnam_unit_init` exports abort; the
inspector reads its appended descriptor and completes preflight without loading
the container or invoking either hook.

Inspector-only tests live in a dedicated test root, rather than the production
inspector module, so importing the inspector from higher-rung tests cannot
silently rerun a fresh-floor assumption after those rungs are active. The
isolated lower-rung matrix now runs 70 tests per target, including the four
inspector cases and seven closure/authority cases. Zig, Rust debug, and Rust
release pass in separate processes. Strict Rust formatting and Clippy plus the
complete combined `zig build test` gate pass.

## 10. Phase R4 — Collector, layouts, and image primitives

**Goal:** complete the risky memory-management contract before broad runtime
verification.

### R4.1 — Manual marking and block tracing

- Implement exact-address candidate lookup in the block registry.
- Implement mark worklist behavior and permanent-object exclusion.
- Implement conservative payload tracing before precise layouts are enabled.
- Root the export-environment values and symbol-interner entries explicitly.
  Preserve the current treatment of permanent type vtables: their protocol keys
  are permanent Symbols, their values are raw Method pointers, and the registry
  does not keep a collectable private-vtable owner alive merely because its
  permanent vtable records that owner.
- Test object graphs, cycles, false-positive raw words, and permanent roots.

**Gate:** pure Rust heap graph tests pass without scanning the native stack.

#### R4.1 execution record — 2026-07-22

The Rust heap registry now marks only an exact registered collectable payload
address. Nulls, foreign pointers, allocation headers, interior addresses,
permanent blocks, and already-marked blocks never enter the worklist. The marker
copies each padded payload into machine-word candidates before mutating the
registry, then drains the transitive conservative closure. Tests cover cycles,
duplicate discovery, false pointer-shaped words, and mark clearing in debug and
optimized Rust builds.

The process export environment snapshots its values as roots, and the symbol
interner explicitly contributes every interned identity. Symbols, vtables, and
singletons remain permanent and therefore do not carry mark bits or enter the
worklist. Permanent vtable payloads are not traced: in particular, a private
vtable's owner field does not keep its collectable owner alive. A runtime-level
test proves exported objects and their payload children are marked while an
unexported object and an otherwise-unreachable private-vtable owner are not.

This checkpoint activates marking only. The public collection call performs the
floor-root mark pass, clears private marks, and still returns zero; it neither
invokes foreign roots nor scans the native stack nor sweeps. The existing
debug/release external metadata and fatal-capacity probes therefore continue to
pass while R4.2 through R4.5 add the remaining authority in order.

### R4.2 — Root providers and trace hooks

- Activate the bounded root-provider registry populated by the R3.0 shell with
  the current capacity and failure behavior.
- Invoke providers with the Rust mark callback and no live runtime borrow.
- Restrict the mark callback to the dynamic extent of the provider/hook call;
  test that the supported fixture uses it synchronously and document retaining
  it as a contract violation.
- Implement per-vtable trace hooks after payload scanning.
- Add Zig fixture providers/hooks that hold Oops outside the Rust heap.
- Test reachability controlled independently by global providers and reachable
  instances with trace hooks.

**Gate:** foreign provider and hook tests survive forced manual collection.

#### R4.2 execution record — 2026-07-22

The retained 32-entry provider registry now snapshots its function pointers
before invocation. Each provider receives a dynamically scoped C mark function;
no `RuntimeState`, heap, marker, registry, or vtable reference remains live
across the foreign call. The same boundary invokes a copied trace hook only
after the reachable instance's conservative payload has been scanned. Marks
added by either callback rejoin the one worklist and are drained transitively.

The mark function is installed in owner-thread TLS with raw pointers to the live
heap and marker for exactly one synchronous callback. Its guard is cleared even
when the callback unwinds through Rust. Calling a retained function after the
provider returns is a contract violation and terminates with `GC mark callback
escaped its invocation`; attempting nested collector callback authority also
terminates. This makes the temporal lifetime executable rather than relying on
an undocumented Rust borrow assumption.

Rust graph tests prove a provider-rooted object fires its trace hook, that the
hook's external child becomes reachable, and that an unreachable instance of
the same vtable does not fire. The external Zig fixture repeats this in debug
and release, reenters read-only `IdApi` queries from both callbacks, exercises
the maximum provider count, and verifies the escaped-callback abort in a child
process. Finalizers and sweeping remain inactive. Strict Rust checks, optimized
tests, and the complete combined `zig build test` gate pass.

### R4.3 — Precise heap layouts

- Activate borrowed static layout descriptors retained by the R3.0 shell
  without taking ownership.
- Implement prefix and rest slot kinds.
- Trace only `slot_oop` under precise mode and ignore raw/code slots.
- Implement `slotKind` and unknown-layout behavior for the image layer.

**Gate:** the same synthetic layouts produce the same retained and reclaimable
sets under both floors. R4.5 verifies that the Rust reclaimable set is physically
deallocated; R4.3 must not introduce sweep before stack roots and finalizer
ordering are ready.

#### R4.3 execution record — 2026-07-22

Precise mode now consults the copied instance vtable after a block leaves the
worklist. When that vtable has a borrowed static descriptor, only prefix or rest
slots whose kind is exactly `slot_oop` are submitted as candidates. Pointer bits
in `slot_raw`, `slot_code`, or an unknown kind are ignored. A vtable with no
layout remains conservative even while precise mode is enabled. The existing
`slotKind` boundary reports the identical prefix/rest rule and
`slot_unknown` for an absent descriptor.

Rust graph tests cover mixed prefix kinds, an Oop rest slot, pointer-shaped raw
and code words, switching back to conservative mode, and the unknown-layout
fallback. A selected-floor Zig fixture makes the mark decision externally
observable with per-target trace hooks and a provider-rooted carrier. The same
two cases pass through the Zig floor, Rust debug, and Rust release; the isolated
matrix now contains 72 tests per target.

The original gate wording required physical reclamation before R4.5 had
implemented sweep. That dependency was impossible without either sweeping
before native roots exist or duplicating the sweep implementation. The gate now
requires identical retained/reclaimable decisions at R4.3 and reserves actual
Rust deallocation parity for R4.5. Strict Rust checks, optimized tests, and the
complete combined `zig build test` gate pass.

### R4.4 — Native stack and register scanning

- Bind the supported platform's `_setjmp` operation and reserve a register-save
  buffer with verified size/alignment.
- Scan between the local save buffer and the host-provided stack bottom using the
  current downward-growing arm64 macOS assumption.
- Treat only exact registered payload addresses as roots.
- Keep collection disabled when the stack bottom is absent.
- Exercise optimized generated C holding Oops in locals/registers during a forced
  collection, not only Rust-owned test variables.

**Gate:** root discovery passes in Debug and release builds and fails safely—not
by sweeping—when no stack anchor is installed.

#### R4.4 execution record — 2026-07-22

The Rust floor now binds Darwin arm64 `_setjmp` with a 192-byte, 16-byte-aligned
register-save buffer whose size is checked against the platform header. Before
marking any runtime, provider, or foreign-hook roots, collection saves the
current register state and word-scans from that save area through the
host-provided stack bottom under the existing downward-growing-stack contract.
Only exact registered collectable payload addresses become roots.

An absent, reversed, or implausibly distant stack anchor makes native-root
discovery fail closed. Public collection then returns without marking providers
or sweeping, preserving every allocation until the host installs a valid
anchor. The scan is unavailable on unsupported targets rather than silently
using a guessed context layout.

The external proof compiles an optimized C helper that allocates its object
internally, keeps the only live Oop pinned in arm64 register `x19`, and crosses
`gcCollect` without returning that identity to the Zig caller. Its trace hook
fires under both Rust debug and Rust release floors. The same probe with a zero
stack bottom confirms that no trace hook fires. Strict Rust checks, optimized
tests, the 72-test isolated three-floor matrix, and the complete combined
`zig build test` gate pass.

### R4.5 — Sweep and finalizers

- Collect dead keys before mutating the block registry.
- Invoke foreign finalizers registered through the R3.0 shell exactly once
  before deallocation and without a live registry borrow.
- Reclaim with the original allocation layout.
- Keep allocation, sweep, and finalizer ordering unspecified; parity tests compare
  reachability and exactly-once counts, not callback order.
- Clear marks and allocation-byte counters after each collection.
- Guard against reentrant collection and allocation from finalizers.

**Gate:** all original GC/finalizer/String reclamation tests pass under both
floors, including child-process tests for forbidden reentrancy.

#### R4.5 execution record — 2026-07-22

Sweep now snapshots unreachable collectable addresses before registry mutation,
then detaches each block while keeping its payload live. A copied finalizer runs
with no live Rust registry or runtime-state borrow, after which the detached
owner deallocates the block with its original size and alignment. Permanent and
marked blocks cannot enter this path. Marks and exact allocated-byte accounting
reset only after a successful collection.

Register capture was moved into a shallow frame outside the collector. Mark and
sweep execute below its saved start address, preventing stale collector locals
from self-retaining objects on a later debug collection. The precise collection
group passes repeatedly under Rust debug as well as Rust release.

A phase guard refuses recursive collection with a zero result and rejects every
id allocation path while a collection is active. Child-process fixtures prove a
recursive finalizer does not nest collection and that an allocating finalizer
terminates with the expected invariant diagnostic. Manual collection reclaims
all 2,000 baseline objects, String owners release their byte buffers, and all
1,000 finalizers run exactly once. Active collection is now included in the
74-test isolated Zig, Rust-debug, and Rust-release floor matrix; strict Rust
checks and the complete combined `zig build test` gate pass.

### R4.6 — Automatic collection

- Implement the current allocation threshold and enable/disable control.
- Trigger only with a valid stack bottom and outside an active collection.
- Stress object churn while bounding live count.
- Run the stress under release optimization long enough to cross many thresholds.

**Gate:** live-set bounds and finalizer counts match the Zig-floor behavior; no
threshold-dependent downstream test flakes.

#### R4.6 execution record — 2026-07-22

Every Rust floor allocation contributes its exact padded block size to the
collection counter. With automatic collection enabled, a valid stack anchor,
an idle collector, and at least 16 MiB allocated since the last successful
collection, the next allocation first runs `gcCollect`, matching the Zig
floor's trigger timing. Disabling automatic collection or omitting the stack
anchor suppresses the trigger without discarding the accumulated count.

The optimized baseline passes under both Rust debug and Rust release dylibs:
the manual phase moves from 42 to 2,042 and back to 42 with all 2,000 objects
reclaimed; the finalizer phase runs and reclaims exactly 1,000 objects; and
6,000 allocations with 4,096-byte payloads finish with a live-set delta of
about 1,912 rather than 6,000. The complete downstream gate passes with the
current threshold unchanged.

### R4.7 — Image introspection primitives

- Complete `objectPayloadSize`, `exportedNameOf`, layouts, code-slot reporting,
  vtable ownership, and every remaining API entry.
- Remove all fatal placeholder functions from the production table.
- Construct the production table from a typed implementation structure with no
  placeholder variant; non-null pointers alone are not proof of completion.
- Scan the final dylib/source for placeholder symbols and exercise every safely
  callable entry through the conformance suite.
- Run the complete frozen pre-rewrite native client for the first time against
  the Rust floor. It must exercise its fixed early/late fields and callbacks
  without a rebuilt client or a reduced subset mode.
- Run the collection GC/layout group deferred by R3.6 against both floors before
  claiming image readiness.
- Extend the selected-floor adapter to the image unit and CLI smoke paths before
  using their result as Rust-floor evidence; assert the floor identity and dylib
  digest inside those workers as in R3.6/R5.
- Save and load a representative object graph through the existing Zig image
  module using each floor as an initial same-floor smoke only. Cross-floor image
  compatibility is the R5 gate.

**Gate:** the frozen native client, full collection GC/layout group, image unit
tests, and image-save smoke pass under both floors, including mapped code and
`--rebuild`.

#### R4.7 execution record — 2026-07-22

The production Rust table now names no fatal gap and is constructed directly
from typed implementations for all 54 frozen fields. Introspection covers exact
padded payload sizes, reverse export names, absent and prefix/rest layout kinds
including code slots, private-vtable ownership, parent/owner mutation, and
bounded vtable and selector enumeration. Source and debug/release dylib scans
reject placeholder-like implementations, while the full external conformance
suite safely exercises every callable field in scope.

Selected-floor workers now hash the exact dylib before loading it and compare
the digest supplied by the isolated matrix, then verify the exported floor
identity before `id_init`. The CLI host applies the same optional digest and
identity checks, so image evidence cannot silently use a stale or substituted
floor. The floor-neutral null-argument child checks the shared invariant text
rather than a Zig-specific diagnostic prefix.

The isolated matrix now runs 111 tests under Zig, Rust debug, and Rust release.
It includes the complete IdApi conformance suite, active collection/layout
tests, and all 17 image tests: representative graph round trips, mapped code,
`--rebuild`, compiled-unit integrity, module closure preflight, Alien replay,
and block re-derivation. Separate CLI gates save a representative benchmark
image through both Rust profiles.

Finally, the committed pre-rewrite arm64 client and module execute unchanged
against each Rust profile. The committed pre-rewrite image then boots through
the Rust-selected CLI both by mapping its code and by rebuilding from recorded
source. All 48 optimized Rust tests, strict checks, and the complete combined
`zig build test` gate pass.

## 11. Phase R5 — Full dual-floor verification

**Goal:** make floor selection systematic and prove that the language above the
floor cannot distinguish implementations except through allowed performance and
diagnostic differences.

### Work

1. Generalize the test-floor adapter across every higher-rung Zig test that
   currently imports `id.zig` only to obtain `id_init`. Every selected-floor
   worker loads one floor, calls exported `id_init` once, and retains its handle
   until process exit.
2. Rewrite the one direct private `bind` assertion as an external behavior test
   or retain it solely in each floor's private suite.
3. Add named build steps:
   - `test-floor-zig`;
   - `test-floor-rust`;
   - `verify-floor-zig`;
   - `verify-floor-rust`.
4. Make both verification paths build all self-hosted modules from the same
   source but use independent install prefixes, Zig/Cargo caches, stage1 output
   trees, temporary image paths, and process environments. Each process must
   assert the diagnostic floor-identity symbol and selected dylib digest before
   executing tests.
5. Run REPL goldens, source fallback, project check/build/run/install/test, LSP,
   image save, isolate worker, Alien, wrapped-library load, and relocated install
   smoke under the Rust floor.
6. Run mixed-generation checks: artifacts compiled while using one floor must
   load and execute under the other.
7. Run the complete image compatibility matrix:
   - Zig save to Zig mapped boot and Rust mapped boot;
   - Rust save to Rust mapped boot and Zig mapped boot;
   - repeat all four paths with `--rebuild`;
   - boot the frozen pre-rewrite image under both floors;
   - compare durable facts, exported/canonical identities, behavior, slot
     relinking, post-boot seed MOP backfill, and deferred-object counts rather
     than only successful exit.
8. Compare language-visible errors by kind/message where specified; differences
   in Rust/Zig internal panic text are not compatibility promises.
9. Run repeated clean-build/verify cycles to detect global-state, dylib-lifetime,
   finalizer, and order dependence.

### Gate

- both full verification paths are green;
- floor identity and dylib digest prove every test used the requested floor;
- no Rust-floor process calls `id_init` more than once and no process loads both
  floor implementations;
- no test is skipped merely because the Rust floor is selected;
- compiled artifacts cross-load in both directions;
- every same-floor, cross-floor, rebuild, and frozen-image cell is green;
- there are no unexplained image, reflection-order, or live-count differences;
- the Zig floor remains the default.

### R5 execution record — 2026-07-22

Every selected-floor Zig test now reaches `id_init` through the shared adapter;
no higher-rung test imports the Zig implementation for a private bind assertion.
The named `test-floor-zig` and `test-floor-rust` paths run the same complete
256-test matrix from isolated prefixes and caches under Zig, Rust debug, and
Rust release. The named `verify-floor-zig` and `verify-floor-rust` paths add the
328-test self-hosted suite, REPL goldens, source fallback, project
check/build/run/install/test, LSP, image save, isolate, Alien/wrapped-library,
and relocated-install checks. Each selected process verifies both the floor
identity export and the exact dylib SHA-256 before `id_init`.

The mixed-generation artifact gate compiles the same self-described module once
with each floor. All four producer/consumer cells execute its behavior, proving
both cross-load directions as well as the two controls. The image gate likewise
saves once with each floor. Its save reports match exactly: 298 durable facts,
193 source-recovered facts, no recovered slots, and no rejected facts or slots.
All eight current-image cells (four mapped and four `--rebuild`) preserve the
same durable application facts, exported `String` identity, heap-slot identity,
compiled behavior, re-derived closure, and seed `#clone -> #primitive` MOP
mirror. The committed pre-rewrite native client and image also pass mapped and
rebuilt under both floors.

The diagnostic gate compares specified language-visible kinds and messages for
`#doesNotUnderstand`, `#zeroDivide`, `#mustBeBoolean`, and `#notASymbol`, plus
the located syntax-error report; both floors are byte-for-byte equal. Finally,
two clean full-verification cycles pass in alternating Zig/Rust then Rust/Zig
order, with new isolated build trees and verified digests on every run. No test
is skipped for Rust, no process initializes or loads both floors, and the
installed/default floor remains Zig.

## 12. Phase R6 — Performance, robustness, and release readiness

**Goal:** prove the replacement is not merely functionally correct.

### Performance

- Add explicit `--floor=zig|rust` selection to the benchmark driver. Give each
  run an isolated prefix and result name containing floor kind, commit, and
  selected dylib digest so same-commit runs cannot overwrite one another.
- Make the benchmark process assert floor identity before recording results.
- Run the current Design 46 ReleaseSafe suite from clean processes for both
  floors on the same machine and quiet-power conditions.
- Use the existing thresholds: 10% for micro cases and 5% for macro cases.
- Pay particular attention to unary/keyword send, block invocation, allocation,
  String, collection, relational, parse, compile, image, startup, and docgen.
- Treat a threshold failure as investigation work, not as a reason to widen the
  comparator or silently retire a case.
- Record dylib size, startup time, peak RSS, live-object count, and GC pause
  behavior even where they are not current benchmark cases.

### Robustness

- Stress deep delegation, dictionary growth/rebinding, interner churn, repeated
  module loads, forced collections, finalizers, Alien callbacks, image cycles,
  and isolate process churn.
- Run pure Rust pointer/data-structure tests under Miri where supported; do not
  claim Miri covers foreign stack scanning or dynamic callbacks.
- Use available native memory diagnostics on cross-language smoke tests; retain
  deterministic child-process regressions for every bug found.
- Inspect the Rust dylib's dynamic dependencies and install name. It may depend
  only on allowed system libraries and must not retain build-tree paths.
- Confirm abort-on-panic policy in every runnable cdylib profile and inspect all
  exported symbols.

### Gate

The Rust floor is release-ready when functional gates pass and Design 46 reports
no unaccepted regression. Any accepted performance tradeoff requires a separate
record naming the case, measured cost, user benefit, and follow-up.

### R6 execution record — complete, 2026-07-23

The non-timing release checks are now executable gates. Debug and release
cdylibs use `panic=abort`, export only the four specified production symbols,
carry `@rpath/libbirnam_id.dylib` as their install name, depend only on
`/usr/lib/libSystem.B.dylib`, and retain no repository path. Darwin allocator
diagnostics run the cross-language provider, trace-hook, sweep, finalizer, and
fatal-guard fixture with fresh/freed storage scribbling, guarded large
allocations, and periodic heap checks. The shared Alien suite also performs 32
callback create, C-invoke, release, and forced-collection cycles under either
floor. Existing stress gates cover 4,096-deep delegation, 4,096-entry ordered
dictionaries and export/symbol registries, 1,000 repeated module loads, 16
mapped plus 16 rebuilt image boots, 32 isolate lifecycles, and 256 repeated
finalizer/collection cycles per Rust profile.

The complete 51-test pure-Rust suite passes Miri on `rustc 1.97.0-nightly
(8b03437a8 2026-05-12)`. Miri found and drove fixes for two real unsafe-code
defects: allocator provenance had been discarded before teardown and marked
object traversal, and a field-derived raw heap pointer survived a later unique
borrow of the complete runtime state. Large registry stress counts reduce to
128 only under `cfg(miri)` so the same invariants complete in useful time; the
ordinary suite retains 4,096. This evidence covers Rust allocation, pointer,
registry, dispatch, and callback-state code. It does not cover native stack
scanning, generated C, dynamic-loader behavior, or actual foreign callbacks;
the cross-language and Darwin allocator gates cover those separately.

The selected-floor live-count probe is byte-for-byte equal. Manual collection
reports 42 -> 2,042 -> 42 and reclaims 2,000 objects. The finalizer case reports
42 -> 1,042 -> 42, reclaims 1,000 objects, and calls exactly 1,000 finalizers.
After 6,000 automatic-GC allocations with 4 KiB payloads, both floors report a
live delta of 1,911.

The latest complete diagnostic benchmark pair at `50ef02e` records a 307,824
byte Zig dylib and a 450,672 byte Rust dylib, along with startup, peak RSS, GC
pause, and the full Design 46 suite. It is not acceptance evidence: its
comparator reports nine regressions, and subsequent diagnosis found redundant
owner checks inside an already owner-proven active send plus severe external
load during a follow-up run. The active-dispatch fix preserves the public owner
check and the metacircular `lookup` send; three focused fresh processes then
converged at 1.42-1.43 us unary sends, 1.57-1.64 us three-keyword sends,
1.56-1.59 us DNU sends, 69-71 ns block sends, and 105-110 ns map hits. A new
clean paired release run is still required after the host is quiet.

Full `zig build verify` passed at `3d4f10a` after those robustness changes. A
subsequent bounded dispatch change at `1435279` also reuses the already-active
runtime for nested public `IdApi.send` calls. Calls outside an active dynamic
extent still enter the runtime and enforce owner-thread authority, while nested
calls retain the same metacircular method lookup and DNU paths without repeating
the owner check and TLS replacement. The change passes all 51 Rust tests, strict
Clippy, full Miri, the cross-language reentrant-dispatch probe in debug and
release, and the 256-test selected Rust-floor lower-rung matrix in both profiles.
Its performance effect remains unclaimed until the guarded clean pair completes.
The complete `zig build verify` gate passed again at `7666f87`, after the
wrapper-level regression and release pre-build host guard landed. At `5e9d171`,
active-runtime reuse moved into `Runtime::enter` itself: a fresh public send now
performs one TLS lookup and one owner check, while a nested send performs one TLS
lookup and reuses the previously proved authority without replacing TLS state.
The foreign reentry fixture now initializes and exercises the actual
process-lifetime public runtime rather than a private dispatch harness. All 52
Rust tests, strict Clippy, full Miri, the debug/release foreign reentry probe,
and both 256-test selected-floor profiles pass. Its timing effect remains
unclaimed until the guarded pair completes. The complete `zig build verify`
gate also passes at `e130f99` with this entry path. R6 remains open, and R7
must not begin until that pair passes or a separate tradeoff record is accepted.

The guarded driver was subsequently hardened without changing Design 46's
regression thresholds. Tracked runs require AC power, a three-sample host-idle
median, a hottest-process ceiling, and quiet measurement intervals; a loaded
measurement is discarded and retried. Micro, GC, and image medians use three
fresh processes, relational medians use five, and the short macro cases use
three repetitions. The driver waits up to 900 seconds for a quiet host instead
of failing immediately or accepting a contaminated sample.

The first complete guarded pair exposed six Rust-floor regressions. Two bounded
changes removed their measured causes without changing the ABI or MOP: scalar
payload reads no longer repeat runtime membership/owner lookup after their
typed public precondition, and GC scans payload words in place rather than
copying every marked payload. A subsequent pair cleared all six cases. The last
remaining micro failure, `string.split-native`, came from repeated public
allocation checks while Zig collection code was synchronously executing inside
an already owner-proven Rust runtime. Commit `c7a1e28` adds active allocation
helpers used only through the TLS guard, while retaining collection-phase
checks, automatic collection, canonical String vtables, and finalizer metadata.
All 54 Rust tests, strict Clippy, the Store consistency-law suite in both
profiles, and the complete isolated 256-test Zig/debug-Rust/release-Rust matrix
pass. In the next tracked pair, `string.split-native` changed from a prior Rust
median of 2.869 ms to 2.705 ms and matched Zig at +0.0%; the same-floor
comparison measured a 5.7% Rust improvement.

That pair is still not R6 acceptance evidence. Seven unrelated relational, GC,
and boot cases crossed their thresholds while the two floor runs experienced
different external host load. A second guarded attempt discarded contaminated
micro and docgen measurements and waited rather than accepting them, but
unrelated Go/Rust builds plus macOS PDF and Spotlight work kept the final Rust
docgen preflight below the idle threshold for the full 900-second budget. The
driver therefore emitted no partial Rust snapshot. R6 remains open pending one
complete guarded pair on a quiet host; R7 remains prohibited. The complete
`zig build verify` gate passes at `15d0824` with the active allocation path and
all of the benchmark-driver hardening in place.

The complete guarded rerun at source commit `2fe44d2` produced the exact
Zig/Rust result pair recorded by Design 61a. The unchanged Design 46 comparator
reports two threshold crossings, zero missing cases, and zero unusable cases:
`query.aggregate` is 1,475,835 -> 1,562,867 ns/op (+5.8%), and
`compile.stage1-stdlib` is 39,818 -> 44,564 us (+11.9%). The former adds 0.087
ms per benchmark operation; the latter adds 4.746 ms to a cold whole-process
compile. Every other case meets its existing threshold, including
`string.split-native` at +2.4%, while 26 cases improve beyond their threshold.

Design 61a accepts those two tradeoffs only for the exact result-file hashes,
names their user benefit and bounded follow-up, and makes the decision
machine-checkable through `mise run r6-acceptance`. The task leaves Design 46's
comparator and thresholds unchanged and fails on any changed snapshot,
additional regression, missing case, or unusable value. That acceptance gate
passes at `fb3652d`. R6 is complete, and R7 may begin.

## 13. Phase R7 — Build, install, and default cutover

**Goal:** make Rust the default floor without deleting the oracle in the same
change.

### R7.1 — Integrate Cargo into the Zig build graph

- Add an explicit Cargo build step with deterministic target/profile mapping and
  repository-local target directory. Invoke Cargo with `--locked --offline`
  after the declared toolchain is installed.
- Make installation of `libbirnam_id.dylib` depend on that step.
- Stop installing the Zig-built id dylib under the production name.
- Retain a separately named Zig oracle dylib for dual-floor tests and a
  documented developer `-Dfloor=zig` build path that reinstalls it under the
  production artifact name in an isolated prefix.
- Make `zig build`, `zig build test`, `zig build verify`, and stage1 express their
  dependencies without requiring a pre-existing Cargo artifact.

#### R7.1 execution record — complete, 2026-07-23

Commit `f7cf89a` integrates the Cargo floor into the Zig graph with a
repository-local target directory, `--locked --offline`, and deterministic
Debug -> Cargo debug / ReleaseSafe, ReleaseFast, and ReleaseSmall -> Cargo
release profile mapping. The selected floor is installed directly as
`libbirnam_id.dylib`; the Zig implementation is also installed as
`libbirnam_id_zig.dylib`, and `-Dfloor=zig` reinstalls it under the production
name in the requested prefix.

The first complete Rust-selected native graph exposed a shared-prefix race:
standalone dual-floor helpers nested an ordinary Zig build while selected-floor
tests were loading the production path. Commit `abc66d0` makes those helpers
consume the named oracle or isolated prefixes instead. The complete
Rust-selected graph then passed, including the 256-test Zig/debug-Rust/
release-Rust lower-rung matrix.

Commit `7b35b14` makes Rust the default and makes every intentionally Zig or
dual-floor helper select its floor explicitly. A default `zig build test`
passes, while default Rust and explicit `-Dfloor=zig` conformance both pass.
The fixture-free git-archive gate starts with no binary, dylib, image, compiled
unit, Cargo target, or compatibility fixture; it passes Cargo's 54 tests,
builds and boots the Rust production floor, boots the separately named Zig
oracle, and builds and boots a second Zig-production rollback prefix. The
relocated installed-tree smoke also passes against the Rust default. R7.1 is
complete; later subsections record the remaining R7 checkpoints.

### R7.2 — Update CI and task runner

- Install/pin both Rust and Zig in CI.
- Run Rust unit/ABI tests before full verification.
- Run the Rust-default full gate and a focused Zig-oracle conformance gate.
- Keep benchmark execution outside CI under Design 46's current policy.

#### R7.2 execution record — complete, 2026-07-23

Commit `1473b70` pins both toolchains, runs Rust checks/tests before the native
graph, probes the Rust floor and named Zig oracle, exercises the fixture-free
cold bootstrap, then runs focused `-Dfloor=zig` conformance and the Rust-default
`zig build verify`. `mise run zig-oracle-conformance` exposes the focused local
equivalent. Benchmarks remain outside CI.

### R7.3 — Update install and distribution

- Include the Rust floor and required source/license material in installed and
  release layouts.
- Exclude `abi/idapi-v1.json` from installed and release runtime layouts. It
  remains repository-side build/test evidence, not a runtime dependency or
  artifact-selection input.
- Update clean-checkout, isolated-prefix, relocated-tree, source-free-root, and
  release-tarball smokes.
- Confirm `BIRNAM_ID_DYLIB` can still select an explicit compatible floor for
  tests and embedding; do not present this as release rollback unless that
  alternate floor is actually packaged.
- Confirm missing-floor diagnostics retain their current public path behavior.

#### R7.3 execution record — complete, 2026-07-23

Commit `88c7dcb` installs the complete Rust crate source and Cargo metadata
needed for offline documentation, the Rust production
`libbirnam_id.dylib`, and the separately named `libbirnam_id_zig.dylib`.
Repository-only `abi/idapi-v1.json` remains absent. The relocated install smoke
compares installed native sources with the checkout and boots both the Rust
default and explicit Zig oracle. `zig build test-install` and `scripts/dist.sh`
pass; the signed release tarball contains both unambiguously named dynamic
libraries and no ABI manifest. The crate has no third-party dependencies, and
the repository currently has no license file to copy or invent.

### R7.4 — Make documentation implementation-honest

- Replace “Zig floor” wording with “native floor” where the statement spans the
  Rust id layer and Zig higher rungs.
- Extend native-source documentation extraction to include Rust `//!` and `///`
  comments, or provide an equivalent generated native-floor reference.
- Install the Rust floor sources needed by offline docgen.
- Update Designs 32, 40, 42, 44, 46, 54, guides, README, and operator-facing
  help only where their implementation-language claim changed.
- Do not rewrite historical documents that are explicitly archival.

#### R7.4 execution record — complete, 2026-07-23

Commit `4a814ea` extends the compiler-free native documentation scanner to Rust
`//!`/`///`, preserves docs across Rust attributes, gives Rust modules distinct
`rust-*` identities and a `#rustFloor` kind, and threads the installed Rust
source directory through the runtime resource and CLI boundary. Generated
reference pages group the Rust floor and Zig oracle separately. The 334-test
self-hosted suite and relocated `test-install` smoke pass, including a check for
the extracted `IdApi` documentation. Commit `99c2deb` updates current Designs
32, 40, 42, 43, 44, 46, and 54, the relevant guides, README, help, task
descriptions, source comments, and Rust API docs without rewriting archival
history.

### R7.5 — Confirm and record the default switch

- Make the Rust artifact the normal `libbirnam_id.dylib` in one focused commit.
- Consume the exact, executable R6 performance acceptance rather than creating
  a second benchmark policy during cutover.
- Run full verification after the CI, distribution, and documentation follow-up
  commits so the accepted R7 state, not only the switch commit, is covered.
- Record the cutover commit, the exact developer `-Dfloor=zig` recovery command,
  and the previous-complete-release reinstall path in the release notes.

Commit `7b35b14` is the focused default switch: ordinary builds install the Rust
artifact as `libbirnam_id.dylib`; `-Dfloor=zig` is explicit. Design 61a and
`mise run r6-acceptance` bind cutover to the exact accepted dual-floor results
and their two named tradeoffs.

The first final aggregate run exposed a pre-existing wall-clock race in the LSP
project fixture: it rewrote a dependency manifest after ten seconds even when
the server had not processed both `didOpen` notifications. Commit `3fc0a5f`
replaces that sleep with JSON-RPC response/diagnostic barriers. The focused
gate passed three direct repetitions and `zig build test-lsp-project`; the next
complete `zig build verify` passed. `mise run r6-acceptance` and
`mise run zig-oracle-conformance` also pass at this checkpoint.

The developer rollback remains
`zig build -Dfloor=zig --prefix <isolated-prefix>`. The previous complete
release path was exercised by downloading, extracting, and running the
published `v0.11.0` `birnam-0.11.0-aarch64-macos.tar.gz` bundle; it reports
`birnam 0.11.0`. R7 is complete.

**Gate:** a clean default build and ordinary execution select the Rust production
artifact, never the Zig implementation implicitly. The oracle remains separately
named and developers can explicitly install it under the production name in an
isolated prefix.

## 14. Phase R8 — Observation window and Zig-floor retirement

**Goal:** remove duplicate production code only after the replacement has had a
real stabilization interval.

Keep the Zig oracle for at least one release checkpoint or an equivalent agreed
observation window. During that interval:

- every Rust-floor bug gets a regression test that also states the intended Zig
  behavior;
- no new feature is implemented only in the Zig oracle;
- ABI additions, if unavoidable, update the canonical manifest explicitly, are
  appended in both definitions, and pass old-binary compatibility review;
- performance results continue to use the Rust default;
- rollback remains a build/configuration change rather than source reconstruction.

After the interval, retire in separate commits:

1. remove the Zig oracle from ordinary build/test matrices;
2. remove `src/id/id.zig` production implementation after preserving any uniquely
   useful rationale in current docs or Rust comments;
3. keep `idapi.zig` while Zig modules consume the ABI and continue verifying it
   against the canonical language-neutral manifest;
4. remove oracle-only scripts/options and update the design status to complete;
5. run full verification and distribution once more with no Zig floor available.

The final retirement gate also repeats the from-zero source bootstrap from a
checkout containing no Birnam binary, image, compiled module, or generated floor
artifact. The declared Zig and Rust toolchains may participate; the build may
not recover by downloading or selecting a prebuilt Birnam artifact. Temporarily
remove the frozen compatibility-fixture directory for this proof so it cannot
become an accidental bootstrap input.

Git history is the implementation archive. Do not keep a dead second floor in
the production tree indefinitely.

## 15. Test matrix

| Layer | Zig-floor parity phase | Rust-floor cutover requirement |
|---|---|---|
| ABI layout | Zig descriptor matches frozen manifest and old native fixture | Rust descriptor matches the same manifest; old native fixture loads |
| Floor unit behavior | Existing/private Zig tests | Equivalent Rust private tests plus shared external conformance |
| Initialization | Shared workers call `id_init` once; private Zig tests use internal setup or a cached API | One floor and one exported `id_init` call per process; no repeated-call reset contract |
| Raw `Oop` safety | Code pointers and arbitrary aligned non-objects return null/false from public predicates and `vtableOwner` | Same behavior through expected-O(1) membership; staged R2/R3 scaling gates pass without changing send |
| Call buffers | Null pointer accepted only for zero arguments | Generated-C and direct-C zero-argument sends pass |
| Store/MOP | Full tracked/raw mutation, exact/`#primitive`/absent mirror states, backfill, authorizer, observer, ordering, and inheritance matrix | Same tests through selected floor with dictionary canonical and no stale or falsely-present mirror |
| Floor reflectivity | Canonical exports and self-dylib inspection through `IdApi` | Same without a second initialization |
| Artifact preflight | ID floor plus inspector reject incompatible descriptors before activation | Same with Store/Codegen unavailable and sentinel initializer uncalled |
| PEG/language/codegen | Current unit suites | Same tests, same compiled source behavior |
| Collections/relational | Current unit suites | Same ordering, equality, GC reachability |
| Alien/TCC | Existing native tests | Callbacks, function pointers, dlopen, generated shims |
| Image | Frozen pre-rewrite image plus Zig save/map/rebuild | Full Zig/Rust 2x2 map and rebuild matrix plus frozen-image boot |
| Isolate | Unit + process smoke | Worker boot and copied-value round trip |
| Project/distribution | Full verify/install smoke | Clean, source-free, relocated Rust floor |
| Performance | Floor-stamped Design 46 baseline | No unaccepted 10% micro / 5% macro regression; result includes dylib digest |

Tests should prefer external behavior over internal Rust shape. Private tests are
appropriate for allocation-header arithmetic, block-registry integrity,
reentrancy guards, and other invariants that cannot be observed safely after
violation.

## 16. Rollback and checkpoint policy

Before the focused R7.5 cutover commit, rollback was floor selection with Zig
still the default. After cutover and before R8 retirement there are two
explicitly different recovery policies:

- **Developer rollback:** `zig build -Dfloor=zig --prefix <isolated-prefix>`
  builds the oracle and installs it under the production name in that prefix.
  `BIRNAM_ID_DYLIB` may select the separately named oracle for tests, but only
  when that artifact is present.
- **Release rollback:** reinstall the previous complete release. Releases need
  not ship two production floors, and documentation must not imply that an
  unpackaged oracle can be selected in the field.
- Distribution automation records the default and never packages both floors
  ambiguously under the same name.
- The developer recovery path is exercised by the cold-bootstrap gate. The
  previous-release path is a distribution procedure and remains available
  independently of the source branch.

For either recovery path:

- no image migration is required because image and object contracts did not
  change;
- compiled Birnam artifacts remain usable because their C ABI did not change.

Do not combine the default switch, Zig deletion, ABI extension, object-layout
change, GC redesign, or stackless work in one commit. Each removes a different
rollback route.

## 17. Effort and sequencing

Expected focused effort for one developer familiar with the codebase:

| Phase | Estimate |
|---|---:|
| R0 — ABI/oracle freeze, frozen fixtures, baselines | 1–2 weeks |
| R1 — crate/ABI shell | 1–2 weeks |
| R2 — heap/vtable/dispatch proof | 1–2 weeks |
| R3 — metadata shell and non-GC parity | 2–3 weeks |
| R4 — GC/layout/image primitives | 2–3 weeks |
| R5 — isolated dual-floor and image matrix | 2–3 weeks |
| R6 — performance/robustness | 1–2 weeks |
| R7 — build/install/cutover | 1–2 weeks |
| R8 — retirement | after observation window |

Two to three focused months is an optimistic estimate for a parity prototype.
A hardened default is more credibly three to five focused months, excluding the
R8 observation window. The public raw-`Oop` contract, conservative collector,
test-seam conversion, cross-floor image matrix, and performance investigation
may dominate; phase gates take precedence over either estimate.

## 18. Risks and responses

| Risk | Response |
|---|---|
| `IdApi` layout drift | canonical committed manifest, versioned fingerprint, frozen old-binary fixture, append-only review |
| ABI manifest becomes a second runtime authority | build/test-only policy, install/dist exclusion, runtime smoke without the JSON file |
| rewrite introduces a prebuilt Birnam dependency | clean source-bootstrap gates after toolchain introduction and after Zig-floor retirement |
| Rust borrow held across reentry | central unsafe runtime rule, short borrows, reentrant fixtures |
| panic crosses FFI | abort every runnable cdylib profile and use child-process boundary tests |
| function/data pointer conversion bug | isolate conversion code and test real Zig/C methods |
| raw Method pointer treated as heap object | checked public predicates use exact registry membership; unchecked access remains private |
| checked predicates become O(heap size) | address-keyed membership index plus staged R0, R2, and R3 ReleaseSafe scaling gates |
| null zero-argument buffer becomes Rust slice | explicit null/count branch; generated-C and direct-C fixtures |
| allocator/layout mismatch | freeze exact base/header/payload formula and track original allocation layouts |
| GC misses optimized roots | force collection from optimized generated C, not only Rust tests |
| nondeterministic hash iteration leaks into observable order | ordered authority only for dictionaries, exports, symbols, and vtables; allocation/finalizer order remains unspecified |
| two floor runtimes create split identity universes | cdylib-only artifacts, dynamic fixtures, and exactly one initialized floor per process |
| a live floor or callback provider is unloaded | process-lifetime floor handle and module handles retained for every live function/layout pointer |
| a provider retains the collector mark callback | dynamic-extent contract plus synchronous foreign callback tests |
| finalizer/provider mutates active registry | copy callback target, release borrow, invoke, reacquire |
| Rust dylib gains nonportable dependency | zero-dependency start, inspect linked libraries in install smoke |
| test suite exercises Zig silently or races outputs | isolated prefixes/caches/images, floor identity plus dylib digest, named matrices |
| same-floor image tests hide incompatibility | full 2x2 save/map/rebuild matrix and frozen pre-rewrite image |
| artifact inspection accidentally activates code | ID-floor-only preflight with unavailable higher rungs and a sentinel initializer |
| placeholders pass non-null table check | typed implementation table, placeholder-symbol scan, callable-entry conformance |
| claimed rollback artifact is not shipped | distinguish tested developer rebuild from previous-release rollback |
| rewrite absorbs Design 60 | explicit out-of-scope rule and synchronous ABI parity gate |
| performance regression | existing benchmark thresholds before default cutover |

## 19. Final acceptance checklist

- [x] The committed ABI manifest, Zig descriptor, Rust descriptor, and frozen
      old-binary fixture agree on the complete `IdApi` contract.
- [x] The ABI manifest remains build/test-only and is absent from runtime,
      install, distribution, and artifact-selection paths.
- [x] A clean source checkout cold-builds with the declared Zig and Rust
      toolchains and no prebuilt Birnam artifact.
- [x] The frozen pre-rewrite image boots under both floors.
- [x] Public predicates safely reject Method/code pointers and arbitrary aligned
      non-object addresses through expected-O(1) exact membership, with the R2
      and R3 ReleaseSafe scaling gates green and no new membership lookup in
      ordinary dispatch.
- [x] Null argument buffers are accepted only for zero-argument sends.
- [x] Each test process loads one floor and calls `id_init` exactly once; fresh
      processes provide reset/isolation.
- [x] The Store/MOP addition, replacement, and removal matrix preserves
      dictionary authority, exact/`#primitive`/absent mirror states, exact fact
      ordering, seed/image backfill, authorizer rejection, observer ordering,
      inherited revelation, raw-path mirror absence, and marker invalidation.
- [x] The floor reflects canonical exports and reopens/inspects its own dylib
      through its `IdApi` loader without a second initialization.
- [x] ID-floor artifact preflight rejects kind, identity, target/ABI, native, and
      dependency mismatches before higher rungs or any initializer execute.
- [x] Rust private tests and shared floor conformance are green.
- [x] Every `IdApi` field is implemented; typed construction, symbol scan, and
      conformance prove no fatal placeholder remains.
- [x] Full higher-rung tests pass against both floors.
- [x] Zig and Rust runs use isolated outputs and assert floor identity plus
      selected dylib digest.
- [x] Existing compiled artifacts cross-load under both floors.
- [x] The complete Zig/Rust image save-map-rebuild matrix and relocated boot pass.
- [x] Alien callbacks, hooks, providers, finalizers, and module reloads pass.
- [x] Floor and provider library handles outlive every callback and layout
      pointer, and collector mark callbacks cannot escape their invocation.
- [x] Symbol interning preserves arbitrary non-NUL byte strings, including
      non-UTF-8 selectors, and `doesNotUnderstand:` preserves the 16-argument
      boundary.
- [x] Design 46 comparison reports no unaccepted regression; Design 61a records
      the two exact-snapshot tradeoffs.
- [x] Rust dylib dependencies and install name are distribution-safe.
- [x] Clean build, install, dist, source-free root, and isolate smoke pass.
- [x] Documentation says Rust where Rust is authoritative and native where mixed.
- [x] Default cutover is isolated from Zig retirement.
- [x] Developer Zig-floor rebuild and previous-release rollback are both tested
      and documented without assuming an unshipped oracle.
- [ ] One observation checkpoint completes before the Zig floor is deleted.
- [x] Cold bootstrap passes with frozen ABI/image fixtures unavailable and Cargo
      operating `--locked --offline`.
- [x] Design 61 records the complete cutover evidence; R8 retirement remains a
      later checkpoint.
