# 62 — Rust Native-Rung Migration Plan

> **Status:** Plan of record, 2026-07-23; stabilized after three adversarial
> passes. N0.1 through N0.6, N1.1–N1.4, N2.1, N2.2d, N2.3a, N2.3b, N2.3c,
> and N2.4a through N2.4d are complete; N2.5a through N2.5c, N3.1, N3.2a,
> N3.2b, N3.2c, and N3.3 are complete; N3.4a through N3.4i and N3.4 are
> complete; N3.5.1 through N3.5.3 and N4.1 through N4.4 are complete; N5.1
> through N5.4 and N6.1 are complete; N6.2 is in progress.
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

#### N0.1 execution record — complete, 2026-07-23

`abi/native-rungs-v1.json` assigns all 26 current non-test Zig/build sources,
records the 13 dynamic rungs in exact load order, names globals, dependencies
and consumers, and assigns all 41 non-`module_init` exports to one owner.
Per-rung fingerprints cover method/global/layout/hook/finalizer/import surface
lines, so an unreviewed native-surface change fails closed.

The inventory also freezes the private Store↔Codegen observer/authorizer seam
and image format v4. Current Designs 40 and 45 now match the v4-only
one-authority/one-code-unit implementation instead of claiming v2/v3 support.
The focused `zig build test-native-rung-inventory`, ordinary `zig build`, image
unit suite, and whitespace gate pass.

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

#### N0.2 execution record — complete, 2026-07-23

The no-state `birnam-abi` workspace crate now owns the frozen `IdApi`, `Oop`,
callback, layout, slot-kind, and diagnostic descriptor declarations.
`birnam-id` consumes that crate through a path dependency; it no longer owns a
private ABI copy. Checked foreign byte and Oop buffer borrowing is centralized
there without importing runtime state or language policy.

The existing language-neutral `abi/idapi-v1.json` remains authoritative. Its
Zig, C, and Rust layout checks, the frozen old-client fixture, and the complete
debug/release exported-symbol audit pass unchanged. The expected Rust dylib
digest change is recorded by the audit rather than treated as ABI drift.

Both crate sources and manifests are installed. Until N0.6 generalizes the
installed workspace/resource model, doc generation receives the two Rust source
roots explicitly and preserves the existing `rust-floor` and `rust-abi`
classification. Rust workspace tests and strict Clippy, the ABI/old-client
gates, native-rung inventory, self-hosted 334-test suite, relocated install
smoke, and the complete repository verification gate pass.

### N0.3 — Add one per-rung selector

- Implement the generic `-D<rung>-impl=zig|rust` build mechanism using the
  artifact-inspector rung as the first inactive target.
- Install canonical, Rust-explicit, and Zig-explicit dylib names in isolated
  prefixes without teaching the runtime to fall back.
- Record implementation identity and dylib digest in test receipts.
- Reject unknown implementations and mismatched identity before running tests.

**Gate:** selecting Zig reproduces the current install tree and verify result;
selecting the still-empty Rust target fails at build selection, not at runtime.

#### N0.3 execution record — complete, 2026-07-23

`-Dartifact-inspector-impl=zig|rust` is the first per-rung implementation
selector. Zig remains the default. It is installed under both the canonical
`libbirnam_id_artifact.dylib` name and the explicit
`libbirnam_id_artifact_zig.dylib` oracle name; the siphon still loads only the
canonical name and has no fallback order.

Before the artifact-inspector tests run, the generic selection check proves
that the canonical artifact is byte-identical to the explicit artifact for the
declared implementation and emits a JSON receipt containing the rung,
implementation identity, and SHA-256. The check rejects unknown identities,
mismatched explicit names, missing artifacts, and digest mismatches.

At this checkpoint no Rust inspector existed, so selecting `rust` failed during
build-graph construction with an explicit N1 diagnostic and installed nothing.
An unknown enum value was rejected by the Zig build option parser. The focused
selector smoke exercised both failures in isolated caches and prefixes.
Focused artifact/selector tests, ordinary install relocation, and full
verification passed with the unchanged Zig canonical implementation. N1.4
later changed the default after the Rust shell was proven.

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

#### N0.4 execution record — complete, 2026-07-23

The artifact-inspector suite no longer imports `artifact_inspector.zig`.
`rung_adapter_test.zig` loads the selected canonical dylib through `module_init`
after checking its rung identity, implementation-specific filename, and
byte-for-byte digest against the explicit oracle artifact. It retains the
handle and reload state for process lifetime and makes an identical repeated
selection idempotent.

`scripts/native-rung-harness.sh` supplies isolated prefix, cache, temporary
directory, and process boundaries for either one
`artifact-inspector=zig` selection or the complete currently migrated
production set. Every unselected rung remains at its production default. The
selected mode reruns the four existing inspector tests through the dylib
boundary; the all-production mode reruns relocated installed-tree smoke.

A dedicated child proves that changing rung implementation or artifact path in
one process is rejected before a second `dlopen`. The shared
`assert-child-abort.sh` helper now checks SIGABRT plus a required diagnostic for
that proof and for the Rust FFI and GC fatal-boundary suites. Focused harness,
artifact, singleton, FFI-abort, GC-metadata, installed-tree, and complete
verification gates pass.

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

#### N0.5 execution record — complete, 2026-07-23

`abi/native-rung-baseline-v1.json` is the machine-readable compatibility
contract. Its fail-closed checker freezes the representative inert
compiled-unit descriptor, generated-C boundary fragments, exact compiler
diagnostic and project JSON, image-v4 authority and code-unit rules, mapped and
explicit-rebuild paths, corrupt and older-format rejection, wire-v1 bytes,
Alien callback values and lifecycle cycles, and installed-tree expectations.
The generated-C projection exists only for tests; it adds no production
protocol. An explicit v3-header test closes the previously implicit
older-format rejection evidence.

The accepted, digest-pinned R6 Zig/Rust Design 46 pair remains the optimized
comparator for Store and MOP mutation, PEG parsing, compile/eval, collections,
relational queries, image save/load, and startup. The checker requires the
ReleaseSafe, non-quick stamps; exact 3/1/5/3/3 suite process counts; macro
repetition counts and peak RSS; every category's named cases; both result-file
digests; and the exact two accepted R6 crossings. It cannot silently substitute
a newer or partial result.

Artifact inspection has its own backend-neutral ReleaseSafe probe so the frozen
Design 46 micro suite and its loaded-module footprint remain unchanged. Two
host-guarded clean processes each inspected the same inert descriptor 5,000
times through the selected dylib boundary. They recorded 7,324 and 7,554
ns/inspection with identical 13,598,720-byte peak RSS: 3.14% time variance and
0% RSS variance, within the predeclared 10% and 5% limits. The exact floor and
rung dylib digests, sizes, measurements, and limits are committed in
`bench/baselines/zig-artifact-inspector-n0.5.json`.

`zig build test-native-rung-baseline` verifies the complete semantic and
performance freeze and is part of `zig build test`. Focused ReleaseSafe
Codegen/Image suites and the baseline gate pass.

The R6 acceptance script was rerun after the Rust source-registration and
revision-metadata slices (`aafb835`, documented by `5c155af`). The native-rung
baseline, all six focused replacement gates (including the 18-test Rust
Codegen runtime suite), and the full optimized closure passed. Full verify
still reports only the exact quarantined `run-tests.birnam`/`sortedModules`
fixture crash; the fixed-hash comparator reports no missing or unusable cases
and exactly the accepted `query.aggregate +5.8%` and
`compile.stage1-stdlib +11.9%` crossings.

After the artifact dependency-loader gate landed at `13ec3d3`, the same R6
script passed at `3d55d58`. Its focused Rust Codegen target passed 17/17,
including dependency rejection before artifact initialization; the optimized
closure retained the exact quarantined fixture signature, and the comparator
retained the same digest-pinned pair and two accepted crossings. This is a
functional coverage update only, not a new performance exception.

After the Rust image framing preflight entered the opt-in Codegen boundary at
`f7a1de7`, the acceptance pin advanced to 18/18 Rust Codegen tests. The complete
acceptance script passed again at `73b15c4`: the new case validates a v4
authority envelope and rejects a bad magic without loading, swizzling, or
mutating Store state; the optimized closure retains only the exact quarantined
fixture signature, and the comparator retains the same exact benchmark
snapshot and two recorded crossings. Mapped-unit composition and
materialization remain N6.6 work.

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

#### N0.6 execution record — complete, 2026-07-23

The installed Rust resource is now one repository-relative workspace:
`share/birnam/src/Cargo.toml` and `Cargo.lock` remain at the workspace root,
with every Rust crate copied below `share/birnam/src/rust/<crate>`. The build
copies the workspace tree wholesale, so adding a crate does not require another
per-crate install rule. `runtime_resources` resolves the workspace root
relative to the executable and accepts the narrow `BIRNAM_RUST_WORKSPACE_DIR`
override; it no longer exposes separate floor and ABI source directories.

The CLI and docgen boundary now pass that one workspace resource. The native
scanner discovers every crate under `rust/`, preserves the established
`rust-*` identities for `birnam-id`, gives other crates collision-free
`rust-<crate>` identities, and records `#rustFloor` only for `birnam-id` while
recording `#nativeRust` for `birnam-abi` and future higher native crates. The
HTML reference has separate Rust-floor and Native-Rust groups.

The relocated install smoke compares the complete Rust workspace, rejects the
old nested Cargo manifest, runs `cargo metadata --offline --no-deps` against
the installed root manifest, and generates documentation after relocation.
It also confirms there is no runtime manifest and no loose production Birnam
source. `zig build test-birnam`, `zig build test-install`, and the focused
baseline gate pass.

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

