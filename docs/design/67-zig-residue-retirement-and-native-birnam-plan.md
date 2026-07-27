# 67 — Zig Residue Retirement and Native-Birnam Completion Plan

> **Status:** Reviewed GO for P0 at `d2be5ad` (2026-07-27). Design 61's Rust
> floor, Design 62's Rust native rungs, Design 65's residual-owner audit, and
> Design 66's Z0–Z4 evidence are the prerequisites. This plan does not
> authorize a provider switch or delete a rollback fixture until its phase gate
> and a separate focused commit are green.

## 1. Objective

Finish the migration of the Zig residue without creating a second runtime
authority. “Finish” has three valid outcomes:

1. a native Birnam policy surface remains the sole owner when the behavior is
   language policy or protocol;
2. a Rust implementation owns observation, unsafe ABI, filesystem/process/FFI,
   or another irreducible mechanism behind the existing dynamic boundary; or
3. a Zig file remains explicitly classified as build-only, shared contract,
   rollback/oracle, or test evidence, with no canonical production path able to
   select it implicitly.

The plan is therefore a disposition and retirement plan, not a blind
line-for-line translation. The current repository has 88 tracked Zig files:
20 oracle/build residue, 64 evidence files, three shared-contract files, and
one build driver. The authoritative inventory is `abi/remaining-zig-v1.json`
and its checker. Its `dynamic_dependencies` field names only runtime dynamic
libraries; source paths, Rust crates, static TCC objects, test executables, and
native Birnam sources are classified separately in the file graph and the
checked residue-edge table introduced by P0.

## 2. Constitutional constraints

These rules are acceptance laws for every phase.

### 2.1 Bootstrap language neutrality

The toolchain-specific implementation language is not constitutional. Rust,
Zig, C, or a later native Birnam compiler may implement a mechanism, but every
clean archive must build the complete production closure from source using only
declared non-Birnam stage-0 toolchains and dependencies. It may not consume a
prebuilt Birnam executable, image, compiled dylib/module, or generated Birnam
output as input. A native-Birnam compiler or build driver is allowed only when
that compiler/driver is itself built from source by this cold chain. “No Zig
compiler” means no Zig *Birnam compiler implementation* or runtime provider;
the Zig toolchain may orchestrate the build until a separately reviewed
toolchain-retirement plan exists.

### 2.2 One floor and dynamic-only providers

One process has one initialized IdApi, heap, interner, Store authority,
generation registry, and canonical family. Every runtime rung remains one
dynamic library with its existing `module_init(IdApi*, void*)` boundary. The
inventory and all new receipts may name only installed dynamic provider names
under `<prefix>/lib`; they must not treat source files, Rust crates, static
objects, or test binaries as runtime dependencies. A rollback fixture is loaded
only by a named oracle/rollback prefix or isolated test process.

### 2.3 Frozen contracts

IdApi layout and append-only evolution, Oop/vtable placement, payload layouts,
generated-C symbols, compiled-unit descriptors, image-v4 framing, Isolate
wire-v1, diagnostic identities, and dynamic install names remain unchanged.
No phase may fold a rung into the floor, siphon, host, or another rung.

### 2.4 MOP consistency law

The object dictionary is dispatch authority. Store owns its fact timeline; its
MOP mirror is derived. For every direct `(vtable, selector)` binding the mirror
is exactly the live Method, `#primitive` for an unreified direct binding, or
absent. It may not name another Method, copy inherited methods into direct
slots, or become a second method registry. Store is the sole supported
post-bootstrap semantic mutation chokepoint: its authorizer runs before any
dictionary or fact mutation, and its observer runs exactly once after the
completed mutation and mirror update. Raw mutation is bootstrap/private only
and may degrade the mirror only to honest absence. Addition is
`mutate → assert`; backfill covers direct bindings only. Replacement retracts,
mutates, then reasserts; removal retracts, then removes. Relational reads
consume Store facts without becoming authority. Any candidate port that
cannot prove this law stops before provider selection.

### 2.5 Unsafe and re-entry rules

