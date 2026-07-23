# 62 — Rust Native-Rung Migration Plan

> **Status:** Plan of record, 2026-07-23; stabilized after three adversarial
> passes. Approved to begin N0.
>
> This plan begins after [61 — Rust libid Floor Rewrite](61-rust-libid-rewrite-plan.md)
> reached R7. The Rust `libid` is the production floor; its Zig implementation
> remains a time-gated oracle under R8. This plan replaces the Zig implementation
> of the higher native rungs, cold compiler, and host with Rust where the work is
> irreducible mechanism, and with Birnam where behavior is language policy that
> can be deleted from the native layer without compromising a from-zero build.
>
> This is a plan of record, not authorization to retire the Design 61 oracle
> early. Every numbered migration checkpoint is a separate commit and leaves the
> selected production path green.

## 1. Outcome

The completed system has:

1. one Rust `libid` floor;
2. a sequence of dynamically loaded native rung dylibs implemented in Rust;
3. policy and derived protocol implemented in Birnam where it can be compiled
   from source by the independent cold compiler;
4. an independently bootstrappable Rust cold compiler and Rust siphon;
5. no Zig implementation in the production runtime, compiler, project
   workflow, or installed source bundle; and
6. no Zig toolchain requirement after the final verification and build-tool
   retirement phase.

The migration preserves the language, artifact, image, and object contracts. It
does not redesign them under cover of a language rewrite.

The present source inventory is about 22,596 non-test lines in `src/id/*.zig`,
including the 1,496-line Design 61 oracle and 364 lines of benchmark/sentinel
support. The largest single risk is `codegen.zig` at 4,678 lines. Raw line count
is not the progress metric: a native semantic implementation is complete only
when the corresponding Zig production implementation can be removed without
leaving a second authority.

## 2. Constitutional constraints

These are acceptance laws for every phase, not aspirations for the end.

### 2.1 From-zero bootstrap

A clean source checkout must build using only its declared non-Birnam
toolchains and source dependencies. It may not require a prebuilt Birnam
executable, image, compiled module, dylib, or generated Birnam artifact.

The implementation languages are not constitutional. Rust may replace Zig
because Birnam remains bootstrappable from another declared implementation
language. During the transition the declared toolchains are Rust, Zig, C, and
the platform linker; after toolchain retirement they are Rust, C, and the
platform linker.

### 2.2 Independent cold compiler

At every commit there is one complete non-Birnam compiler that can parse Birnam,
emit the current generated-C ABI, invoke libtcc, and produce a self-describing
compiled-unit dylib from source. The current Zig compiler remains that compiler
until the Rust replacement passes clean-build independence, deterministic
emission, frozen semantic parity, and full-corpus gates. A warm Birnam compiler
may be added later, but it is not a substitute for this proof.

### 2.3 One floor and dynamic rung boundaries

`libid` is the only floor. Store, PEG, the compiler, collections, images, and
the host are higher rungs even when implemented in Rust.

Every runtime module remains a dynamic library with its current load boundary
and `module_init` contract during parity migration. No phase may statically fold
a rung into `libid`, the siphon, or another rung. Birnam compiled units likewise
remain self-describing dylibs; loose source or a project/build manifest does not
become runtime authority.

Consolidating or changing rung boundaries requires a later design supported by
runtime evidence. It is not part of this rewrite.

### 2.4 ABI, object, and dispatch identity

- `IdApi` remains append-only and C-compatible.
- `Oop`, `Method`, payload layouts, slot kinds, vtable ownership, singleton
  identity, selector identity, and DNU behavior do not drift.
- Existing native clients and compiled units continue to load.
- The generated `birnam_*` C runtime symbols retain names, calling convention,
  argument order, and behavior.
- Reentrancy through sends, callbacks, finalizers, root providers, trace hooks,
  method-mutation hooks, libtcc, and `dlopen` is part of normal operation. Rust
  code may not hold an exclusive borrow or non-reentrant lock across such a
  call.
- No Rust panic or Zig error unwinds across a C ABI.

The committed language-neutral ABI manifest remains the source of truth.
Rust, temporary Zig adapters, generated C declarations, and fixtures are
projections checked against that manifest.

### 2.5 Object Canonical and the MOP consistency law

The live object dictionary is authoritative for dispatch. Store is a derived
MOP mirror and the supported post-bootstrap semantic mutation chokepoint.

For every direct `(vtable, selector)` binding, the mirror is exactly one of:

- the exact live Method for a tracked binding;
- `#primitive` for an unreified direct binding found by backfill; or
- absent.

With no direct binding it is absent. It never names a different method, and
`#primitive` never survives replacement or removal of the direct binding.
Tracked replacement remains:

```text
retract mirror → mutate dictionary → assert exact mirror
```

Tracked removal remains:

```text
retract mirror → remove direct dictionary entry
```

A raw bootstrap/private mutation may degrade the mirror only to honest absence.
The migration may not make facts authoritative over dispatch, bypass Store for
supported post-bootstrap mutation, copy inherited methods into direct slots, or
create a second Rust-side method registry.

### 2.6 Compiled-unit and image authority

- The pointer-free artifact descriptor remains the authority for identity,
  artifact ABI, target, roots, dependencies, capabilities, and digests.