#### N1.1 execution record — complete, 2026-07-23

The new `birnam-artifact` workspace crate parses the pointer-free descriptor
from borrowed byte slices only. It performs bounded magic search, checked
length arithmetic, all nine field/list reads, the 4096-entry list bound,
duplicate-valid-record rejection, and ABI/target/identity validation without
`IdApi`, `dlopen`, object allocation, or activation policy. Malformed magic
candidates retain the Zig inspector's public `artifactDescriptionMissing`
category; ABI and target mismatches remain distinct.

The Zig inspector and Rust parser now cover the same valid, ABI/target mismatch,
garbage, truncated, duplicate, oversized-list, and deterministic adversarial
byte cases. The Rust adversarial loop exercises 10,000 generated 257-byte
inputs; the Zig boundary suite exercises the same bounded-failure shape through
the selected inspector dylib. `cargo test --workspace --locked --offline`,
`zig build test-rust-artifact`, the focused Zig artifact suite, the relocated
install smoke, the native-rung inventory, and the complete `zig build verify`
gate pass. The installed workspace metadata assertion now includes
`birnam-artifact`.

### N1.2 — Add the Rust inspector dylib shell

- Add the artifact and descriptor object types through `birnam-abi`.
- Register the exact current globals, layouts, finalizers, and selectors.
- Convert parsed fields to Birnam objects only after inert parsing succeeds.
- Export only the existing `module_init` boundary.

**Gate:** module initialization and object protocol tests pass in an otherwise
Zig higher-rung stack.

#### N1.2 execution record — complete, 2026-07-23

`birnam-artifact` now also emits the Rust `cdylib` inspector shell. Its
`module_init` registers the existing `Artifact`, `ArtifactVt`, and
`ArtifactDescriptorVt` objects, the exact 13-Oop descriptor layout, all current
selectors, and the existing reload/export behavior through `birnam-abi`. It
materializes Birnam objects only after the borrowed parser has accepted the
inert descriptor and preserves the existing missing/ABI/target/native-resource
diagnostic categories. File reads are bounded by the same 1 GiB ceiling and
native requirements are checked relative to the artifact without activation.

`-Dartifact-inspector-impl=rust` now installs and tests the Rust dylib under
`libbirnam_id_artifact.dylib` plus the explicit
`libbirnam_id_artifact_rust.dylib` identity; Zig remains the explicit oracle and
default. The shared artifact-inspector suite exercises both implementations,
including descriptor fields, counts, indexed access, and the inert Vector
boundary. Rust workspace tests, strict Clippy, Zig and Rust focused artifact
selection, ReleaseSafe Rust selection, relocated install verification, and the
complete `zig build verify` gate pass. The generic selection smoke now treats
Rust as an implemented choice and still rejects unknown implementations.

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

#### N1.3 execution record — complete, 2026-07-23

`artifact_preflight_test.zig` now loads the selected inspector through the
backend-neutral rung adapter; it no longer reaches into the Zig inspector
implementation. The existing seven-case sentinel process therefore exercises
the exact host closure walker against whichever inspector is selected, while
retaining the independent aborting artifact initializer proof.

The ordinary Zig and Rust-floor lower-rung matrices continue to run the Zig
inspector, and each isolated Zig-floor, Rust-debug-floor, and Rust-release-floor
matrix now adds a separate Rust-inspector preflight process. All cases preserve
the requested-root identity, kind, dependency identity, missing sibling, cycle
and activation-order diagnostics. The Rust inspector never activates the
candidate or dependency initializer during preflight. Focused Zig/Rust
preflight runs, the expanded lower-rung matrix, and the complete verification
gate pass.

### N1.4 — Switch and observe

- Run full verify, installed relocation, project build/run, old compiled-unit,
  and previous-release rollback gates.
- Make Rust the default artifact-inspector implementation in a focused commit.
- Record an observation checkpoint before removing the Zig inspector.

**Gate:** no accepted regression and no artifact-boundary bug through the
declared observation window.

#### N1.4 execution record — complete, 2026-07-23

The canonical artifact-inspector selection now defaults to Rust. The selected
Rust dylib remains installed at `libbirnam_id_artifact.dylib`; the Zig
implementation remains independently installed as
`libbirnam_id_artifact_zig.dylib` and is still selectable explicitly for oracle
and rollback checks. No runtime fallback or dual implementation loading was
added.

The focused default-Rust selector receipt, all seven host preflight cases, and
relocated install smoke passed in isolated prefixes. The complete `zig build
verify` gate then passed with Rust selected by default, including both Rust
floor profiles, the Zig-floor compatibility matrix, Rust-inspector preflight
for every floor, project build/run, image save/load, old compiled-unit and
rollback fixtures, and the 343-test self-hosted suite. The observation window
starts from this commit; the Zig inspector is retained until its declared
retirement checkpoint.

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

#### N2.1 execution record — complete, 2026-07-23

The Store characterization suite now has a complete direct-MOP transition
matrix: tracked add/replace/remove, raw add/replace/remove, primitive-marker
backfill, repeated backfill, and the absent-after-removal boundary. Each step
checks the canonical direct dictionary and the derived mirror independently, so
an inherited method, stale exact method, or stale `#primitive` marker cannot
make the test pass accidentally.

The suite also freezes pre-mutation authorizer observation, post-commit
observer notification, ignored observer answers, synchronous observer
re-entry, duplicate observer/authorizer registration, and same-provider
idempotence. The existing history, cardinality, Globals, image backfill,
precise tracing, and raw-mutation tests remain in the same process-level
characterization boundary.

`rust-store-mop-smoke.sh` now runs the 19-test Zig Store suite against the live
Zig-floor oracle and Rust debug/release floors, plus the Store image-backfill
fixture for each floor. The expanded matrix passes in clean prefixes and the
Rust lower-rung/full verification gates remain green. No Rust Store
implementation has been introduced in this phase; N2.2 owns that shell and
storage core.

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

#### N2.2a execution record — complete, 2026-07-23

The workspace now contains a dedicated `birnam-store` `cdylib` member with the
same `module_init(*IdApi, state)` ABI and a static Rust implementation identity.
Its current shell is intentionally inert: it validates the host table and
publishes no Store objects until the storage protocol is ready, so the Zig
Store remains the production implementation and cannot be accidentally
co-loaded. Workspace lock/resource/install metadata includes the new member;
strict Clippy and the shell unit test pass. N2.2b will add the rung-owned state
and first raw storage protocol behind this boundary.

#### N2.2b execution record — complete, 2026-07-23

`birnam-store` now owns the first Rust storage core: a traced, allocation-owning
fact log with transaction basis, schema-as-data seeding, raw assert, `flip`,
schema-aware one/many `assert`, current-value lookup, presence/count, `basis`,
and `new`. The cross-language smoke loads this dylib directly and passes against
both the Rust and Zig IdApi floors. Workspace format, strict Clippy, Rust unit
tests, the focused Rust Store core test, and the full `zig build verify` gate are
the acceptance checks for this slice. MOP policy, historical range semantics,
image bridging, Cell/Globals protocol, and the production Store selector remain
outside N2.2b and are intentionally deferred to N2.2c–N2.4.

#### N2.2c execution record — complete, 2026-07-23

The Rust fact log now preserves the Store's history/retraction contract. Default
history-off `:one` updates replace one retained slot; history-on attributes keep
assert and retract events; history-off retraction removes the live fact; and
`:many` reads fold the newest event per member so retracting one member does not
hide the others. `hasValue:attr:` and `count:attr:` are cardinality-aware, the
trace hook retains values referenced only by historical retract entries, and the
cross-language suite covers one-slot compaction, opt-in timelines, many-member
retraction, and both selected IdApi floors. Rust formatting/Clippy, the focused
Rust Store tests, and the full repository gate are required before this commit.

#### N2.2d execution record — complete, 2026-07-23

The Rust Store now owns the Cell handle protocol: `cellFor:attr:` allocates a
traced `(store, entity, attribute)` view, while `value`, `set:`, and `history`
delegate to the Store's schema-aware protocol. A second Cell over the same pair
observes the first Cell's writes, and the cross-language test passes against the
selected floor. Cell payloads use an explicit trace hook, and all carriers are
copied before foreign sends so no Rust reference spans a callback. Globals,
MOP, image bridging, and production Store selection remain deferred.

#### N2.3a execution record — complete, 2026-07-23

The Rust Store now owns the first MOP chokepoint slice. Tracked add/remove
registers the single-provider authorizer and observer, rejects denied writes
before either representation changes, commits retract → dictionary → mirror
assert ordering, and ignores the observer's answer after post-commit
notification. The floor overwrite hook retracts stale mirror facts, and
`backfillMirror` collects only real type-vtable selectors before installing the
`#primitive` presence marker. A cross-language test covers denial, replacement,
exact removal, observer counts, raw overwrite invalidation, and both IdApi
floors. Full MOP/image/compiled-unit parity remains for N2.3c–N2.4.

#### N2.3b execution record — complete, 2026-07-23

The Rust rung now provides the exported `Globals` facade over the same Store
fact log. `at:`, `at:ifAbsent:`, `at:put:`, and `includes:` preserve absent versus
bound-`nil` semantics, and fallback blocks are invoked only for genuine absence.
The cross-language suite passes this boundary on both floors; no second global
environment or per-global native state was introduced.

#### N2.3c execution record — complete, 2026-07-23