No Rust borrow, lock, temporary pointer, or `&mut` crosses a resolved Method or
`doesNotUnderstand:` send, mutation hook, root provider, trace hook, finalizer,
module initializer, TCC call, Alien callback, or loader call. Copy callback and
pointer values out, end the Rust borrow, call foreign code, then reacquire;
callback-visible native objects remain rooted for the callback lifetime. The
collector mark callback is synchronous/dynamic-extent and cannot escape.
Process-lifetime handles remain loaded for every live Method, layout, and
callback pointer. C boundaries abort on violations and never unwind a Rust
panic. Failed parse, compile, map, install, publication, or rollback is
fail-closed and leaves dispatch, Store, and filesystem state unchanged.

### 2.6 Native policy ownership

`*.birnam` modules remain the authority for language policy, message protocol,
and derived orchestration. Rust may observe, validate, perform IO/FFI, and
enforce atomicity; it may not shadow a native policy method in a competing
vtable or add a second message authority. A proposed native-Birnam move must
name the old owner, new selector/protocol owner, and proof that no Rust branch
still chooses the result.

## 3. Residue disposition map

| Surface | Current status | Target disposition | First proof |
| --- | --- | --- | --- |
| `src/id/main.zig` | Zig host oracle | retain until a separately reviewed release retirement; Rust `bin/birnam` remains canonical | Rust host load trace and relocated project/LSP |
| `project.zig`, `project_build.zig`, `project_check.zig`, `runtime_resources.zig` | Zig oracle/build helpers | Rust owns canonical observation; retain only for oracle/build until a replacement recovery decision | helper-boundary audit + source bootstrap |
| `artifact.zig` | shared contract | retain schema/ABI contract; pure Rust codec is canonical | descriptor parity and manifest checker |
| `artifact_inspector.zig`, `artifact_preflight.zig` | Zig oracle/build | Rust host owns closure/preflight permission; `Module loadArtifact:` owns activation/publication | inert negative corpus, zero activation, sentinel |
| `id.zig`, `store.zig`, `peg.zig`, `lang.zig`, `tcc.zig`, `codegen.zig`, `prelude.zig`, `collection.zig`, `error.zig`, `isolate.zig`, `relational.zig`, `alien.zig`, `image.zig` | Zig rollback/oracle implementations; Rust canonical providers | gather per-rung retirement-readiness evidence, but retain the complete coherent Zig vector until a vector-level decision | exact surface, cross-floor, image/wire, and archive evidence |
| `idapi.zig`, `abi/idapi_manifest.zig` | shared contract | retain while any Zig oracle or higher Zig source consumes the ABI | append-only manifest and old-binary load |
| Zig `*_test.zig`, probes, baselines, sentinels | evidence | retain or replace only when equivalent failure behavior is proven | checker coverage and focused negative tests |
| `build.zig` | build-only orchestrator | retain until a separate toolchain-retirement plan | clean archive and pinned toolchain gate |
| `src/id/*.birnam`, `modules/*.birnam` | native policy | keep canonical; move duplicated mechanism only with owner proof | MOP/policy fixture and source-corpus parity |

No row is “deleted by default.” A retirement record must name the replacement,
the exact dynamic provider names, the fixture/recovery path, and the commit
that proves the old path is no longer canonical.

Design 66 already closed the host/artifact authority work and recorded
`z2_1_seam.status=none`: no native-policy removal seam is proven. Its floor,
Store, and PEG dispositions are explicitly retained, and their retirement may
not be reconsidered until the Design 66 release-cycle predicate is satisfied.
P1 below is therefore revalidation and retirement-readiness evidence, not a
second authority rewrite. With dependency order preserved, Lang is the first
unreviewed rung after the retained floor/Store/PEG evidence; no rung is deleted
or made independently recoverable by this plan.

## 4. Phase P0 — inventory and authority lock (first implementation phase)

P0 is diagnostic and documentation-only. It must land before any Zig source or
fixture is moved.

### P0.1 Reconcile the manifest

1. Run `scripts/check-remaining-zig-inventory.py` and
   `zig build test-remaining-zig-inventory` from the current checkout.
2. Refresh `abi/remaining-zig-v1.json` only for real repository drift. Every
   Zig path gets one of the allowed kinds, an owner, a disposition, and an
   explicit reason. The `dynamic_dependencies` array must continue to enumerate
   only `libbirnam_*.dylib` runtime providers.