- Complete dependency closure is inspected without mapping or initialization
  before any artifact is activated.
- `artifacts.toml` remains a disposable build receipt, never runtime authority.
- Compatible images run their captured compiled-unit dylibs as primary
  executable state.
- Image v4's one generation-authority record bound to one code unit, checksums,
  record ordering, swizzling, atomic publication, rejection-before-mutation,
  mapped execution, and explicit source recovery do not drift. The current
  runtime accepts only v4. Restoring v2/v3 support would be a separate
  compatibility change, not parity work.
- Old compiled dylibs remain loaded while any reachable code pointer may refer
  to them.

### 2.7 Execution model is unchanged

This plan preserves the current native C/Rust/Zig stack and conservative native
stack-root contract. Trampolining, continuations, stackless frames, and stack-map
changes belong to [60 — Stackless Native Execution](60-stackless-native-execution.md).
Mixing them into the language rewrite would destroy the parity oracle.

### 2.8 Determinism and behavior

Ordering visible through reflection, artifacts, project reports, diagnostics,
images, and generated C remains deterministic. Hash-map iteration order is not a
permitted substitute for existing ordered behavior. Existing command output,
exit status, error values, and JSON schemas remain compatible unless a separate
design explicitly changes them.

## 3. Destination map

“Rust” below means irreducible native mechanism. “Birnam” means policy or
derived protocol that is compiled from source by the retained cold compiler and
whose old native implementation is deleted. A parity port lands before an
optional policy extraction unless an already-shipped Birnam boundary exists.

| Current area | Destination | Boundary |
|---|---|---|
| `id.zig` | Delete after Design 61 R8 | Time-gated oracle; not owned by this plan |
| `idapi.zig` | Generated/test-only projection of `birnam-abi` | No independent ABI authority |
| artifact constants and preflight | language-neutral schema + Rust | Pointer-free parsing and closure validation stay below activation |
| artifact inspector | Rust dylib | Exact current object/message surface |
| Store | Rust dylib, existing Birnam MOP clients | Historied cells, facts, hooks, backfill, image bridge; dictionary remains canonical |
| PEG | Rust matcher dylib; Birnam construction policy only where measured safe | Hot backtracking stays native |
| language grammar/parser | Rust cold compiler | No Birnam artifact needed to parse the first Birnam source |
| libtcc wrapper/runtime shims | Rust dylib | Preserve the `birnam_*` symbols currently owned by TCC and every callback rule |
| code generator | Rust cold compiler dylib | Preserve generated C and compiled-unit descriptor semantics |
| primitive prelude | Rust dylib + existing Birnam derived protocol | Arithmetic, system calls, native loops stay native |
| collection structures | Rust dylib + existing Birnam collection protocol | RRB/HAMT/Range and measured hot scans stay native |
| error core | Rust dylib + existing Birnam error protocol | Error representation, propagation primitives, DNU swap, factory stay native |
| isolate substrate | Rust dylib + `isolate.birnam` policy | Process, pipe, and wire-v1 mechanism stay native |
| relational engine | Rust dylib; Birnam surface only behind performance gates | Indexes, join, aggregation, and per-row loops stay native |
| Alien/FFI | Rust dylib | `dlopen`, ABI marshalling, callback and native-memory ownership stay native |
| image mechanism | Rust dylib | Encoding, checksums, mapping, swizzling, quiescence, and atomic publication stay native |
| project resolution/build/check | Rust host + existing `project-policy.birnam` | Host keeps observation/mechanism; Birnam keeps provider selection, dependency closure/order/diagnostics, build reuse/dirty propagation, artifact-record shape, and publication authorization |
| runtime resources and siphon | Rust binary + existing Birnam CLI/REPL policy | One raw floor load, ordered dylib mounts, stack anchor, resource resolution |
| Zig-only benchmarks/sentinels | Rust fixtures | Move after their production subject so they remain independent during parity |
| `build.zig` and Zig tests | Rust build/test driver and behavior fixtures | Last, after all production replacements and observation gates |

Moving a body from Zig to Rust is not self-hosting progress. Moving policy to
Birnam counts only when the native policy is deleted, clean bootstrap still
works, and performance and behavior gates pass.

## 4. Repository and crate shape

The Cargo workspace grows by responsibility, not as one monolithic runtime
crate:

- `birnam-abi`: no-state C ABI types, checked accessors, manifest projection,
  and FFI failure policy;
- `birnam-artifact`: language-neutral descriptor constants and pointer-free
  parser;
- one `cdylib` crate per current native rung, preserving the installed dylib
  names and `module_init`;
- `birnam-compiler` support crates for grammar, generated-C emission, artifact
  metadata, and deterministic text generation, exposed through the existing
  compiler rung dylibs while the boundary remains frozen;
- `birnam-host`: the siphon and project workflow binary; and
- `birnam-xtask`: the final build, install, distribution, and verification
  driver.

Shared crates may contain mechanism but may not become hidden runtime
singletons. A rung owns its state, publishes it only through its current object
surface or explicit ABI, and remains replaceable as a dylib.

Every production `cdylib` uses `panic = "abort"`, rejects accidental unwinding
at its entrypoints, and documents each `unsafe` block with the local invariant
that makes it valid. Interior mutability must be callback-safe. Process-global
state follows the proven `birnam-id` pattern: short borrows around local state
transitions, never across a send or foreign call.