The Rust Store now exposes honest `valueOf:attr:asOf:` reads and the live triple
window protocol: current, as-of, since, and half-open between enumeration. The
implementation folds one/many cardinality and retract events before any
callback, materializes carriers, then sends `value:value:value:` with no Rust
borrow held across re-entry. History-off overwrite transaction watermarks now
match the Store basis contract; an unavailable earlier value returns the
kinded `#historyNotRetained` signal instead of silently fabricating absence.
The cross-language suite covers current/as-of/since/between behavior, many
retraction, callback shape, and both selected IdApi floors. Rust formatting,
strict Clippy, focused Store tests, and the full repository gate are required
before this commit.

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

#### N2.4a execution record — complete, 2026-07-23

The Rust Store now implements the deterministic `attrCount:` /
`attrAt:index:` report surface. Reports preserve first-appearance order while
filtering fully retracted attributes and respecting one/many liveness; an
out-of-range index returns raw absence exactly like the Zig Store. The
cross-language test covers ordering and removal on both selected floors.

#### N2.4b execution record — complete, 2026-07-23

The Rust Store now exposes the raw image fact bridge used by the image codec.
Export copies each fact's entity, attribute, value, transaction, and assert /
retract op into the collector protocol while excluding registered type-vtable
mirror facts. Load begin replaces the fact stream; load add preserves the
record metadata; load end restores the transaction basis and raw-mode flag.
The cross-language test covers export, wholesale replacement, assert/retract
round-trip, and both selected IdApi floors. Image container policy and heap
object traversal remain in the Zig image module until the next slice.

#### N2.4c execution record — complete, 2026-07-23

The host now accepts `BIRNAM_STORE_DYLIB` as an explicit Store-module override
for integration and observation. The normal installed-module path remains the
default, while the override loads the Rust Store without changing the dynamic
rung boundary or the production default. The Rust Store also implements the
compatibility `currentValue:attr:ifAbsent:` protocol, including the distinction
between an absent value and a bound `nil` value.

With the override pointed at the debug Rust Store dylib, the full stage-one
stack passed `image-save-smoke.sh` and `repl-golden.sh` (26/26 cases), in
addition to the focused cross-language Store tests, strict Clippy, and the
stage-one build. This is an observation seam, not the N2.5 production switch;
the Zig Store remains the default until the remaining consumers and gates are
covered.

#### N2.4d execution record — complete, 2026-07-23

The Rust Store observation seam is now an executable build gate:
`test-rust-store-stack` builds the debug and release Rust Store dylibs and
runs both image-save and the complete 26-case REPL golden suite through each
one. The gate is wired into `zig build test` and therefore `zig build verify`,
while preserving the installed Zig Store as the default when
`BIRNAM_STORE_DYLIB` is unset. Both profiles and the complete repository gate
passed after the wiring change.

### N2.5 — Switch and observe

Make Rust Store the production default in one commit. Retain the Zig Store as an
explicit oracle through an observation window covering live method
replacement, image save/load, and project workflows before deletion.

#### N2.5a execution record — complete, 2026-07-23

The installed canonical `libbirnam_store.dylib` is now the Rust Store for both
debug and release builds. The former Zig implementation remains installed as
`libbirnam_store_zig.dylib`, and `BIRNAM_STORE_DYLIB` can select either path for
an explicit observation run. Stage-one compilation, the full `zig build verify`
gate, and direct image/REPL probes through the retained Zig oracle all passed.

#### N2.5b execution record — complete, 2026-07-23

The observation window is now executable in `test-rust-store-stack`. It keeps
the Rust debug/release image and REPL checks, then drives the retained Zig Store
oracle through image save/load, all REPL goldens, project resolution/build/run,
and the project-aware LSP smoke. The Rust canonical path remains covered by the
normal `zig build verify` consumers; the explicit Zig-oracle run passed all four
workflow surfaces.

#### N2.5c execution record — complete, 2026-07-23

The Store observation checkpoint is closed for the migration decision: the
canonical Rust Store passes the complete repository gate, and the retained Zig
Store passes the same image, REPL, project, and LSP workflow gate through its
explicit dylib path. The Zig implementation remains installed as an oracle for
later cross-rung comparisons; no consumer still selects it by default.

## 9. Phase N3 — PEG matcher

### N3.1 — Freeze parser behavior

Record grammar construction, every PEG operator, CST shape and child ordering,
byte ranges, failure position, recursive rule behavior, malformed grammar
diagnostics, deep/backtracking inputs, and compile-corpus performance.

### N3.2 — Port pattern and grammar representation

Implement Rust-owned patterns, grammars, CST nodes, exact GC layouts, root
handling, and current Birnam message protocol. Keep construction semantics out
of `libid`.

#### N3.2a execution record — complete, 2026-07-23

The workspace now contains a `birnam-peg` Rust `cdylib` shell with a distinct
`rust-peg-n3.2-shell` identity export and fatal null-`IdApi` boundary. It is
installed only as `libbirnam_id_peg_rust.dylib`; the Zig PEG remains the
canonical `libbirnam_id_peg.dylib`, so the shell cannot silently co-own `Peg`.
Cargo lock/install metadata, strict Clippy, the shell symbol audit, and an
isolated `zig build install` all pass. The shell exports no runtime objects yet;
N3.2b owns the first Rust pattern representation.

#### N3.2b execution record — complete, 2026-07-23

Rust now owns the PEG pattern payload and constructor/combinator protocol in the
explicit `birnam-peg` dylib. Pattern objects retain their kind, literal bytes,
child Oops, charset ranges, and rule symbols in a single traced/finalized
payload; the `Peg` facade provides `literal:`, `any`, `eof`, `charset`,
`ruleRef:`, `seq`, and `choice`, while patterns provide `star`, `plus`, `opt`,
`negate`, `lookahead`, `add:`, and `from:to:`. The grammar and matcher remain
unimplemented and the Rust dylib is not selected by the normal PEG path.

`zig build test-rust-peg-pattern` loads only the explicit Rust PEG dylib and
checks its identity, exported vtables, all constructors, combinators, charset
range mutation, and rule-reference payload path against both selected IdApi
floors. Cargo formatting, strict Clippy, the Rust unit test, and both focused
floor runs pass. N3.2c owns the Grammar representation and parse-facing state.

#### N3.2c execution record — complete, 2026-07-23

Rust now owns the Grammar payload: ordered named rules, String-or-Symbol rule
name coercion through the shared interner, per-instance furthest-failure state,
and traced/finalized rule storage. The `Peg` facade provides `grammar`,
`newGrammar`, and `anyByte`; Grammar provides `rule:is:` replacement semantics
and `furthestFail`. Grammar remains a construction-only representation: parse
diagnostics beyond the initial offset and all matching/CST behavior stay on the
Zig oracle for N3.3.

The focused Rust PEG test loads only the explicit dylib and checks both grammar
constructors, the Grammar vtable, symbol and String rule names, replacement
identity, and the initial furthest-failure value against both IdApi floors.
Cargo formatting, strict Clippy, the Rust unit test, and both focused floor
runs pass.

### N3.3 — Port the matcher

Implement the hot iterative/backtracking engine with explicit bounds and
deterministic furthest-failure behavior. No Birnam send is introduced per input
byte or backtracking step.

#### N3.3 execution record — complete, 2026-07-23

The explicit Rust PEG dylib now owns ordered choice/backtracking, literal/byte/
charset/eof terminals, sequence and repetition operators, lookahead and
negation, rule-boundary packrat memoization, zero-width repetition guards, and
left-recursion/undefined-rule diagnostics. A transient indexed CST arena is
converted into traced/finalized `Cst` objects only after matching; scratch roots
protect partially converted trees across allocation-triggered collections, and
no Rust borrow or lock spans a foreign send or callback.

The Rust identity is `rust-peg-n3.3-matcher`, while the dylib remains installed
only as `libbirnam_id_peg_rust.dylib`; the canonical PEG path is still Zig. The
focused cross-language gate covers ordered backtracking, lookahead, nested rule
spans, CST accessors, syntax-failure offsets, undefined rules, left recursion,
and both selected IdApi floors. Cargo formatting, strict Clippy, the Rust unit
test, and both focused floor runs pass.

### N3.4 — Prove downstream compiler parity

Run the unchanged Zig language and codegen rungs on the Rust PEG. Compare full
CSTs, generated C, diagnostics, module metadata, compiled-unit descriptors, and
the whole source corpus.

#### N3.4a execution record — complete, 2026-07-23

The unchanged Zig `lang` rung now runs through the explicit Rust PEG matcher in
`test-rust-peg-lang`. Its grammar construction exercises the ergonomic PEG
facade (`seqOf:`, `choiceOf:`, prefix combinators, `anyOf:`, and `range:to:`),
then parses representative lets, blocks, sends, vectors, maps, comments, and
multi-statement programs. The negative binary-operator case remains rejected.
The focused bridge passes against both selected IdApi floors, while the normal
canonical PEG path remains Zig. Full CST/codegen/diagnostic corpus comparison
is the remaining N3.4 work.

#### N3.4b execution record — complete, 2026-07-23

The unchanged Zig TCC and Codegen rungs now evaluate arithmetic, persistent
`let` bindings, and collection `inject:into:` programs through the explicit Rust
PEG matcher. The test links the existing libtcc boundary and exercises the same
runtime sends used by the production codegen path; no generated-C ABI or TCC
ownership change is hidden in this slice. The focused codegen bridge passes on
both selected IdApi floors. Full generated-C, diagnostics, module metadata, and
whole-corpus comparison remain open.

#### N3.4c execution record — complete, 2026-07-23

The complete unchanged Zig `lang_test.zig` suite can now select the explicit
Rust PEG dylib through `BIRNAM_RUST_PEG_DYLIB`; with the variable absent, the
same test target keeps its canonical Zig PEG behavior. The new
`test-rust-peg-lang-full` gate runs the existing language corpus, including its
negative syntax cases, without changing the language rung or its grammar
authority. It passes with the Rust floor selected and with the Zig floor
selected, while the Rust PEG remains an explicit test backend rather than the
canonical installed PEG.