3. Add a separate checked `abi/remaining-zig-edges-v1.json` with an `edges[]`
   table. Each row records `source`, `consumer`, `provider`, `edge_kind`
   (`runtime-dlopen`, `build-input`, `oracle`, `evidence`, or
   `native-policy`), `phase`, and `selection_context`. Freeze the allowed
   `phase` values to `P0`, `P1`, `P2`, `P3`, or `P4`, and the allowed
   `selection_context` values to `production`, `rollback`, `oracle`, `build`,
   or `evidence`; representative rows for each edge kind must be checked in.
   The existing
   `remaining-zig-v1.json` remains schema version 1; merging `edges[]` into it
   would require an explicit schema-version-2 migration. The checker derives
   runtime-dlopen rows from `dynamic_dependencies` and the Rust/Zig loader
   traces, and derives build/evidence rows from the checked build graph and
   script inventory; it fails closed on a missing or stale edge. Edge rows must
   never be copied into `dynamic_dependencies`.
4. Record the baseline commit, selected defaults, Rust/Zig versions, and the
   strict R6 receipt in a dated P0 appendix.

### P0.2 Prove the authority boundary

1. Run `test-remaining-zig-boundary` from an unrelated cwd with decoy Zig
   dylibs and source files. Require the Rust prefix to load only canonical
   dynamic names and the explicit oracle prefix to be the only Zig path.
2. Run `test-rust-host-message-owner-inventory` and
   `test-rust-artifact-authority`; reject any new Rust send/global lookup or
   artifact initializer call without an owner row.
3. Run the strict gate:

   ```text
   BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
   ```

   The pinned verify graph, six-skip allowance, benchmark comparator, and Z3
   production/rollback receipts must remain unchanged.

### P0.1 receipt (2026-07-27)

Commit `95681f2` adds and checks `abi/remaining-zig-edges-v1.json` without
changing provider selection. The checker now freezes the five edge kinds, five
phase values, and five selection contexts; validates safe source/provider paths;
requires representative rows for runtime, build, oracle, evidence, and native
policy; and requires every one of the 14 declared dynamic providers in both
production and complete-vector rollback contexts. The receipt is green:

```text
remaining-zig-inventory: ok zig_files=88 birnam_sources=91 rust_crates=18 test_scripts=77 build_targets=123 dynamic_dependencies=14
test-remaining-zig-inventory success
```

The existing `abi/remaining-zig-v1.json` remains schema version 1 and continues
to reserve `dynamic_dependencies` for installed dylib names only. The new edge
table is a separate schema so build inputs, oracle sources, evidence scripts,
and native policy cannot be mistaken for runtime providers.

### P0.2 receipt (2026-07-27)

At the post-P0.1 commit `95681f2`, the authority boundary and strict gate pass:

```text
remaining-zig-boundary: rust_receipts=14 zig_oracle_receipts=14 canonical_load_trace=ok oracle_load_trace=ok policy_authority=ok
rust-host-message-owner-inventory: ok send_calls=120 named_global_lookups=31 owner_classes=14 reviewed_functions=56 disposition=read-only
rust-artifact-authority: codec=pointer-free projection=single-module-init host=preflight-before-activation policy=native-loadArtifact
```

The strict rerun `BIRNAM_R6_REQUIRE_VERIFY=1 bash
scripts/r6-acceptance.sh` passes `305/305` verify steps, `516/522` tests, and
six skips. Z3 emits `distribution-selection: production omits Zig; rollback is
a complete Zig vector` and `z3-archive: production_a=clean production_b=clean
rollback=full-zig traces=ok`. The benchmark comparator remains unchanged with
exactly `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`, no missing
or unusable cases. No provider selection or runtime authority changed in P0.

### P0 exit gate

The checker is green, `edges[]` is complete, no source or test path is mistaken
for a dynamic dependency, decoys are ignored, and the strict R6 receipt is
recorded. P0 lands as one documentation/checker commit. No runtime behavior or
provider selection changes.

## 5. Phase P1 — host and artifact residue revalidation

Design 66 already established the Rust host/artifact authority split. P1
revalidates that split from clean archives and records any concrete parity gap;
it does not reopen a closed authority decision or move language policy.

### P1.1 Rust host observation completeness

1. For each project/resource helper operation, map the Zig oracle function to
   the Rust host observation function, its typed diagnostic, and its test.
2. Exercise check/build/run/install/test/LSP and source-free relocation from a
   second cwd with `BIRNAM_LIB_DIR`, `BIRNAM_ID_DYLIB`, and workspace overrides
   unset. The build checkout must never be read; recovery may use only
   packaged/embedded source or the supplied project root.