No third-party crate is added merely to make translation convenient. Each new
dependency requires a source-bootstrap, license, audit, and installed-source
decision. The default remains small internal implementations plus platform C
APIs already in the declared boundary.

The installed source tree preserves the Cargo workspace's repository-relative
shape under `share/birnam/src`: `Cargo.toml` and `Cargo.lock` sit at that root,
and crates remain under `share/birnam/src/rust/<crate>`. This makes the real
workspace member paths executable without an installed-only manifest rewrite.
It is a documentation and audit resource, not a runtime module manifest. The
fixed executable-relative runtime layout from Design 54 remains unchanged.
Native documentation classifies `birnam-id` as `#rustFloor` and every higher
native crate as `#nativeRust`.

## 5. Migration mechanics

### 5.1 Per-rung dual selection

Each rung gains a build selection only when its Rust implementation begins:

```text
-D<rung>-impl=zig|rust
```

The default remains Zig until that rung's acceptance commit. Both
implementations install under explicit oracle names in isolated test prefixes;
the selected implementation alone receives the canonical production name.
The siphon continues to load only canonical names and does not learn a
production fallback order.

After a rung switches, ordinary builds select Rust. The Zig implementation
remains an explicitly selected oracle until its observation gate closes. An
oracle is never silently selected after a Rust failure.

### 5.2 Test topology

Tests compare externally observable behavior, not source structure:

1. pure Rust unit tests for memory and state-machine invariants;
2. current Zig unit tests against both dylib implementations while Zig remains
   available;
3. language-neutral C ABI fixtures;
4. selected-rung integration with the rest of the current production ladder;
5. all-migrated-so-far clean-process integration;
6. frozen compiled-unit and image fixtures;
7. installed-tree and previous-release rollback smokes; and
8. `zig build verify` until the final driver cutover, followed by an equivalent
   `cargo xtask verify` gate that contains every old check.

Stateful backend comparisons run in separate processes. A test process never
initializes two implementations of the same singleton-owning rung.

### 5.3 Commit and rollback policy

Every numbered checkpoint is one focused commit. A commit may add an inert Rust
crate, a characterization test, one behavior family, a dual-backend gate, or a
default switch; it may not combine those into an opaque rewrite.

Before a default switch, rollback is build selection. After a switch, rollback
is explicit selection of the named Zig oracle or the last known-good release.
After oracle deletion, rollback is the previous release or revert of the
focused retirement commit.

No Zig production implementation is deleted in the same commit that first
makes Rust the default.

If a later phase has an explicit cross-implementation matrix, the oracle's
accepted dylib is frozen with identity and digest in the test-only compatibility
fixtures before source deletion. Such a fixture is never installed, distributed,
or selected by production, and is removed from every clean-bootstrap proof.

### 5.4 Observation rule

Every “observation window” in this plan means one of:

1. a post-cutover release built from the accepted switch commit, with full
   verify, distribution, installed-tree, rollback, and relevant optimized
   benchmarks green; or
2. an explicitly agreed equivalent record naming its duration, exercised
   workloads, implementation regressions (including none), regression tests,
   and final dual-implementation comparison.

The observation record lands before the oracle-removal commit. Calendar time
alone and the pre-cutover v0.11.0 release do not satisfy this rule.

Design 61 may retire the live Zig floor independently after its own R8. If that
happens before N2, N2 runs its Zig-floor cross-checks against the frozen v0.11.0
Zig-floor binary in an isolated compatibility-fixture prefix under the rule
above. Design 62 therefore does not prolong a dead production oracle merely for
later comparison.

## 6. Phase N0 — Shared boundary and migration harness

N0 creates no new production behavior.

### N0.1 — Freeze the native-rung inventory

- Record every production dylib, load order, object globals, installed methods,
  exported C symbols, GC layouts, callbacks, native dependencies, and direct
  consumers.
- Assign every generated `birnam_*` symbol to exactly one current owner dylib.
- Record the private Store↔Codegen mutation observer and authorizer contracts,
  including the integer-encoded function pointer, exact signature, registration
  multiplicity, provider lifetime, reentry, and unload prohibition.
- Separate runtime code from tests, benchmarks, sentinels, and the build driver.
- Record the current non-test and test line counts as descriptive evidence, not
  a target.
- Add a check that fails if a production Zig file or exported symbol appears
  without being assigned to this plan.
- Reconcile current image documentation with the implemented v4-only format
  before freezing image compatibility; stale v2/v3 prose is not a contract.

**Gate:** inventory generation is deterministic and covers the installed
runtime closure exactly; ordinary build output is unchanged.

### N0.2 — Extract the shared ABI crate

- Move the Rust `IdApi`, `Oop`, callback, layout, and slot-kind declarations out
  of `birnam-id` into the no-state `birnam-abi` crate.
- Make `birnam-id` consume that crate without changing its exported descriptor,
  symbols, or behavior. Record the expected dylib digest change rather than
  pretending a source/link-layout refactor is byte-identical.
- Generate or check the temporary Zig and C declarations against the committed
  language-neutral ABI manifest.
- Centralize checked byte/string/Oop access helpers without adding policy.