#### N3.4d execution record — complete, 2026-07-23

The complete unchanged Zig `codegen_test.zig` suite now has the same explicit
Rust PEG override. `test-rust-peg-codegen-full` exercises the existing TCC and
Codegen corpus, including its expected syntax-error fixture, through Rust PEG
and passes with both selected IdApi floors. The test links and runtime
boundaries are unchanged; no generated-C, TCC, or Codegen ownership moved in
this slice. Full CST comparison, generated-C comparison, diagnostics/module
metadata comparison, and the whole source corpus remain open before N3.4 is
closed.

#### N3.4e execution record — complete, 2026-07-23

The frozen language corpus now has an executable CST parity gate. It walks
every successful `Lang` tree through the public CST protocol, hashing rule
names, half-open byte spans, child counts, child order, and deterministic
furthest-failure offsets for malformed inputs. The canonical Zig PEG produced
the baseline digest `0df93b8aea32fe0d`; the explicit Rust PEG reproduces that
digest with both the Rust and Zig IdApi floors selected. The gate is
`test-rust-peg-cst-parity` and leaves the canonical installed PEG unchanged.
Whole-source-corpus parity, generated-C parity, diagnostics, module metadata,
and compiled-unit descriptor comparison remain open.

#### N3.4f execution record — complete, 2026-07-23

The language diagnostic corpus now has an executable parity gate. With the
Error rung loaded, malformed source must return an Error object with kind
`#syntaxError`; its message is checked exactly as `syntax error at line L, col
C`, where `L` and `C` are derived from the same frozen furthest-failure offset
already covered by the CST corpus. The gate passes through the explicit Rust
PEG with both IdApi floors selected, and the canonical Zig PEG baseline passes
the same assertions. Undefined-rule and left-recursion error-value parity is
covered by the lower PEG protocol gate; whole-source diagnostics remain open.

#### N3.4g execution record — complete, 2026-07-23

The unchanged Zig Codegen emitter now has a frozen generated-C parity corpus.
The gate hashes the complete emitted translation unit for representative
arithmetic, persistent bindings, collection folds, literals, blocks, and
commented multi-statement programs. The canonical Zig PEG baseline is
2b8293bb3ae6f625e68060ce7a02795a9802e41e6c611b08b5b18eb637dbb0dc; the
explicit Rust PEG reproduces it with both IdApi floors selected. The same gate
can explicitly load the Zig PEG dylib to re-validate the baseline. The
generated-C ABI and TCC ownership remain unchanged; whole-source-corpus,
module-metadata, and compiled-unit descriptor comparisons remain open.

#### N3.4h execution record — complete, 2026-07-23

The unchanged downstream metadata contracts now run through an explicit Rust
PEG parity gate on both IdApi floors. It checks literal module declarations
while ignoring legacy-only source, preserves dependency order and names, and
compiles a self-describing unit whose descriptor remains inert before
activation. The descriptor assertions freeze the module kind, described
identity, ordered file/time requirements, described roots, and the
aarch64-macos target. Existing Zig Codegen and Artifact authorities remain
unchanged; whole-source-corpus comparison is the remaining N3.4 gap.

#### N3.4i execution record — complete, 2026-07-23

The complete checked-in source corpus is now covered by an executable parity
gate: 76 deterministic .birnam files under docs, examples, modules, and
src/id. For each file it hashes the relative path, source bytes, parse
success/failure, furthest-failure offset, and full CST shape. The canonical Zig
PEG baseline is bf7e0449dfd15d2f; the explicit Rust PEG reproduces it with
both IdApi floors selected. The gate supports an explicit Zig PEG baseline
rerun and leaves the installed canonical PEG unchanged. N3.4 now has its
requested downstream parity evidence; N3.5 remains gated on performance,
deep-input robustness, full bootstrap, and installed compilation.

### N3.5 — Switch and observe

Switch the PEG dylib only after parser performance, deep-input robustness, full
bootstrap, and installed compilation pass. Retain the Zig matcher oracle through
the compiler migration.

#### N3.5.1 execution record — complete, 2026-07-23

The explicit Rust PEG passes the first N3.5 robustness and installed-path gate:
an adversarial 4,096-rule chain and a 100,000-byte repetition parse without
stack overflow, with ReleaseSafe timings of roughly 3.4 ms and 1.3 ms on this
machine. The relocated installed smoke copies the complete install tree,
replaces only the canonical PEG filename with the Rust PEG dylib, and compiles
and runs a source file from another working directory. Both Rust-floor and
Zig-floor installs pass. The canonical PEG remains Zig; performance comparison
and full-bootstrap observation remain before the switch.

#### N3.5.2 execution record — complete, 2026-07-23

The build graph now accepts an explicit PEG implementation selection:
-Dpeg=rust installs the Rust matcher under the canonical PEG filename while
retaining the Zig PEG oracle and explicit Rust filename. Clean stage-one
installation with -Dpeg=rust compiled the complete installed source closure
successfully under both Rust and Zig IdApi floors. The relocated source
compile/eval smoke then passed from another working directory. The default
selection remains Zig, so this is an observation-capable switch rather than a
production default change.

#### N3.5.3 execution record — complete, 2026-07-23

ReleaseSafe stress observation recorded the Rust matcher at about 3.4 ms for a
4,096-rule chain and 1.3 ms for a 100,000-byte repetition on the Rust floor;
the Zig-floor run measured 3.4 ms and 0.8 ms. Both remain far below the
five-second fail-safe threshold. The accepted R6 performance comparator still
governs repository-level PEG/compile/eval tradeoffs; this slice adds no
unrecorded regression allowance and keeps the Zig PEG oracle available for a
paired rerun.

## 10. Phase N4 — libtcc and generated runtime shim

### N4.1 — Freeze the native-code backend

Freeze every libtcc operation, error callback, symbol binding, output mode,
library path, relocation/finalizer behavior, runtime `birnam_*` export, and
generated-code callback. The inventory assigns TCC-owned forwarding,
allocation, layout, and C-string symbols separately from Codegen-owned global,
method-mutation, candidate, quiescence, and lifecycle symbols.

#### N4.1 execution record — complete, 2026-07-23

The Rust workspace now contains a birnam-tcc dynamic-rung shell with the
frozen Tcc, TccVt, and TccUnitVt export names plus identity
rust-tcc-n4.1-shell. It is installed only as
libbirnam_id_tcc_rust.dylib; Zig TCC remains the canonical
libbirnam_id_tcc.dylib, and the shell initially exported no compile/runtime
methods.
The cross-language shell test passes on both IdApi floors, Cargo workspace
tests and strict Clippy pass, and install metadata requires the explicit shell.
N4.2 owns the unsafe libtcc state machine and runtime-symbol implementation.

### N4.2 — Port the TccUnit mechanism

Wrap libtcc in Rust with explicit ownership states: created, configured,
relocated, installed, disposable, and released. Prevent double release and
use-after-finalize without holding Rust borrows across libtcc callbacks.

#### N4.2 execution record — complete, 2026-07-23

`birnam-tcc` now compiles the vendored `libtcc.c` through an offline Cargo
build and owns a raw `UnitPayload { TCCState*, phase }` with the explicit
created/configured/relocated/installed/disposable/released transitions. The
compile, staged compile, library/file, relocation, symbol, install, disposable,
and AOT output operations are routed through that state machine; relocation is
idempotent, installed units cannot become disposable, and finalization clears
the state pointer before calling `tcc_delete`. The generated-C runtime symbol
table is also registered through the same pre-relocation window.

The Rust TCC integration test compiles and installs a native Method, proves
idempotent relocation and rejects release of an installed unit. Cargo tests,
strict Clippy, and the cross-language test pass with both Rust and Zig IdApi
floors. N4.3 closes the generated-C callback and relocatable-artifact matrix.

### N4.3 — Port runtime symbols

Export only the generated-C symbols currently owned by `tcc.zig` and route them
through `IdApi` exactly as today. Preserve raw/private method installation only
where the current compiled-unit contract permits it. The still-Zig Codegen
continues to own its disjoint symbol set until N6; no mixed stack contains two
providers for one symbol.

#### N4.3 execution record — complete, 2026-07-23

The Rust TCC rung now exports the complete current `birnam_*` set under the
canonical dynamic names and registers the same addresses in libtcc's
pre-relocation symbol window. A generated C Method calling
`birnam_mkSmallInt` compiles, binds, relocates, installs, and dispatches through
the Rust TCC module on both IdApi floors. The shell test also audits every
runtime export with `dlsym`.

The complete unchanged 50-test Codegen suite now runs with both the explicit
Rust PEG and Rust TCC dylibs and passes on the Rust floor. The Zig TCC oracle's
runtime symbols are hidden from test executables, preventing a host-export
collision from masking the selected Rust callback provider. The separate
process artifact-load check passes with no authoring-process state dependency.

The installed `BIRNAM_TCC_DYLIB` override selects the explicit Rust TCC while
leaving the canonical Zig TCC untouched. The separate-process AOT smoke compiles
an installable module with that override and activates it from a fresh process;
both Rust- and Zig-floor runs pass. This closes N4.3 without changing the
production default.

### N4.4 — Switch and observe

Require generated-C execution, callback reentry, compiled dylibs in another
process, old compiled units, Alien, image composition, and optimized compile
performance before switching. An installed symbol audit must prove exactly one
owner for every generated-C symbol before and after the switch.

#### N4.4 execution record — complete, 2026-07-23