3. Run the Rust helper-boundary static audit and the Zig-oracle comparison. A
   mismatch in failure kind/order is a diagnosed parity task, not a reason to
   route the Rust host through Zig.

### P1.2 Artifact closure and preflight

1. Keep `birnam-artifact` pure and allocation-free; it owns descriptor parsing
   only. Keep one dynamic Artifact/ArtifactVt projection and one `module_init`.
2. Keep Rust host as the sole filesystem closure/preflight permission owner.
   Native-Birnam `Module loadArtifact:` remains the only dependency-ordering,
   initializer-delegation, Store/MOP-publication, and activation owner. The
   host never invokes an artifact initializer.
3. Run wrong-kind, identity/digest mismatch, missing dependency, malformed
   bytes, path escape, stale receipt, duplicate provider, and aborting-sentinel
   cases in Debug and ReleaseSafe. Assert zero activation and unchanged target,
   Store, and MOP state on every rejection.
4. Compare normalized failure kind, detail, and first-failure ordering between
   the Rust path and the Zig oracle using identical inert bytes.

### P1 exit gate

The Rust host/project/LSP and artifact negative corpus pass in isolated
prefixes; the Zig helper is loaded only by the explicit oracle/build; and the
strict R6 plus the explicit Rust archive commands remain green:

```sh
BIRNAM_HOST_IMPL=rust BIRNAM_CODEGEN_IMPL=rust \
  bash scripts/source-bootstrap-smoke.sh
bash scripts/rust-cold-compiler-determinism.sh
```

Relocated production may never read the build checkout. Explicit source
recovery may use packaged/embedded source or the supplied project root only;
checkout fallback is a failure. P1 is split into one focused
test/implementation commit per diagnosed gap, followed by a separate
documentation receipt commit.

### P1 revalidation receipt (2026-07-27)

At the post-P0 commit `e46d7bb`, the explicit Rust archive command passed Cargo
check/test, Rust production and complete Zig rollback prefixes, relocated
project/LSP execution, and the shared eight-case artifact corpus. The cold
compiler command passed both independent archives with these stable receipts:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

The focused artifact rerun also passed:

```text
rust-artifact-preflight: shared eight-case dependency, identity, format, path, native, and real-sentinel corpus passed (debug)
rust-artifact-authority: codec=pointer-free projection=single-module-init host=preflight-before-activation policy=native-loadArtifact
```

These are revalidation receipts only. The Rust host remains the canonical
observer, `Module loadArtifact:` remains the sole activation/publication owner,
and the Zig helper/oracle and complete rollback vector remain recoverable.

### R6 gate refresh receipt (2026-07-27)

The latest P0/P1 inventory, boundary, owner, and artifact checks are now part
of the scoped R6 matrix. Commit `bb387d8` pins the expanded matrix at
`183/183` build steps and `99/99` tests; the five added targets contribute
steps, not runtime test cases, and their receipts are asserted explicitly:

```text
remaining-zig-inventory: ok zig_files=88 birnam_sources=91 rust_crates=18 test_scripts=77 build_targets=123 dynamic_dependencies=14
remaining-zig-boundary: rust_receipts=14 zig_oracle_receipts=14 canonical_load_trace=ok oracle_load_trace=ok policy_authority=ok
rust-host-message-owner-inventory: ok send_calls=120 named_global_lookups=31 owner_classes=14 reviewed_functions=56 disposition=read-only
rust-artifact-authority: codec=pointer-free projection=single-module-init host=preflight-before-activation policy=native-loadArtifact
rust-artifact-preflight: shared eight-case dependency, identity, format, path, native, and real-sentinel corpus passed (debug)
```

The strict rerun at that commit also passes `305/305` verify steps, `516/522`
tests, six skips, both Z3 receipts, and exactly the existing two benchmark
crossings (`query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`), with
zero missing or unusable cases. No provider selection, benchmark allowance, or
rollback policy changed.

## 6. Phase P2 — per-rung retirement-readiness, no vector mutation

The Rust implementations already own the canonical dynamic providers. P2
gathers evidence for one rung at a time; it does not delete a Zig provider,
change the rollback vector, or claim independent recovery. Process the first
unreviewed rung after the retained floor/Store/PEG set (Lang), then TCC,
Codegen, Prelude, Collection, Error, Isolate, Relational, Alien, and Image.
Each named provider is its own slice; adjacent names are not a combined commit
or rollback decision.