**Gate:** Rust floor tests, Zig oracle tests, old-client fixture, descriptor
manifest, and full ABI symbol audit are byte-for-byte or behaviorally unchanged.

### N0.3 — Add one per-rung selector

- Implement the generic `-D<rung>-impl=zig|rust` build mechanism using the
  artifact-inspector rung as the first inactive target.
- Install canonical, Rust-explicit, and Zig-explicit dylib names in isolated
  prefixes without teaching the runtime to fall back.
- Record implementation identity and dylib digest in test receipts.
- Reject unknown implementations and mismatched identity before running tests.

**Gate:** selecting Zig reproduces the current install tree and verify result;
selecting the still-empty Rust target fails at build selection, not at runtime.

### N0.4 — Generalize the backend-neutral rung harness

- Extract the existing floor-neutral test-launch pattern into a rung-neutral
  clean-process driver.
- Allow one selected rung or the complete migrated set to vary while every
  other rung uses its production default.
- Add child-process assertion helpers for abort-only FFI failures.
- Require a proof that singleton-owning implementations never coexist in one
  process.

**Gate:** the harness reruns the existing artifact-inspector Zig suite and an
all-current-production installed smoke without changing results.

### N0.5 — Capture compatibility and performance baselines

- Freeze representative compiled-unit descriptors, generated C, compiler
  diagnostics, project JSON, image v4 accept/reject behavior, wire-v1, Alien
  callback, and
  installed-tree outputs.
- Include the v4 one-authority/one-code-unit invariant, compatible mapped path,
  explicit rebuild path, and corrupt/older-format rejection fixtures.
- Record optimized timing and memory baselines for artifact inspection, Store
  operations and MOP mutation, PEG parsing, compile/eval, collections,
  relational queries, image save/load, and process startup.
- Use the Design 61 benchmark methodology: isolated prefixes, exact artifact
  digests, guarded pairs, machine-readable comparison, and explicit accepted
  tradeoffs.

**Gate:** two clean runs are reproducible within existing variance limits.
No later phase may invent a weaker acceptance rule after seeing its result.

### N0.6 — Generalize installed native-source resources

- Install the complete Rust workspace under the fixed layout defined in §4.
- Replace the single-floor source-directory assumption with one Rust workspace
  resource while preserving executable-relative resolution and explicit
  overrides.
- Extend native documentation scanning to all Rust crates and distinguish
  `#rustFloor` from `#nativeRust`.
- Preserve offline documentation generation without a runtime manifest or loose
  Birnam production source.

**Gate:** exact install-tree, relocated execution, and relocated offline-doc
smokes pass before the first higher Rust rung lands.
`cargo metadata --offline --manifest-path
<prefix>/share/birnam/src/Cargo.toml` resolves the installed workspace without
network access.

## 7. Phase N1 — Pointer-free artifact boundary

This is the first production rung because it is small, security-sensitive, and
must run before untrusted activation.

### N1.1 — Add the artifact parser crate

- Implement descriptor magic search, bounded length decoding, field decoding,
  duplicate/missing-field rejection, and target/ABI comparison in Rust.
- Keep it independent of `IdApi`, `dlopen`, and object allocation.
- Run the same valid, corrupt, truncated, duplicate, oversized, and adversarial
  byte fixtures against Zig and Rust.

**Gate:** exact accept/reject and diagnostic-category parity; fuzz/property
inputs cannot panic, allocate unboundedly, or read out of bounds.

### N1.2 — Add the Rust inspector dylib shell

- Add the artifact and descriptor object types through `birnam-abi`.
- Register the exact current globals, layouts, finalizers, and selectors.
- Convert parsed fields to Birnam objects only after inert parsing succeeds.
- Export only the existing `module_init` boundary.

**Gate:** module initialization and object protocol tests pass in an otherwise
Zig higher-rung stack.

### N1.3 — Prove the unchanged host preflight against both inspectors

- Keep the host-owned `artifact_preflight.zig` closure walker unchanged.
- The inspector validates descriptor encoding, target, artifact ABI, and native
  requirements and exposes capabilities without selecting policy from them.
- Run the host's requested-root/identity, sibling module-dependency, sibling
  identity, and cycle validation against each inspector implementation.
- Prove complete closure rejection occurs before any candidate or dependency
  artifact constructor, `module_init`, `__birnam_unit_init`, or `__ev`. The
  trusted inspector dylib's own `module_init` necessarily ran to provide this
  inert protocol.
- Keep the independent Zig sentinel until host migration; a Rust or C sentinel
  may be added, but does not replace the independent fixture here.

**Gate:** the full artifact-preflight matrix passes in clean child processes,
including a later-invalid sibling that cannot leave an earlier unit activated.
The closure walker moves only with `birnam-host` in N9.

### N1.4 — Switch and observe

- Run full verify, installed relocation, project build/run, old compiled-unit,
  and previous-release rollback gates.
- Make Rust the default artifact-inspector implementation in a focused commit.
- Record an observation checkpoint before removing the Zig inspector.

**Gate:** no accepted regression and no artifact-boundary bug through the
declared observation window.

## 8. Phase N2 — Store and MOP

Store is migrated before language and codegen so every later port exercises the
canonical MOP law through the Rust implementation.

### N2.1 — Characterize Store state