The Rust TCC selection seam, separate-process AOT evidence, and canonical
selection gate are executable. `-Dtcc=rust` now installs Rust under
`libbirnam_id_tcc.dylib` while retaining both explicit Rust and Zig names;
`-Dtcc=zig` restores the Zig canonical path. Unknown values fail closed.

The complete 43-test Alien suite now runs through the explicit Rust TCC on both
IdApi floors, including callback, struct-by-value, library, and failure-path
coverage. Image composition and optimized compile observations remain open.

The Rust-TCC image/compiled-unit matrix passes 15 tests with the same 3
platform skips on both floors. It covers mapped compiled code, image save/load,
compiled-unit replay, and the existing image composition paths without changing
the canonical Zig TCC selection.

The installed Rust TCC audit now fails closed on its exact 22 runtime exports,
`module_init`, `@rpath/libbirnam_id_tcc_rust.dylib` install name, and allowed
Darwin dependency set. It also caught and closed the Cargo build-tree install
name before the audit was added.

The benchmark driver accepts `BENCH_TCC=rust` as an observation-only
override. A quick ReleaseSafe Rust-floor run through the explicit Rust TCC
completed the full suite; `compile.stage1-stdlib` measured 40,682 µs median
(2 reps), below the accepted R6 Rust-floor snapshot's 44,564 µs. Because this
was a quick, non-guarded run, it is not a replacement acceptance snapshot.

The canonical-selection smoke, relocated Rust-canonical REPL evaluation, and
explicit Zig rollback REPL evaluation all passed. The exact R6 acceptance gate
also remains green with only its two recorded crossings. The retained Zig TCC
is still available under `libbirnam_id_tcc_zig.dylib` for the oracle window.

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

#### N5.1/N5.2 execution record — complete, 2026-07-23

`birnam-lang` is a separate `cdylib` with the explicit
`libbirnam_id_lang_rust.dylib` identity. It builds the complete current grammar
through the PEG message surface, exports `Lang`, `LangVt`, and
`BirnamGrammar`, and preserves `parse:`, `grammar`, and `furthestFail`. The
unchanged six-test Lang suite passes with Rust PEG and Rust Lang, and strict
Clippy plus the crate unit test pass.

### N5.3 — Prove protocol and corpus parity

Compare exact CST, failure position, and diagnostics across the source corpus
and adversarial malformed inputs. Run the Rust parser with Rust PEG and with the
retained Zig PEG oracle to distinguish parser from matcher failures.

#### N5.3 execution record — complete, 2026-07-23

The frozen CST corpus digest remains `0df93b8aea32fe0d` with Rust Lang plus Rust
PEG, matching the retained Zig-Lang shape. The malformed corpus preserves
syntax-error kind, furthest offset, line/column message, and error value
identity. The existing Rust-PEG/Zig-Lang run remains available to isolate PEG
from grammar regressions.

### N5.4 — Switch and observe

Switch the canonical lang dylib only after clean from-zero compilation reaches
the unchanged Zig code generator, full verify and optimized parser gates pass,
and the §5.4 observation record is defined. Retain the Zig lang oracle through
the Codegen migration.

A Birnam grammar/compiler remains optional and additive. It cannot replace this
phase's cold parser.

#### N5.4 execution record — complete, 2026-07-23

`-Dlang=rust` now selects Rust Lang canonically while installing explicit Rust
and Zig names; `-Dlang=zig` restores the Zig canonical path. The selection gate
checks both canonical/explicit identities, runs a relocated REPL expression on
both implementations, and rejects unknown values. Rust Lang parity and
diagnostics remain pinned to CST digest `0df93b8aea32fe0d`; the Rust and Zig
oracles remain independently selectable.

The relocated install smoke now includes the Rust Lang workspace crate, and the
native-rung harness propagates `-Doptimize` into its nested builds. The focused
Rust-Lang set, optimized native-rung harness, and explicit Rust Codegen runtime
boundary pass. The optimized cross-layer closure reaches 434/440 tests with
one deterministic `modules/run-tests.birnam` `sortedModules`/`vSortBy` fixture
crash after its assertion; the R6 acceptance script quarantines only that exact
seven-marker signature and fails closed on any change or additional failure.
It then validates the unchanged digest-pinned benchmark pair and its two
explicitly accepted crossings. The default Debug relocated `docgen` recursion
remains a separate pre-existing host observation; it is not used as N5.4
acceptance evidence and does not change the ReleaseSafe gate.

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

#### N6.1 execution record — complete, 2026-07-23

The first authority slice is now executable. The Codegen test freezes the exact
47-selector `CodegenVt` surface and 12-selector `ReflectVt` surface, rejecting
both missing and newly introduced direct bindings. The generated-C parity corpus
now covers literals, sends, blocks, globals, module metadata, embedded files,
method definition/removal, and both JIT and installable-artifact translation
units. Its exact digest is
`594469bfe072d00b71ea6d2b5ead6e571d1afec7ce417a7794419844050682b2`.

The existing candidate lifecycle and image replay suites remain the behavioral
authority for those generated-C paths. The committed
`src/id/fixtures/codegen-authority-v1.json` manifest now freezes the 16
Codegen-owned dynamic exports, and `test-codegen-authority` audits the installed
dylib against it (excluding only compiler-emitted UBSan handlers). `test-codegen`
(51 tests) and `test-rust-peg-codegen-parity` pass in ReleaseSafe. The parity
projection now includes candidate-root and compacted-image translation units in
addition to JIT and installable-artifact units. The descriptor/header projection
is pinned in the same digest without normalizing semantic bytes, completing the
authority freeze.

### N6.2 — Port deterministic source emission

Port CST walking, identifier/salt generation, literal encoding, block/method
emission, and translation-unit assembly. A Rust byte buffer replaces Zig's
`ArrayList`; no O(n²) Birnam string construction is introduced.

**Gate:** emitted C text is exact for the frozen corpus before any generated C
is executed.

#### N6.2 execution record — in progress, 2026-07-23

The Rust workspace now contains a pure `birnam-codegen` crate. Its first slice
is an allocation-owning byte emitter with Zig-compatible C-string escaping,
small-integer, String, Symbol, late-bound global, and send projections. It has
no `IdApi`, loader, object allocation, or runtime dependency, and therefore
cannot silently become a second runtime authority. Two unit tests pin the
escaping and compositional output shape; `cargo test --workspace --locked
--offline` passes. The emitter now also accepts a pure expression IR with an
explicit 4096-node depth bound; deep input returns a typed error instead of
recursing without a limit. Integer emission now matches the Zig oracle's
`MKV(<literal>L)` spelling exactly. Clippy is clean, and the crate is not
selected by the production build yet.

The deterministic translation-unit assembler now emits the shared runtime
declaration prelude, baked-global handle declarations, `__ev` body, and the
artifact-only `__birnam_unit_init` fixup in separate JIT/artifact modes. Exact
unit text is pinned for both modes; workspace formatting, tests, and Clippy
remain clean.

The next N6.2 slices now lower the literal and send portions of the Rust PEG
CST through a pure `CstNode` protocol. Source-preserved integer and float
spans, Birnam string escapes, singleton identifiers, symbols, globals, binary
and unary messages, keyword selector concatenation, and same-category
left-folded chains all lower into the emitter IR. Keyword selectors use an
owned `Cow` only when their source pieces are non-contiguous; literal and
single-selector paths remain borrowed. Wrapper descent and expression lowering
have explicit 4096-node bounds, and invalid spans/shapes fail with typed
errors. The crate still has no `IdApi`, loader, allocator, or runtime
dependency. Eight deterministic Codegen tests, workspace tests, formatting,
and Clippy pass; the Rust implementation remains unselected by production
builds until the later orchestration and runtime-boundary gates are complete.

The expression adapter now also lowers vector and map literals into the exact
persistent `Vector new`/`add:` and `Map new`/`at:put:` send chains, and lowers
`?name` into `LogicVar named:` with a symbol argument. Zero-argument sends use
the Zig ABI's `0, 0L` null-argument representation rather than an empty
compound literal. The nine Codegen tests and full Cargo workspace suite pass
after this slice.

Block lowering and emission are now covered by the pure seam as well. The
adapter preserves parameter order, statement order, local `let` bindings, and
nested expressions. The emitter computes captures in enclosing-scope order,
swaps a bounded lexical scope while emitting nested block functions, restores
that scope on both success and failure, and emits the Method-ABI function,
arity-specific `value` selector, block vtable, precise Oop layout, captured
payload, and last-statement return. Nested function definitions are emitted in
post-order and failed nested emission rolls back their definitions. Ten
Codegen tests and Clippy pass; runtime orchestration and actual dylib
selection remain intentionally deferred to N6.3+.

The CST root/body seam is now explicit: `program` lowers to an ordered
statement IR, and `Emitter::emit_program` emits local bindings and the final
body value transactionally while preserving nested block definitions. It does
not yet perform persistent global writes or runtime error publication; those
remain deliberately gated under N6.3 orchestration. Eleven Codegen tests and
the full workspace suite pass for the current pure crate.

The first N6.3 runtime boundary is now executable but remains opt-in. The
installed `birnam-codegen-runtime` dylib exports `RustCodegen emit:` and
`RustCodegen eval:` without claiming the frozen `Codegen` authority. Both
methods parse through the live Rust PEG object, materialize a bounded owned CST,
and lower through the pure emitter. `eval:` then drives the explicit Rust TCC
object through `new`, `compile:`, `bindRuntime`, `relocate`, and
`install:as:on:` before dispatching the generated `__ev` Method. The dedicated
IdApi test now passes both deterministic emission and scalar native evaluation
(`7 -> 7`) with Rust PEG and Rust TCC loaded from installed paths. It also
invokes an emitted identity block immediately (`[:x | x] value: 4 -> 4`).
Block-bearing evals now register their TCC unit through an IdApi GC root
provider and survive a collection probe; scalar evals resolve `__ev` through
TCC's existing `symbol:` ABI, invoke it without installing a method, and mark
their unit disposable. A repeated scalar/GC probe covers that cleanup path while
preserving the TCC invariant that installed units cannot be released. Persistent globals, error
publication, compile-to-dylib output, metadata, candidate lifecycle, and image
recovery remain N6.3–N6.6 gates.