### P2.1 Lang candidate checkpoint

The Lang candidate is evidence-only: Rust Lang is already the canonical
provider, so this checkpoint changes no provider selection, ABI, fixture, or
rollback artifact. The exact commit hash is captured here before running the
archive and focused gates; the following receipt commit records whether Lang
is retain-ready or blocked.

### P2.1 Lang readiness receipt (2026-07-27)

Candidate checkpoint: `de88de3`. No Lang source, provider selection, ABI, or
rollback artifact changed at this checkpoint. The focused Lang matrix passed:

```text
Build Summary: 74/74 steps succeeded; 8/8 tests passed
lang-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed
```

The fixture-free archive proof at the same hash passed both independent cycles:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
```

The installed Rust Lang dylib exports exactly one `module_init`; the Rust Lang
contract retains `Lang`, `LangVt`, `BirnamGrammar`, `parse:`, `grammar`, and
`furthestFail`, with CST digest `0df93b8aea32fe0d`. These checks plus the prior
N5.1–N5.4 behavior, diagnostics, and rollback records make Lang
**retain-ready** for evidence purposes. The Zig Lang dylib remains installed as
the explicit oracle and complete-vector rollback provider; no deletion or
vector mutation is authorized by this receipt.

### P2.2 TCC candidate checkpoint

The TCC candidate is evidence-only: Rust TCC is already available as the
explicit replacement provider, while the canonical Zig TCC and its complete
rollback vector remain unchanged. This checkpoint records the exact hash
before the TCC shell, artifact, export-audit, Alien, Image, and selection gates.

### P2.2 TCC readiness receipt (2026-07-27)

Candidate checkpoint: `9bd908d`. The focused TCC matrix passed with the three
existing platform skips:

```text
Build Summary: 79/79 steps succeeded; 62/65 tests passed; 3 skipped
tcc-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed
rust-tcc-dylib-audit: identity, exports, install name, and dependencies passed
rust-tcc-artifact: separate-process AOT activation passed
```

The TCC contract proof covers the explicit `Tcc`, `TccVt`, and `TccUnitVt`
surfaces, created/configured/relocated/installed/disposable/released state
transitions, the full `birnam_*` runtime-symbol set, generated-C callback
reentry, Alien and Image matrices, and the separate-process artifact path. The
installed Rust TCC dylib exports exactly one `module_init` and the audit keeps
the 22 runtime exports, `@rpath/libbirnam_id_tcc_rust.dylib` install name, and
allowed Darwin dependencies fail-closed.

The exact-hash archive receipts also pass:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make TCC **retain-ready** for evidence purposes. The canonical Zig
TCC, explicit `libbirnam_id_tcc_zig.dylib` oracle, and complete-vector rollback
remain installed; no provider deletion, mixed-vector recovery, or benchmark
allowance change is authorized by this receipt.

### P2.3 Codegen candidate checkpoint

The Codegen candidate is evidence-only: Rust Codegen is already the canonical
provider, while the canonical Zig Codegen implementation and its complete
rollback vector remain unchanged. This checkpoint records the exact hash before
the Codegen runtime, canonical, stack, Zig-store, selection-refusal,
distribution, and authority gates.

### P2.3 Codegen readiness receipt (2026-07-27)

Candidate checkpoint: `bccd9d4`. The focused Codegen matrix passed:

```text
Build Summary: 139/139 steps succeeded; 42/42 tests passed
codegen-selection: Rust canonical selection, retained fixtures, and unknown-choice refusal passed
codegen-distribution: packaged Rust canonical and explicit Zig rollback loads passed
codegen-dylib-audit: exact exported Codegen authority surface passed
```

The Rust runtime contract covers the frozen selector/export surface, pure CST
parity and invalid-diagnostic refusal, deterministic lowering/emission,
receiver-first trampolines, stack behavior, and Zig-store compatibility. The
canonical Rust Codegen dylib exports exactly one `module_init`; the explicit
Zig rollback dylib remains a separate complete-vector authority.

The standalone selector-refusal and packaged-distribution scripts pass at the
same candidate. The fixture-free archive proof passes both independent cycles:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make Codegen **retain-ready** for evidence purposes. The canonical
Rust Codegen provider, explicit Zig Codegen oracle, and complete full-Zig
rollback vector remain installed; no provider deletion, mixed-vector recovery,
or benchmark allowance change is authorized by this receipt.

### P2.4 Prelude candidate checkpoint

The Prelude candidate is evidence-only: Rust Prelude is already the canonical
provider, while the canonical Zig Prelude implementation and its complete
rollback vector remain unchanged. This checkpoint records the exact hash before
the primitive-protocol, standalone-Rust-Error, subprocess-I/O, and selector
gates.

### P2.4 Prelude readiness receipt (2026-07-27)

Candidate checkpoint: `15e55aa`. The focused Prelude matrix passed:

```text
Build Summary: 73/73 steps succeeded; 4/4 tests passed
prelude-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed
```

The Rust Prelude contract covers the two primitive protocol tests, including
integer arithmetic/comparison/loop behavior, the standalone Rust Error owner,
and the System `load:` source-file path through Codegen. The subprocess probe
also passed the `readLine`/`println` stdin/stdout boundary. The canonical Rust
Prelude dylib exports exactly one `module_init`; the explicit Zig Prelude
rollback dylib remains a separate complete-vector authority.

The selector smoke passes independently at the same candidate, and the exact-
hash archive receipts pass:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make Prelude **retain-ready** for evidence purposes. The canonical
Rust Prelude provider, explicit Zig Prelude oracle, and complete full-Zig
rollback vector remain installed; no provider deletion, mixed-vector recovery,
or benchmark allowance change is authorized by this receipt.

### P2.5 Collection candidate checkpoint

The Collection candidate is evidence-only: Rust Collection is already the
canonical provider, while the canonical Zig Collection implementation and its
complete rollback vector remain unchanged. This checkpoint records the exact
hash before the persistent-vector protocol and collector/layout gates.

### P2.5 Collection readiness receipt (2026-07-27)

Candidate checkpoint: `14ce52d`. The focused Collection matrix passed:

```text
Build Summary: 72/72 steps succeeded; 5/5 tests passed
test-rust-collection success: 1 test passed
test-collection-gc success: 2 tests passed
test-gc-layout-mark success: 2 tests passed
```

The canonical Collection module suite also passes with Rust selected:

```text
Build Summary: 66/66 steps succeeded; 20/20 tests passed
test-collection success: 20 tests passed
```

Together these checks cover the Rust persistent Vector/Range/Map/StringBuilder
protocol, structural sharing and indexed persistence, collector churn, precise
layout tracing, and conservative unknown-layout behavior. The canonical Rust
Collection dylib exports exactly one `module_init`; the explicit Zig Collection
rollback dylib remains a separate complete-vector authority.

The exact-hash archive receipts pass:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make Collection **retain-ready** for evidence purposes. The
canonical Rust Collection provider, explicit Zig Collection oracle, and complete
full-Zig rollback vector remain installed; no provider deletion, mixed-vector
recovery, or benchmark allowance change is authorized by this receipt.

### P2.6 Error candidate checkpoint

The Error candidate is evidence-only: Rust Error is already the canonical
provider, while the canonical Zig Error implementation and its complete rollback
vector remain unchanged. This checkpoint records the exact hash before the
standalone family, preload-order, and cross-floor diagnostic gates.

### P2.6 Error readiness receipt (2026-07-27)

Candidate checkpoint: `31e0ced`. The focused Error matrix passed:

```text
Build Summary: 70/70 steps succeeded; 6/6 tests passed
test-rust-error success: both preload orders passed
test-error success: 4 tests passed with Rust selected
cross-floor-errors: specified kinds, messages, and syntax diagnostics match
```

The Rust Error contract preserves the standalone error family and
`doesNotUnderstand` behavior in both Rust-first and Zig-first preload orders.
The canonical Error suite covers propagation, partitioning, DNU, and
zero-divide behavior. The independent cross-floor script confirms identical
error kinds, messages, and syntax diagnostics for Zig and Rust floors. The
canonical Rust Error dylib exports exactly one `module_init`; the explicit Zig
Error rollback dylib remains a separate complete-vector authority.

The exact-hash archive receipts pass:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make Error **retain-ready** for evidence purposes. The canonical
Rust Error provider, explicit Zig Error oracle, and complete full-Zig rollback
vector remain installed; no provider deletion, mixed-vector recovery, or
benchmark allowance change is authorized by this receipt.

### P2.7 Isolate candidate checkpoint

The Isolate candidate is evidence-only: Rust Isolate is already the canonical
provider, while the canonical Zig Isolate implementation and its complete
rollback vector remain unchanged. This checkpoint records the exact hash before
the wire-v1, worker-control-plane, malformed-payload, and process smoke gates.

### P2.7 Isolate readiness receipt (2026-07-27)

Candidate checkpoint: `178f07a`. The focused Isolate matrix passed:

```text
Build Summary: 98/98 steps succeeded; 9/9 tests passed
test-rust-isolate success: 4 tests passed
test-isolate success: 5 tests passed with Rust selected
test-isolate-process success: worker process round-trip passed
```

The contract covers wire-v1 network-order headers, scalar/vector/map codec
round trips, duplicate-key/trailing-byte refusal, structured worker failures,
host registry start/stop, copied-value dispatch, malformed request rejection,
and the separate worker-image process smoke. The canonical Rust Isolate dylib
exports exactly one `module_init`; the explicit Zig Isolate rollback dylib
remains a separate complete-vector authority.

The exact-hash archive receipts pass:

```text
rust-cold-compiler: cycle-a archive passed
rust-cold-compiler: cycle-b archive passed
rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