- Freeze cell identity, basis/transaction ordering, assert/retract/flip,
  as-of/since/between history, cardinality, Globals, root tracing, precise
  layouts, image import/export, and deterministic enumeration.
- Add the full tracked/raw add/replace/remove/backfill MOP matrix.
- Assert the dictionary directly and the mirror independently after every
  transition and raw escape-hatch path.
- Freeze authorizer denial as pre-mutation/no-change. Freeze the observer as a
  post-commit notification whose answer is ignored; it does not roll mutation
  back.
- Freeze object-observer and raw-authorizer registration, duplicate-provider
  rejection, reentry, and provider-dylib lifetime.

**Gate:** the new suite passes against the Zig Store and the Rust floor plus
either the live Design 61 oracle or the frozen compatibility fixture specified
in §5.4.

### N2.2 — Add the Rust Store shell and storage core

- Implement rung-owned allocation, cell/fact indexes, transaction basis, and
  deterministic iteration without exposing Rust collection order.
- Register root providers and trace hooks with callback-safe ownership.
- Implement raw assert, flip, lookup, history, and cardinality before installing
  higher policy.
- Represent the authorizer as an `unsafe extern "C" fn`, reconstructed from the
  existing encoded address without creating a Rust reference. The provider
  Codegen dylib remains pinned for process lifetime; re-registration accepts
  only the same observer/address and rejects a different provider.

**Gate:** focused storage/history tests and forced GC pass with Rust Store under
the required floor matrix; Zig-Codegen→Rust-Store authorization, denial,
post-commit observation, reentry, duplicate registration, and lifetime tests
pass; no borrow or lock spans a callback.

### N2.3 — Implement the MOP chokepoint

- Port tracked add/remove and the method mutation observer/authorizer.
- Preserve exact retract-before-dictionary-before-assert ordering.
- Backfill all real vtable dictionaries with exact or `#primitive` values and
  exclude private/internal dictionaries under the current rules.
- Prove raw mutation can produce absence but never a stale or false mirror.

**Gate:** every Design 42 consistency-law case passes after each individual
operation, after image load, and after compiled-unit activation/rollback.

### N2.4 — Complete Store object protocol and image bridge

- Port live triple iteration, as-of ranges, image export/import, Globals, cell
  protocol, reports, and deterministic diagnostics.
- Run relational, codegen, compiled-unit, image, and project integration against
  Rust Store.
- Move policy into Birnam only when it deletes the native version and does not
  put a per-row send in a measured hot loop.

**Gate:** all Store consumers, image fixtures, MOP tests, full verify, and the
N0 performance comparator pass.

### N2.5 — Switch and observe

Make Rust Store the production default in one commit. Retain the Zig Store as an
explicit oracle through an observation window covering live method
replacement, image save/load, and project workflows before deletion.

## 9. Phase N3 — PEG matcher

### N3.1 — Freeze parser behavior

Record grammar construction, every PEG operator, CST shape and child ordering,
byte ranges, failure position, recursive rule behavior, malformed grammar
diagnostics, deep/backtracking inputs, and compile-corpus performance.

### N3.2 — Port pattern and grammar representation

Implement Rust-owned patterns, grammars, CST nodes, exact GC layouts, root
handling, and current Birnam message protocol. Keep construction semantics out
of `libid`.

### N3.3 — Port the matcher

Implement the hot iterative/backtracking engine with explicit bounds and
deterministic furthest-failure behavior. No Birnam send is introduced per input
byte or backtracking step.

### N3.4 — Prove downstream compiler parity

Run the unchanged Zig language and codegen rungs on the Rust PEG. Compare full
CSTs, generated C, diagnostics, module metadata, compiled-unit descriptors, and
the whole source corpus.

### N3.5 — Switch and observe

Switch the PEG dylib only after parser performance, deep-input robustness, full
bootstrap, and installed compilation pass. Retain the Zig matcher oracle through
the compiler migration.

## 10. Phase N4 — libtcc and generated runtime shim

### N4.1 — Freeze the native-code backend

Freeze every libtcc operation, error callback, symbol binding, output mode,
library path, relocation/finalizer behavior, runtime `birnam_*` export, and
generated-code callback. The inventory assigns TCC-owned forwarding,
allocation, layout, and C-string symbols separately from Codegen-owned global,
method-mutation, candidate, quiescence, and lifecycle symbols.

### N4.2 — Port the TccUnit mechanism

Wrap libtcc in Rust with explicit ownership states: created, configured,
relocated, installed, disposable, and released. Prevent double release and
use-after-finalize without holding Rust borrows across libtcc callbacks.

### N4.3 — Port runtime symbols

Export only the generated-C symbols currently owned by `tcc.zig` and route them
through `IdApi` exactly as today. Preserve raw/private method installation only
where the current compiled-unit contract permits it. The still-Zig Codegen
continues to own its disjoint symbol set until N6; no mixed stack contains two
providers for one symbol.

### N4.4 — Switch and observe

Require generated-C execution, callback reentry, compiled dylibs in another
process, old compiled units, Alien, image composition, and optimized compile
performance before switching. An installed symbol audit must prove exactly one
owner for every generated-C symbol before and after the switch.

## 11. Phase N5 — Rust language-parser rung

Port `lang.zig` as an independent dynamic rung used by the cold compiler, not as
a warm self-hosting ratio exercise.