The same opt-in runtime now exposes `RustCodegen compile:output:`. It emits an
artifact-mode translation unit and delegates `compile:output:as:` to the Rust
TCC object; the installed-path test verifies a non-empty dylib is written for
the scalar `7` program. This proves the output boundary only. The artifact has
not yet acquired compiled-unit metadata, embedded source, dependency
descriptors, runtime initialization, or loader acceptance, so it cannot enter
the production Codegen authority or the N6.4 image/module path.

The first artifact metadata slice is now present. Rust compile output derives
the module identity from the `libbirnam_<name>.dylib` output convention, then
prepends the frozen `birnam-artifact-generation-v1` ABI marker and a
pointer-free `BIRNAM-ARTIFACT-DESCRIPTOR-V1` record with that identity,
aarch64-macos target, empty dependency lists, the activate/source/reflection
capabilities, and deterministic SHA-256 source/revision digests. It also exports the original
source bytes plus a length symbol for source fallback. The installed runtime
test opens the resulting dylib and verifies the ABI, embedded source, and
descriptor symbols before any initializer or entry point is invoked. Dependency
derivation, runtime initialization, and loader acceptance remain N6.4 work.

The opt-in runtime now also exposes `RustCodegen load:` for that artifact
boundary. It opens with lazy/global resolution, validates the ABI marker before
calling `__birnam_unit_init`, runs the allocation-free Rust artifact inspector
over the descriptor bytes and length, dispatches the frozen `__ev` Method ABI,
and retains successful handles for process lifetime. The installed gate
compiles `7`, verifies the descriptor symbols, loads the dylib through Rust
Codegen, and observes `7` again. Host-capability policy and image-generation
handoff remain open.

The diagnostic seam is also executable: every opt-in emit/eval/compile-output
operation clears a process-local `lastError`, records a first failure message,
and exposes it through `RustCodegen lastError`. Parse failures now query Rust
Lang's furthest-failure offset and publish `syntax error at byte N`; lowering
and TCC failures retain their specific boundary messages. The focused test
proves a syntax diagnostic and a subsequent successful `7` evaluation clears
it. This is intentionally a string-compatible observation seam, not yet the
full Error-object propagation and source-span formatting required by N6.3. The
compile-output test also proves a lowering failure returns `nil` without
creating the requested dylib.

The generated-C global-read path is now wired through the installed Rust Store
without adding a second global environment. `RustCodegen` imports the live
`Globals` singleton, exports a narrow `birnam_global(const char*)` shim, and
binds that symbol through the Rust TCC unit's `addSymbol:value:` boundary before
relocation. The shared C prelude declares the shim explicitly, avoiding an
implicit-function ABI mismatch on aarch64. The installed six-test runtime gate
now stores `rust_codegen_answer` through `Globals at:put:` and proves that
`rust_codegen_answer` evaluates to `41`; the existing scalar, block/GC,
artifact-cleanup, and diagnostic checks remain green. This closes the global
read portion of N6.3; full Error-object propagation, source-path metadata,
candidate lifecycle, and image recovery remain open.

Persistent top-level bindings now use the same singleton as reads. The pure
emitter has an explicit persisting mode that emits `birnam_global_set` only for
top-level `let` statements; block-local bindings remain lexical. The opt-in
runtime exposes `evalPersisting:`, binds the setter through TCC's pre-relocation
symbol window, and the installed gate persists `rust_codegen_persisted := 43`
then reads it back through ordinary `eval:`. This closes the global write
portion of N6.3 without introducing a second environment or changing the
non-persisting evaluator.

The first source/file portion of N6.4 is now executable at the explicit Rust
boundary. `OwnedCst` can carry compiler-owned byte payloads, and the runtime
rewrites only the canonical `[System embedFile: "relative/path"]` CST send into
that synthetic literal after validating the path and reading at most 16 MiB.
`eval:from:` and `evalPersisting:from:` anchor resolution at the source
document directory; `compile:output:from:` embeds the same bytes in the dylib.
The installed gate evaluates a binary asset, compiles it, deletes the authoring
file, and loads the artifact successfully with the original bytes. This keeps
filesystem access outside the pure emitter and leaves project-root embedding,
dependency-list derivation, host-capability policy, and image handoff for the
remaining N6.4 work.

Rust artifact descriptors now also derive static module metadata from the Rust
PEG tree: `Module named:requires:` supplies the identity and Birnam dependency
symbols, while canonical `Alien dlopen:` sends supply distinct native-library
strings. The existing inert Artifact inspector verifies the resulting counts
and identity before any initializer runs. Descriptor fallback still uses the
output filename for source without a module declaration; source/revision
digest enrichment and capability-policy enforcement remain subsequent N6.4
slices.

Rust `load:` now applies the native-capability half of that handoff before
initialization: each descriptor entry must be a basename and must exist beside
the artifact, otherwise loading returns `nil` with a stable diagnostic and no
initializer or `__ev` call. The gate deletes a declared native fixture and
proves this pre-init rejection. Birnam dependency activation and image replay
remain outside this narrow loader check.

The Rust boundary also has an explicit `compile:output:root:` form. It shares
the embedded-file lowering with `compile:output:from:`, but treats its third
argument as the project root rather than deriving a directory from a source
document. The gate proves a nested source path fails under source-directory
resolution while the declared root compiles, embeds, and reloads the asset.

`RustCodegen embeddedFilesOf:` now provides the corresponding allocation-free
static analysis surface. It walks the Rust PEG CST, returns distinct decoded
literal paths in source order, and does not read files or mutate Store state;
the runtime test covers duplicates and nested blocks. This is the input needed
for project-level asset digests before full candidate/image orchestration.

The loader compatibility seam now covers both compiler directions. The
installed gate compiles a scalar artifact with Zig Codegen and loads it through
Rust Codegen, then compiles with Rust Codegen and loads through the unchanged
Zig Codegen loader. Rust accepts the older Zig descriptor framing by deriving
the payload length from the mandatory magic/header when Zig's optional length
symbol is absent; both directions return `7` through the frozen Method ABI.

The opt-in Rust boundary now exposes `moduleMetadataOf:` as well. It returns
the same five-field metadata projection used by project indexing for a literal
`Module named:requires:` declaration, including deterministic source/revision
digests, and answers `nil` for legacy source without that declaration. It is
analysis-only: no source is evaluated and no Store facts are changed.

`sourceRevisionOf:` now exposes the corresponding seven-field revision value
(`name`, requirements, authored source, and three deterministic digest slots)
for the same literal module shape, with `nil` for legacy source. Its bounded
top-level scan now partitions non-method statements, direct declarative method
operations, and the complete authored revision into the three digest slots;
candidate compilation and Store publication remain part of N6.5.

The first Rust source-recovery seam is now executable as well. `RustCodegen`
implements `recordModuleSource:named:`, `moduleSourceFor:`, and
`sealInitialGeneration`. Registration re-parses the source metadata, enforces
that the declared module identity matches the supplied symbol, rejects
conflicting duplicates, and retains both the symbol and authored source through
the IdApi GC root-provider contract. Lookup still returns the original source
object after collection; sealing makes subsequent introductions fail closed.
This is intentionally a Rust-only opt-in surface: it does not replace Zig's
`Codegen` registry or claim the N6.5 dispatch lifecycle.

Registration now consumes the same strict top-level revision scan as
`moduleMetadataOf:` and `sourceRevisionOf:` rather than the looser recursive
artifact walk. A fixture with duplicate module declarations proves that a
later declaration cannot enter the registry under an otherwise matching name.

`initialGenerationProjection` now consumes that registry into the frozen
three-field generation projection. Entries are sorted by module name, each
entry carries the Rust source revision and a nil unit slot, and the dispatch
generation digest uses the same length-delimited name/revision-digest framing
as the Zig authority. The projection is read-only and remains opt-in; successor
replacement, Store publication, and rollback stay in N6.5.

The corresponding read-only successor projection is now present at
`successorGenerationProjection:replacing:with:unit:`. It replaces exactly one
existing module entry, enforces the same name/requirements/non-method revision
transition rule as the Zig projection, preserves untouched entries, and
recomputes the canonical generation digest. Duplicate names, missing modules,
malformed vectors, and top-level-evolution attempts fail closed. It still does
not publish a Store generation or mutate dispatch; those are deliberately
reserved for the transactional N6.5 implementation.

The projection boundary now validates the complete seven-field revision shape:
the tag, symbolic module identity, symbolic requirements vector, authored
source String, and all three 64-character hexadecimal digest Strings. It also
requires each source-map entry name to equal its revision name and type-checks
String payloads before calling the invariant-enforcing IdApi byte accessor. A
regression fixture proves that a partially malformed successor revision is
rejected without recomputing or publishing a generation. The digest projection
also rejects duplicate module names among untouched entries, with a separate
fixture covering that non-target collision.

The partition is pinned by a 15-test runtime gate: changing only a method body
leaves the non-method digest unchanged, while changing only a top-level form
leaves the method digest unchanged. The source, identity, and requirements
digest still changes for either edit.