These checks make Isolate **retain-ready** for evidence purposes. The canonical
Rust Isolate provider, explicit Zig Isolate oracle, and complete full-Zig
rollback vector remain installed; no provider deletion, mixed-vector recovery,
or benchmark allowance change is authorized by this receipt.

### P2.8 Relational candidate checkpoint

The Relational candidate is evidence-only: Rust Relational is already the
canonical provider, while the canonical Zig Relational implementation and its
complete rollback vector remain unchanged. This checkpoint records the exact
hash before the kernel, Rust-vs-Zig Store callback, full suite, and selector
gates.

For each provider, use this six-step transaction. It is a protocol, not a
promise of six commits; commit only material changes, and never combine two
providers:

1. **Candidate checkpoint:** commit the focused contract/evidence change (or
   record that no code change is needed) and capture its exact hash.
2. **Contract and behavior proof:** at that hash, compare
   export/selector/layout/identity surfaces, run generated-C or wire/image
   compatibility as applicable, hostile-input and forced-GC tests, and the
   complete MOP law. Prove exactly one exported `module_init` authority while
   preserving repeated `prior_state` behavior.
3. **Cold archive proof:** run the two fixture-free Rust archives and the
   explicit *complete* Zig rollback archive at the candidate hash. Verify
   relocation, dynamic load traces, no prebuilt Birnam artifact, and semantic
   generated-C/descriptor receipts; normalize only archive-specific prefixes.