### N5.1 — Add the Rust lang dylib shell

Create `libbirnam_id_lang.dylib` with the exact current `module_init`, Lang
object/vtable, globals, selectors, layouts, failure state, and PEG dependency.
Internal compiler support crates may be shared, but do not erase this load
boundary.

### N5.2 — Port the cold grammar

Represent the complete Birnam grammar in Rust over the accepted PEG API. Freeze
grammar and selector spellings in one shared source description only if doing
so adds no generated bootstrap prerequisite.

### N5.3 — Prove protocol and corpus parity

Compare exact CST, failure position, and diagnostics across the source corpus
and adversarial malformed inputs. Run the Rust parser with Rust PEG and with the
retained Zig PEG oracle to distinguish parser from matcher failures.

### N5.4 — Switch and observe

Switch the canonical lang dylib only after clean from-zero compilation reaches
the unchanged Zig code generator, full verify and optimized parser gates pass,
and the §5.4 observation record is defined. Retain the Zig lang oracle through
the Codegen migration.

A Birnam grammar/compiler remains optional and additive. It cannot replace this
phase's cold parser.

## 12. Phase N6 — Rust code generator and cold-compiler cutover

This is the largest phase and must be decomposed by behavior rather than ported
as one file.

### N6.1 — Freeze compiler authority

- Expand generated-C goldens to cover every syntax form, block capture,
  primitive, global, method definition/removal, source record, module metadata,
  embedded file, artifact descriptor, candidate generation, and image replay.
- Normalize only nondeterministic toolchain/container bytes; do not normalize
  semantic text, salting, ordering, or digests.
- Freeze the public compiler object protocol and all exported runtime symbols.
- Assign and test the Codegen-owned global, method-mutation, candidate,
  quiescence, and lifecycle symbols independently of the already-migrated
  TCC-owned symbols.

### N6.2 — Port deterministic source emission

Port CST walking, identifier/salt generation, literal encoding, block/method
emission, and translation-unit assembly. A Rust byte buffer replaces Zig's
`ArrayList`; no O(n²) Birnam string construction is introduced.

**Gate:** emitted C text is exact for the frozen corpus before any generated C
is executed.

### N6.3 — Port eval and compile orchestration

Connect the Rust emitter to the accepted TCC rung for eval, output dylibs,
source paths, error values, and temporary-resource cleanup.

**Gate:** eval and compile results match across clean processes; failed
compilation publishes no artifact and leaks no live code owner.

### N6.4 — Port compiled-unit metadata and loading

Implement module metadata, embedded source/files, descriptors, source/revision
digests, inert inspection handoff, mapping, runtime initialization, and
host-capability loading.

**Gate:** old and new compilers' units load in both old and new compiler
stacks, subject to the frozen ABI.

### N6.5 — Port live generation lifecycle

Implement candidate compilation, validation, preparation, commit, rollback,
discard, reconciliation, mutation authorization/observation, generation
projection, and exact MOP updates. Initializer effects remain outside the
dispatch-transaction claim. Register the Rust Codegen observer and raw
authorizer with Rust Store under the frozen N0 contract; no Rust reference or
borrow crosses that callback.

**Gate:** failed candidates are inert; activate/replace/remove/rollback leaves
the exact dictionary and mirror required by Designs 42, 45, 47, and 49.
Before the Codegen default switch, the complete Zig-Codegen→Rust-Store,
Rust-Codegen→Rust-Store, and Rust-Codegen→live-or-frozen-Zig-Store matrix passes.
Each combination covers authorizer denial, post-commit observer notification,
reentry, provider lifetime, candidate rollback, and actual Store rollback
selection.

### N6.6 — Port image compilation and recovery

Implement image-code composition, replay, global/method derivation, generation
digests, and quiescence barriers without changing image authority.

**Gate:** compatible boot uses captured dylibs; explicit rebuild uses source;
both converge on equivalent live behavior. Frozen v4 images produced by the Zig
implementation or a previous compatible release remain readable; unsupported
older formats remain rejected.

### N6.7 — Prove the independent Rust cold compiler

From a clean checkout with no Birnam artifacts:

1. build the Rust compiler crates from Rust source with Cargo;
2. build the still-transitional native rung closure from declared sources;
3. compile every self-hosted and library Birnam source to dylibs using only the
   Rust implementation of the Birnam parser/TCC/Codegen stack;
4. compare its behavior and deterministic emitted C with the frozen Zig Birnam
   compiler fixtures;
5. repeat from a second independent clean checkout/cache/prefix and compare
   emitted C and compiled-unit descriptors;
6. run full verify and installed relocation from the resulting closure; and
7. prove that no Zig implementation of the Birnam compiler and no prebuilt
   Birnam artifact participated. The Zig toolchain may still build unmigrated
   rungs and the host until N11.

This is a cold-independence and determinism proof, not a self-application
fixpoint. `C_N == C_(N+1)` applies only if a compiler implemented in Birnam later
compiles itself.

The Rust compiler becomes default only in a separate commit after this gate.
The Zig compiler remains an oracle through an observation window covering
project builds, live replacement, images, and release packaging.

## 13. Phase N7 — Primitive library rungs

Migrate one dylib and one default-switch commit at a time in load order.