`moduleMetadataOf:` now consumes the same static revision scan: its source
digest remains the complete authored-source identity, while its revision slot
matches `sourceRevisionOf:`. This keeps project indexing and lifecycle
projections on one digest definition without evaluating source or touching
Store state.

Rust artifact descriptors now carry that same partition: indexed module units
write the complete source digest and static revision digest separately, while
legacy/non-indexable sources conservatively use the source digest in both
slots. The installed descriptor gate proves the two values diverge for a
module-bearing source with non-method content.

The first N6.5 candidate slice is now independent and executable in Rust:
`birnam-artifact::candidate::inspect` parses the frozen
`BIRNAM-CANDIDATE-2` sidecar without allocation, dylib loading, or IdApi state.
It validates the runtime ABI, candidate ABI 3, aarch64-macos platform, exact
64-character artifact digest, manifest framing, and byte-exact source length.
The same boundary now parses the `v2` pipe-delimited manifest and exposes
borrowed dependency and operation rows with target, selector, kind, and arity
validation; envelope inspection invokes this manifest validation before
returning. Eleven artifact tests (including adversarial malformed-input probes)
pass. This is only the inert candidate boundary; candidate compilation,
mapping, Store publication, rollback, and image composition remain unported
N6.5/N6.6 work.

The Rust Codegen runtime now has a pure candidate-manifest parity seam as well:
its static revision scan retains declarative operations, and a test compares
candidate name, source/non-method/method/revision digests, dependencies, target
and selector names, operation kind, and selector arity against the manifest.
This helper is not yet a loader or publication path; it exists to make the
later `inspectCandidate:into:` boundary compare two independently parsed
representations before any mapping or Store mutation.

That boundary is now executable. Rust `inspectCandidate:into:` reads and
validates the detached sidecar, hashes the candidate dylib, applies the
pre-`dlopen` thin-arm64/no-init/libSystem Mach-O policy, reparses the authored
source through Rust Lang, and compares every manifest digest, dependency, and
operation row. Only after those checks does it assert the inert candidate facts
(`candidateArtifact`, source/revision digests, requirements, and `mapped =
false`) into the selected Store and retain the unit/name/source as GC roots.
The installed gate inspects a Zig-produced candidate and proves the facts are
present while no native mapping occurs. The next slice now implements Rust
`mapCandidate:`: it gates Birnam dependencies through `Module isLoaded:`, copies
the verified artifact and sidecar to an exclusive checksum-addressed
`.birnam-candidates` path, rechecks the published bytes, then opens the copy
with lazy/global resolution. Before retaining the handle it validates the
artifact ABI, exact exported candidate manifest, all lifecycle hook symbols,
the generated initializer, the exported validator, and remove-operation
bindings; failed attempts close the handle and publish no mapped state. A
successful map records the hook addresses and asserts `mapped = true` in Store
without activating methods. The installed Rust runtime gate now inspects and
maps a Zig-produced candidate and verifies that transition. Candidate prepare,
activation, rollback, and Store observer/authorizer integration remain open
N6.5 slices.

The first explicit Rust lifecycle token slice now follows mapping. Rust
`prepareActivation:` revalidates the mapped candidate, calls its exported
prepare hook while the artifact's preparation mode suppresses dispatch
mutation, and returns a rooted process-local token. `commitPrepared:` consumes
that token, calls the retained activation hook, marks the unit active and
rollback-available in Store, and only then releases the token root;
`discardPrepared:` consumes a prepared token without activation. The installed
gate now proves inspect → map → prepare → commit, observes the active Store
fact, and dispatches the newly installed method. Explicit publication plans,
quiescence, publication events, and the full Rust Store observer/authorizer
matrix remain subsequent N6.5 hardening slices.

Preparation now also captures each manifest operation's exported/global target,
owned vtable, and raw prior method carrier without holding a Rust borrow across
IdApi sends. Global lookup follows the frozen late-bound rule: Store bindings
win, then exported prototypes such as `Integer` are imported. These snapshots
are the native inputs for the upcoming exact rollback and drift checks.

The symmetric rollback token path is now executable for a single active
candidate. `prepareRollback:` compares every current raw method carrier with
the recorded post-activation carrier and fail-closes as `drifted`, disabling
rollback when an external mutation intervened. `commitPreparedRollback:` then
restores prior carriers in reverse manifest order through Store's tracked
`addMethod:on:with:`/`removeMethod:on:` surface, marks the unit `rolledBack`,
and consumes the token. The installed gate now proves active dispatch, exact
rollback, and the resulting Store state. Quiescence, publication events,
observer/authorizer callbacks, and multi-generation reconciliation remain open.

Predecessor-aware composition now extends the token boundary without changing
the single-owner mutation law. `prepareActivation:` accepts an optional active
prior unit, rejects self or inactive priors, and retains the prior identity in
the prepared token. Commit asserts `successor.supersedes = prior`, marks the
prior `superseded`, and deactivates its Rust registry entry only after the new
candidate's activation and post-carrier checks succeed. Rollback captures that
predecessor, restores the exact prior method carriers, reasserts the prior
`active` state, and reactivates its registry entry. The installed gate now
exercises A → B → rollback B → A, including the Store state transitions and
observable dispatch result.

The Rust boundary also accepts the frozen eight-field publication-plan shape
through `prepareActivation:plan:` and `prepareRollback:plan:`. It copies the
event, basis, generation handles, and predecessor identity into the rooted
prepared state; commit requires bootstrapping or quiescence, asserts the event
facts, and consumes the plan token only after the exact MOP mutation succeeds.
The installed gate exercises both explicit plan selectors. A process-local
current-generation fallback rejects stale plans at preparation and commit. When
the durable `dispatch.activeGeneration` fact is present, Rust additionally
requires canonical source-map/base-digest projections, compares the plan's
expected generation to that Store fact at both boundaries, and advances the
Store cell only on a successful publication. Activation and rollback also
validate the expected/successor entry topology: predecessor and delta-unit
facts must point at the units named by the plan, so a canonical digest cannot
mask a structurally inconsistent generation. The rooted prepared token carries
that identity, and commit repeats the topology check after the quiescence
barrier before invoking the native hook.

Rust `reconcileCandidate:` and `reconcileAllCandidates` now expose the same
post-carrier check without requiring a rollback attempt. They return the unit
or `true` when every active binding still matches, and otherwise mark the
candidate drifted and disable rollback. The installed gate covers both healthy
single-candidate and all-candidate reconciliation; persisted generation
reconciliation remains later work.