4. **Readiness decision:** record `retain-ready` or `blocked` for this rung.
   `retain-ready` means its Rust provider is canonical and its evidence is
   sufficient for a future vector decision; it does not authorize deletion.
   Floor, Store, and PEG remain blocked until Design 66's release-cycle
   predicate is satisfied.
5. **Acceptance gate:** run `zig build verify`, strict R6, the cold compiler
   harness, `scripts/dist.sh`, and the rung-specific relocated smoke in
   isolated prefixes. No benchmark allowance may be added.
6. **Receipt commit:** after the gates pass, commit the inventory/status
   receipt naming the canonical provider and the still-retained complete Zig
   rollback vector. This commit is separate from any candidate implementation.

### P2 stop conditions

Stop the rung slice if any export/layout/identity differs, if a Zig provider is
opened from a Rust production prefix, if an image/wire/generated-C fixture
changes, if Store facts diverge from the MOP law, if a failed candidate mutates
state, if a callback crosses a Rust borrow, if the complete Zig vector cannot
be built, or if recovery requires reconstructing an artifact from Git history.
Resolve by adding a typed boundary or fixture; never by weakening a contract.

## 7. Phase P3 — native-Birnam policy disposition (currently closed)

Design 66's checked 120-callsite inventory concluded
`z2_1_seam.status=none`: no safe policy-removal seam is proven. P3 therefore
has no implementation work in this plan. It may reopen only after a reviewed
inventory change nominates one exact branch; selecting a convenient branch is
not sufficient. If reopened, it considers only branches that duplicate a
policy decision already owned by a `.birnam` module. It is not a general
“rewrite Rust in Birnam” phase.