### N7.1 — Prelude

Port primitive arithmetic, conversion, printing, filesystem/process/system
calls, native loop primitives, dynamic module load, and stack/runtime controls.
Keep already-derived arithmetic and library protocol in `stdlib.birnam`.

### N7.2 — Collection

Port Vector/RRB, Map/HAMT, Range, layouts, tracing, and measured hot scans.
Retain existing Birnam higher-order protocol. Re-run the send-cost gates before
moving any additional loop to Birnam.

### N7.3 — Error

Port the error object core, propagation primitives, DNU swap, and factory.
Keep the shipped Birnam error-handling protocol. Prove DNU restoration and
error identity through both compiler implementations.

Each subphase requires exact behavior tests, forced GC, full source bootstrap,
full verify, optimized comparison, focused default switch, and observation
record before its Zig oracle is removed.

## 14. Phase N8 — Service and persistence rungs

Again, each item is an independently selectable dylib and independently
committed cutover.

### N8.1 — Isolate

Port process creation, pipes, worker boot, bounded copied-value wire-v1,
blocking request, failures, and owned termination. Keep orchestration policy in
`isolate.birnam`. Do not add concurrency, replacement, or a new wire version.

### N8.2 — Relational

Port indexes, rule evaluation, join, negation, aggregation, pull, deterministic
ordering, and Store integration. Move surface composition to Birnam only when
the native code is deleted and N0 query gates show no unacceptable regression.
Never make relational facts authoritative over objects or dispatch.

### N8.3 — Alien

Port library ownership, symbol lookup, C type/layout handling, argument/result
marshalling, callbacks, native buffers, wrapper compilation, finalizers, and
reentry. Run the full ABI-shape corpus and hostile lifetime cases; no Rust
reference may cross an Alien callback.

### N8.4 — Image

Port v4 decode and encode, its single authority bound to one code unit,
checksums, graph classification, exact slot swizzling, code records,
quiescence, mapping, atomic save, rejection reports, explicit rebuild, and
rejection of unsupported older formats. Require cross-product tests:

- Zig-written and Rust-written v4 images;
- Zig and Rust Store;
- Zig and Rust Codegen;
- compatible and rebuild paths; and
- corrupt, truncated, duplicate/missing-authority, multiple-code, and
  unsupported-format fixtures.

The Rust image implementation becomes default only after frozen compatible v4
images and previous-release rollback pass from an installed tree. Frozen v4
images from the Zig implementation or a previous compatible release remain readable;
unsupported older formats remain rejected. “Zig Store/Codegen” in this late
cross-product means the live oracle if still retained, otherwise the
identity-and-digest-pinned compatibility dylib preserved under §5.3.

## 15. Phase N9 — Rust host and project workflow

The host moves last because it is the native stack anchor and composes every
other rung.

### N9.1 — Port runtime resource resolution and raw floor load

Implement the fixed executable-relative installed layout and narrow exact
`BIRNAM_LIB_DIR`/`BIRNAM_ID_DYLIB` overrides, the one raw `libid` load,
`id_init`, floor identity checks, native stack-bottom registration, and ordered
canonical dylib mounts. There is no cwd, checkout, source, or manifest fallback.

### N9.2 — Port host-owned artifact closure preflight

Port requested-root and identity checks, sibling dependency traversal, cycles,
sibling identity, and diagnostics into `birnam-host`. The inspector continues
to validate descriptor encoding, target/ABI, and native requirements and to
expose capabilities; the host adds no capability-selection policy. It consumes
only the accepted inspector object protocol and must reject the complete
closure before loading the language stack or executing a candidate/dependency
artifact initializer.

### N9.3 — Port the generic Birnam entry path

Port argument/exit handling, script/eval/repl delegation, artifact root
activation, image and baked-executable paths, and generic CLI dispatch. Keep
command policy in the compiled Birnam CLI/Repl modules.

### N9.4 — Port project resolution

Move filesystem traversal, hashing, manifest transport, source composition, and
Birnam object marshalling to Rust. Keep ProjectPolicy's exact shipped ownership
in Birnam: provider selection, dependency closure and diagnostics,
dependency-first ordering, reuse-versus-compile, and dirty propagation.
Manifest observation/validation, source/CST scanning, and runtime run semantics
remain host mechanism, not new ProjectPolicy authority. Preserve diagnostics and
deterministic reports.

### N9.5 — Port project build/publication

Move compiler invocation, native wrapper processes, staging, receipts,
checksums, atomic publication, reuse validation, and cleanup to Rust. Preserve
ProjectPolicy ownership of artifact-record shape and publication authorization;
keep process invocation, copying, serialization, and atomic rename in the host.
Preserve the rule that installed compiled-unit descriptors, not build receipts,
are runtime authority.

**Gate:** existing project-policy unit tests and full project
check/build/run/test/install workflows prove that no decision crossed the
Design 57 seam.

### N9.6 — Port project check

Preserve `birnam-check-v1`, human and JSON output, compile-only checking,
failure propagation, and LSP consumption.

### N9.7 — Host cutover

Require every command, golden transcript, LSP project workflow, relocated
install, baked executable, image path, project build/check/run/test/install,
distribution archive, and previous-release rollback before making the Rust
binary default. The preflight gate must again prove that no invalid closure
activates any unit. Keep the Zig siphon as an explicitly named oracle during
the host observation window.