The first Rust image-source handoff is now executable. `generationSources`
topologically orders the retained module sources by their declarative
requirements, rejects cycles or malformed revisions, and remains deterministic
within a generation. `generationMethodSources` extracts only install operations
in that same order, while `currentGenerationDigest` reports the process-local
published successor digest when an explicit plan has committed (or the durable
generation's canonical `baseDigest` when Store authority is present); it fails
closed if the durable source-map digest is inconsistent. These methods are
read-only source composition. Rust `replayMethods` now provides the first
image replay boundary: it is accepted only during `beginImageDispatchRecovery`,
replays each indexed define/remove operation in dependency order, and fails
closed on malformed source or compilation failure. Alien replay, global/method
target derivation, mapped-unit composition, and full durable generation
projection/evolution authority remain N6.6 work. Rust `replayAliens` now adds
the corresponding recovery-gated journal boundary: it reads ordered
`__alien_<n>` Store facts, re-evaluates valid String sources, and continues past
individual failed wraps without publishing a partial success as a hard error.
Rust also exposes recovery-gated `deriveImageGlobals` and
`deriveImageMethodTargets`: the former snapshots and re-evaluates only currently
unbound `__globalsrc__` facts, while the latter walks ordered method targets and
materializes their named receivers through that same source journal.

`compileImageCode:` now consumes the ordered Rust generation-source handoff and
emits one TCC-backed dylib at the requested output path. When durable
`dispatch.activeGeneration` exists, it now rejects a source registry whose
canonical digest does not match that authority before emitting code. It is
intentionally a source-backed image compiler: mapped-unit reuse, replay of
durable image facts, global/method derivation, and explicit rebuild policy
remain subsequent N6.6 slices.

The first pure image-codec slice is now present in `birnam-image`. It validates
the v4 magic/version/endian/architecture/ABI header and iterates borrowed
little-endian record envelopes with checked lengths and known record kinds.
Typed borrowed parsers now also enforce the exact 64-byte authority digest and
the 72-byte code-record prefix, exposing the checksum, generation digest, and
opaque unit bytes without allocating or copying. They perform no `IdApi`
allocation or heap/Store interpretation; hash verification, dynamic loading,
and heap/fact payload materialization remain later slices built on this framing
boundary. The field iterator now covers all ten v4 tags (including null Oop,
heap-reference, and exact float bits) with checked length prefixes and no
allocation; it is a format decoder, not yet the loader's swizzling authority.
The borrowed facts iterator now validates the 12-byte basis/count prefix,
walks each `op`/transaction and three tagged values, and rejects truncated or
trailing payloads. It still leaves heap-reference resolution and Store
mutation to the later runtime handoff. A layout-driven heap iterator now
validates each vtable-name/word-count header and consumes Oop fields, raw
machine words, and elided code slots without copying; the caller supplies the
live layout after peeking the vtable name. It returns borrowed encoded bytes,
not materialized objects, so vtable relinking, heap-reference swizzling, and
GC integration remain later work. The pure crate now also exposes whole-image
preflight: it requires one authority, rejects duplicate or out-of-order code,
and verifies each code record's generation digest against that authority before
returning a re-iterable borrowed record stream.

The mapped-candidate observation surface is complete for this slice as well:
`candidateMapped:` reports the retained native handle, while
`candidateContentPath:` materializes and verifies the immutable checksum path
and returns it as a String. Both paths stay read-only with respect to dispatch;
they are intended for the later loader/image handoff.

The Rust runtime now also carries the Store callback seam needed by the final
authority switch. Its managed-method observer watches active candidate
bindings and marks `drifted`/`rollbackAvailable = false` when a post-activation
raw carrier changes; candidate-owned activation and rollback mutations are
guarded from self-observation. The Rust authorizer now has an explicit
bootstrapping → running → quiescent phase machine: ordinary running-phase
mutations fail closed with `dispatch mutation requires quiescence`, while
candidate-owned commits, bootstrap, quiescence, and image recovery are
authorized. `enterRunning`, `beginQuiescence`, and `endQuiescence` expose the
same transitions through the Rust Codegen surface. In a Rust-only stack, the
first Rust candidate map conditionally installs this observer and authorizer;
a mixed stack leaves the existing Zig Codegen callback owner untouched,
  honoring the Store single-registration law. The durable currentness and
  canonical-projection checks now run in Rust publication plans; the full
  Zig↔Rust callback matrix and cross-process generation/evolution authority
  remain later hardening gates.

The image handoff now has explicit Rust `beginImageDispatchRecovery` and
`endImageDispatchRecovery` methods. The begin gate suppresses candidate
activity/observer reactions while the image loader relinks durable state; the
end gate releases that guard. This is only the dispatch-recovery boundary—the
portable heap/fact codec, mapped-unit composition, and source replay remain
N6.6 work. `replayMethods` now preflights every indexed operation before
executing the first one and rejects nil/Error results during execution, so a
malformed later source cannot be accepted as a clean replay or silently leave
the replay count overstated.

The opt-in Rust Codegen `preflightImage:` boundary now consumes the pure image
codec and validates the v4 header, authority/code relationship, record
envelopes, and every facts record's count and tagged-field framing before
returning success. Invalid magic and malformed fact counts fail with a
diagnostic and no runtime mutation. The method deliberately does not resolve
symbols or exports, materialize heap objects, swizzle references, map code, or
touch Store; those remain the mapped-unit and recovery handoff work in N6.6.

The pure codec now also verifies each code record's unit bytes with the exact
Zig Wyhash v4 checksum before an image is considered preflightable. Authority
mismatch is reported before checksum failure, duplicate-code ordering remains
fail-closed, and the Rust test vectors match Zig's implementation. The loader
still does not map or retain the unit at this boundary. The complete R6 gate
was rerun on current head `ff67918` after correcting the fourth Wyhash secret
and adding the remaining long-input reference vectors: the 18-test focused
boundary, optimized closure, exact fixture quarantine, and digest-pinned
comparator all remained green with no additional performance exception. The
workspace image tests and clippy also pass, and the comparator retains zero
missing/unusable cases plus only the two accepted R6 crossings.

The first mapped-unit handoff is now executable at `d8a0787`. Opt-in Rust
`mapImageCode:` requires `beginImageDispatchRecovery`, re-parses the v4 image,
validates facts and the code checksum, stages the borrowed dylib bytes at a
unique temporary path, and routes that path through the existing Rust artifact
loader. The gate proves the recovery guard rejects an out-of-band map and that
the captured unit executes (`7`) without source recompilation. Heap/fact
materialization and Store replay remain outside this narrow slice. The focused
Rust Codegen target is now pinned at 19/19 tests; this is functional coverage
only and does not widen the R6 performance exception.

The scalar fact replay slice is now executable at `89c6283`. Opt-in Rust
`replayImageFacts:` requires image recovery, validates every facts record before
mutating Store, materializes scalar/symbol/string/export fields, rejects heap
references with a stable diagnostic, then performs the existing
`imageLoadBegin`/`imageAddFact...`/`imageLoadEnd:raw:` sequence and mirror
backfill. The gate reads the restored value (`42`) through Rust Store. Heap
materialization, relocation, and full image authority remain later N6.6 work;
the focused Rust Codegen target is now pinned at 20/20 tests.

The complete R6 acceptance script also passed at `89c6283`, including the
20/20 focused target, the documented `sortedModules` fixture exception, and
only the two existing benchmark crossings (`query.aggregate` and
`compile.stage1-stdlib`).

The fail-closed heap-reference regression landed at `5faf0b3`: a rejected
facts record is checked for the stable diagnostic and an unchanged Store
before the scalar replay runs. R6 passed again at that checkpoint with the
same 20/20 focused target and benchmark exceptions.

The source-journal recovery test landed at `2586925`: Rust replay now has
executable coverage for a recorded alien source and for deriving an unbound
scalar global from `__globalsrc__` during image recovery. The complete R6 gate
passed again at that checkpoint with the same 20/20 target, fixture exception,
and two benchmark crossings.

The malformed-alien continuation regression landed at `90d5181`: replay skips
one invalid recorded source without losing the valid neighboring replay. R6
passed again with 20/20 focused tests, the documented fixture exception, and
only the two accepted benchmark crossings. Commit `bc56a67` hardens the
best-effort boundary by clearing the stale diagnostic after a partial success;
malformed entries are skipped, the valid replay count is preserved, and
`lastError` is clear. The complete R6 script passed again at that checkpoint
with 20/20 focused tests, the same fixture exception, and exactly the two
accepted crossings (`query.aggregate +5.8%` and
`compile.stage1-stdlib +11.9%`).

The scalar image-field coverage landed at `507522f`: the existing 20-test Rust
Codegen recovery case now replays and reads back float, boolean, symbol,
String, and nil values through Rust Store while retaining the heap-reference
fail-closed assertion. R6 passed again at that checkpoint with 20/20 focused
tests, the same fixture exception, and no change to the accepted benchmark
snapshot.

Export-reference recovery coverage landed at `20444bb`: the same scalar image
fixture now resolves an `ExportRef("Store")` through the frozen import/export
surface and verifies the live Store identity. R6 passed again at that
checkpoint with 20/20 focused tests, the documented fixture exception, and the
same two accepted benchmark crossings.

The follow-up heap regression at `e653bf4` covers cumulative relocation IDs: a
`VectorNode` points forward to a `RangeVt`, raw words survive the second-pass
fill, and a fact can use the relocated object as its Store entity. R6 passed
again at that checkpoint with 20/20 focused tests and the same accepted
benchmark snapshot.

Heap recovery landed at `f8ec3e7`: Rust Codegen now parses cumulative v4 Heap
records, imports each named vtable, allocates layout-sized objects, fills Oop,
raw, and code slots in a second pass, and swizzles `HeapRef` fields before the
Store load begins. An unresolved reference still fails closed without Store
mutation. The focused Rust Codegen target remains 20/20; the complete R6 gate
passed again at that checkpoint with the documented fixture exception and the
same two accepted benchmark crossings. This closes the first heap/fact
materialization slice of N6.6; generation authority, multi-image policy, and
full compatible image convergence remain later N6.6 work.

Method replay preflight landed at `4fe24b2`: Rust Codegen emits every recorded
operation before evaluating the first one, preventing a later generated-C
failure from leaving a partially applied method replay. The R6 gate's focused
output check was aligned with the aggregate Zig build at `b20ccb5`; the
complete acceptance script passed with the 20-test Rust Codegen boundary, the
documented fixture exception, and only the two accepted benchmark crossings.

Malformed heap framing coverage landed at `b0c9245`: the Rust materializer now
has a regression proving that a Heap record whose object count exceeds its
payload fails closed before `imageLoadBegin` or any Store mutation. The focused
Rust Codegen target remains 20/20, and the complete R6 gate passed again with
the documented fixture exception and the same two accepted crossings.

The positive heap fixture now splits its objects across two Heap records at
`80a52bd`. This exercises cumulative relocation IDs across record boundaries,
including a forward Oop reference and raw-word preservation; the focused Rust
Codegen target remains 20/20 and the complete R6 gate passed again with the
documented fixture exception and the same two accepted crossings.

Code-slot reconstruction was hardened at `6c85f4f`: because v4 elides code
payloads, Rust now writes zero into each code slot during the heap second pass,
matching the Zig loader until mapped-unit recovery supplies executable carriers.
The focused fixture covers the code, Oop, and raw layout kinds; Cargo/clippy,
20/20 Rust Codegen tests, and the complete R6 gate remain green.

Invalid fact-operation coverage landed at `d1ff90a`: Rust now rejects
operation bytes outside assert/retract before `imageLoadBegin`, matching the
Store ABI's abort boundary while keeping malformed images fail-closed. The
focused Rust Codegen target remains 20/20 and the complete R6 gate passed
again with the documented fixture exception and the same two accepted
crossings.

The replay path now honors v4's explicit little-endian encoding for basis and
transaction words at `570d3e3`, rather than relying on the host byte order.
Cargo tests/clippy, the focused 20/20 target, and the full R6 gate all passed;
the accepted benchmark crossings remain unchanged.

The heap fixture at `8fe6b6c` now also covers a `NullOop` slot in a materialized
object, alongside forward Oop relocation, raw words, and elided code slots.
The focused Rust target remains 20/20 and R6 passed again with the same two
accepted crossings.

Rust image replay now rebuilds the relational rule cache at `bf54131` when the
`db` surface is present, matching the Zig loader after facts and the MOP mirror
are restored. The focused target and full R6 gate passed with no benchmark
change.

The Rust artifact loader now enforces declared Birnam module requirements
before `__birnam_unit_init` or `__ev` can run. A missing `Module` policy surface
or unloaded requirement fails closed, closes the `dlopen` handle, and leaves
the generated global untouched; all post-open ABI, descriptor, native-library,
and entry-point failures use the same cleanup path.

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