1. Start from the checked `abi/rust-host-message-owner-v1.json` inventory.
2. Select one candidate whose native selector, protocol, and tests already
   exist. Write an authority transfer record naming the old Rust branch, the
   native owner, result-shape validation that remains in Rust, and the exact
   MOP/Store effects.
3. Add a red test proving the Rust branch is not consulted, then move the
   smallest branch and run the policy/MOP fixture, source corpus, project,
   image, and rollback gates.
4. Reject a candidate if it creates a second message authority, changes
   diagnostic identity/order, moves IO/FFI into policy, or makes bootstrap
   depend on a prebuilt native module.

P3's current “no safe seam proven” result is a successful disposition, not a
failed migration. Any future reopening gets its own review and receipt.

## 8. Phase P4 — complete-vector retirement and toolchain decision

Per-rung `retain-ready` receipts never authorize deletion. Retiring the Zig
runtime requires one vector-level decision that either:

1. retires the complete coherent Zig floor/rung vector atomically, with a
   release/recovery artifact and all fixture coverage; or
2. introduces a separately reviewed, versioned rollback scheme that explicitly
   permits mixed vectors and updates the loader/manifest contract without
   changing any frozen ABI or runtime default.

Until one of those decisions is accepted, `-Ddistribution=rollback` must remain
the complete Zig vector and all Zig provider sources/fixtures remain installed
only in explicit rollback/oracle paths. A per-rung recovery name is not enough.
Keep `idapi.zig`, the ABI manifest, negative sentinels, and evidence probes
while they are consumed by any oracle or contract checker.

The build driver and Zig toolchain are out of scope for P2/P3. A future plan may
replace `build.zig` only after a Rust/native-Birnam build driver can bootstrap
the same dynamic closure from another language, install no hidden static rung,
and reproduce the strict R6/Z3 receipts. That is a new plan and a new review.

## 9. Acceptance matrix

| Dimension | Required proof |
| --- | --- |
| bootstrap | two clean archives build the complete production closure from source using declared non-Birnam stage-0 toolchains only; no prebuilt Birnam executable/image/dylib/module/generated output |
| dynamic boundary | every production provider is an installed dylib with one `module_init`; no source/crate/static dependency in the runtime manifest |
| ABI | IdApi, exports, layouts, descriptors, generated-C/wire/image fixtures unchanged |
| MOP/Store | dictionary authority, derived mirror, retract/mutate/reassert ordering, no inherited direct facts |
| host/artifact | relocated project/LSP, typed preflight failures, zero activation on rejection |
| rollback | explicit complete Zig-vector oracle prefix boots the same fixtures; no ambient fallback or per-rung mixed-vector deletion |
| native policy | one `.birnam` owner, no Rust policy shadow, source-corpus parity |
| release | ReleaseSafe `scripts/dist.sh`, no `_zig` libraries in production bundle, relocated install smoke |
| performance | existing comparator only; no new allowance |

## 10. Commit and review protocol

Every numbered subphase is one or more small commits with focused tests green;
documentation receipts are separate commits. For any provider candidate, first
land the focused candidate checkpoint, then run archive gates against that exact
hash (the archive harness consumes `git archive HEAD`), then land the decision
and receipt commit. Re-run the post-receipt strict gate when the receipt changes
the build graph. A reviewer must see the exact command output, commit hash,
dynamic provider names, and complete-vector rollback path. No “cleanup” commit
may mix a provider switch, fixture deletion, policy move, and build-driver
rewrite.

The first review target is this document itself. A **NO-GO** must be corrected
in a new doc commit and resubmitted. A **GO** permits P0 only; each later
authority change receives its own focused review and gate.

## 11. Initial work order

1. Review and land this plan (no code change).
2. Execute P0.1 manifest and separate `edges[]` reconciliation and checker
   receipt.
3. Execute P0.2 decoy/authority boundary and strict R6 receipt.
4. Submit the P0 result for review before beginning P1.
5. Revalidate P1.1 host observation and P1.2 artifact negative corpus as
   independently committed slices, using explicit Rust selections.
6. Re-run the cold compiler and release gates, then nominate Lang as the first
   P2 rung only after the reviewer confirms that no Zig implementation is
   canonical and the complete Zig rollback vector remains intact.

The current accepted baseline is the Rust default with the post-cutover R8
observation checkpoint recorded in Design 61. Until a later retirement record
is accepted, explicit Zig rollback fixtures remain part of the recovery design.