## 16. Phase N10 — All-Rust production closure

1. Run every rung with its Rust implementation in one clean installed prefix.
2. Re-run from-zero bootstrap without prebuilt Birnam artifacts.
3. Run all frozen old-client, compiled-unit, image, wire, project, and release
   fixtures.
4. Run optimized performance and memory comparisons against the last
   all-Zig-higher-rung release and record every accepted tradeoff.
5. Audit dylib install names, dependencies, rpaths, exported symbols, source
   bundle, licenses, and absence of accidental Zig runtime linkage.
6. Publish a post-cutover release or complete an explicitly agreed equivalent
   observation window.
7. Retire each higher-rung Zig oracle in focused commits only after its
   observation evidence is recorded.

This phase does not override Design 61 R8. The Zig `libid` oracle is removed only
when its own post-cutover observation condition has been satisfied.

## 17. Phase N11 — Zig toolchain retirement

Only after N10 and Design 61 R8:

1. port independent ABI, artifact, image, compiler, and behavior fixtures that
   still require Zig to Rust or C;
2. port benchmark and sentinel executables;
3. implement `cargo xtask` commands for build, test, stage1, install, verify,
   docs, distribution, and benchmark acceptance;
4. build vendored libtcc and every dynamic rung from source without `build.zig`;
5. make `cargo xtask verify` execute every gate formerly owned by
   `zig build verify`;
6. compare install trees and distribution contents;
7. prove relocated offline docs cover `#rustFloor`, every `#nativeRust` crate,
   and the Birnam guides without a runtime manifest;
8. remove Zig source projections, tests, build files, CI setup, installed Zig
   sources, and the Zig toolchain pin in focused commits; and
9. perform one final clean machine bootstrap using only the declared Rust/C
   toolchains and source checkout.

Historical design documents remain historical; current guides and contracts
must describe the Rust implementation honestly.

## 18. Acceptance checklist

- [ ] Clean source bootstrap uses no prebuilt Birnam artifact.
- [ ] A complete independent Rust cold compiler builds without Birnam artifacts,
      matches frozen compiler semantics, and emits identical deterministic C
      across two independent clean builds.
- [ ] `libid` remains the only floor.
- [ ] Every production runtime module remains a dynamically loaded library.
- [ ] `IdApi`, generated C ABI, compiled-unit descriptor, object layouts,
      image v4, and wire-v1 remain compatible; unsupported image formats remain
      rejected.
- [ ] The object dictionary remains canonical and every MOP mutation satisfies
      the exact/`#primitive`/absent law.
- [ ] Artifact closure is rejected before activation.
- [ ] Compatible images execute captured dylibs; rebuild remains explicit.
- [ ] No Rust borrow or lock spans a reentrant send or foreign callback.
- [ ] No panic or unwind crosses a C ABI.
- [ ] Every generated `birnam_*` symbol has exactly one owner in every mixed and
      final installed stack.
- [ ] Store↔Codegen authorizer/observer behavior, provider lifetime, and reentry
      satisfy the frozen private callback contract.
- [ ] Full current behavior, installed, release rollback, robustness, and
      optimized performance gates pass.
- [ ] Every accepted regression is executable, explicit, and recorded before a
      default switch.
- [ ] Each Zig oracle survives its switch commit and is removed only after its
      observation gate.
- [ ] Design 61 R8 is independently satisfied before `id.zig` is removed.
- [ ] The final production/runtime/install closure contains no Zig
      implementation or Zig runtime dependency.
- [ ] The final build and verification path requires no Zig toolchain.

## 19. Effort and critical path

This is a large systems rewrite, not a mechanical translation. The phase table
is an unbuffered **40–64 engineering weeks plus observation windows**:

| Work | Estimate |
|---|---:|
| N0 shared ABI, selectors, fixtures, baselines | 2–3 weeks |
| N1 artifact boundary | 1–2 weeks |
| N2 Store/MOP | 3–5 weeks |
| N3 PEG | 2–3 weeks |
| N4 libtcc/runtime shim | 2–4 weeks |
| N5 parser | 1–2 weeks |
| N6 codegen/cold compiler | 8–12 weeks |
| N7 primitive libraries | 4–6 weeks |
| N8 isolate/relational/Alien/image | 8–12 weeks |
| N9 host/project workflow | 4–6 weeks |
| N10 closure and observation | 2–4 weeks plus calendar time |
| N11 test/build toolchain retirement | 3–5 weeks |

For planning, add 25–40% systems/FFI/bootstrap contingency: approximately
**50–90 engineering weeks plus observation calendar time** for one experienced
developer. N6, N8, and N11 have the widest variance.

The critical path is Store → PEG → TCC → parser → codegen → image → host. The
highest semantic risks are compiled-unit generation lifecycle, MOP atomicity,
Alien callback ownership, image compatibility, and proving that the Rust
compiler is genuinely cold. The highest schedule risk is N6: generated-C parity
alone is insufficient; live replacement, image composition, project builds,
and old artifacts all consume the compiler's extended behavior.

If a phase discovers that parity requires changing an invariant, the phase
stops and receives a separate design. The rewrite is not evidence that the
invariant was wrong.
