# 68 — Remaining Zig Conversion and Native-Birnam Completion Plan

> **Status:** Reviewed GO at `66b66b1` (2026-07-27). This plan starts from the
> accepted Rust floor/native-rung state at `rust-libid-floor` and is the
> execution plan for the Zig residue that remains after the Rust conversion.
> It supersedes neither Design 66's residue map nor Design 67's acceptance
> records; it turns their current evidence into a bounded conversion and
> retirement sequence.

### Review record

- `023576a`: initial adversarial review — **NO-GO** for floor/provider-init
  conflation, Artifact ownership, per-rung rollback deletion, C3 policy scope,
  ledger precision, R6 remeasurement, and incomplete MOP/unsafe/bootstrap
  laws.
- `c96b47a`, `3513b1e`, `4fa83e0`: corrected those issues and tightened the
  release-cycle block, complete-vector definition, ledger transaction, exact
  ABI signatures, mutation ordering, callback rooting, codec/provider audits,
  and retain-versus-retire contract.
- `66b66b1`: final adversarial review — **GO**; no remaining invariant, schema,
  rollback, ownership, MOP, unsafe-boundary, bootstrap, receipt, or phase-order
  blocker. C0 may begin; later authority or retirement changes still require
  their specified focused reviews.

## 1. Objective and completion definition

Convert every remaining Zig implementation that can be a production authority
to either:

1. the already-canonical Rust dynamic provider;
2. the Rust host/build/resource observation layer; or
3. the existing native-Birnam policy layer.

The result is **not** a promise to delete every `.zig` file. Shared ABI
definitions, negative fixtures, frozen old-binary/image evidence, and a named
Zig rollback oracle may remain when they are required for recovery or
compatibility. A retained file must be classified, isolated from production,
and covered by a reason and owner in the conversion ledger.

The conversion is complete only when:

- the canonical production closure loads no Zig implementation, oracle binary,
  `_zig` dylib, checkout source, or ambient loader path;
- the canonical floor has exactly one Rust `id_init() -> IdApi*` owner and
  each of the 13 higher dynamic provider names has exactly one Rust
  `module_init(IdApi*, void*) -> void*` authority, one identity/digest, and an
  explicit rollback fixture policy;
- host observation and artifact preflight are Rust-owned, while
  `Module loadArtifact:` and all `.birnam` policy remain native-Birnam-owned;
- every retained Zig provider is reachable only through an explicit isolated
  oracle/rollback selection and cannot be discovered by production resolution;
- the IdApi, Oop/vtable representation, payload/layout descriptors, generated-C
  ABI, image-v4 records, isolate wire-v1 records, and Store/MOP law are unchanged;
- the conversion ledger, inventory checker, archive/bootstrap, relocation,
  cross-floor, hostile-input, GC, image, and strict R6 receipts pass at the
  same commit; and
- each per-rung readiness/retention decision is separately reviewable, while
  any retirement is one atomic, reversible complete-vector decision.

## 2. Current evidence and residue boundary

The current checker reports:

```text
remaining-zig-inventory: ok zig_files=88 birnam_sources=91 rust_crates=20 test_scripts=80 build_targets=125 dynamic_dependencies=14 explicit_edge_rows=32 derived_coverage_rows=290
```

The 14 production dynamic names are already Rust-canonical: floor, Artifact,
Store, PEG, Lang, TCC, Codegen, Prelude, Collection, Error, Isolate,
Relational, Alien, and Image. The remaining Zig implementation files are:

- `src/id/main.zig`, `project.zig`, `project_build.zig`, `project_check.zig`,
  and `runtime_resources.zig`: explicit Zig-host/oracle or build-only paths;
- `src/id/artifact_inspector.zig` and `artifact_preflight.zig`: the former is
  the Zig Artifact-provider oracle and the latter is Zig preflight evidence
  after Rust codec and host preflight became canonical;
- the 13 Zig floor/lower-rung sources: named rollback/oracle fixtures;
- `src/id/artifact_inspector.zig` is counted separately from those 13 sources
  as the Artifact projection/provider oracle;
- `idapi.zig`, `artifact.zig`, ABI tests, sentinels, probes, and Zig test roots:
  shared contracts or evidence, not production providers; and
- `.birnam` sources: canonical language policy, which must not be shadowed by
  Rust native methods.

The current strict acceptance is:

```text
focused: Build Summary: 218/218 steps succeeded; 99/99 tests passed
verify:  Build Summary: 307/307 steps succeeded; 516/522 tests passed; 6 skipped
live Alien: 66/66 steps succeeded; 9/9 tests passed
distribution-selection: production omits Zig; rollback is a complete Zig vector
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
z3 artifact traces: production-a/production-b/rollback Module loadArtifact confined
benchmark: regressions=2, missing=0, unusable=0
accepted: query.aggregate +5.8%, compile.stage1-stdlib +11.9%
```

This is the baseline for every authority or distribution change. Counts and
digests are measured receipts, not values to be guessed or widened.

### C0 implementation receipt

C0.1 is landed as a fail-closed conversion ledger and build target. The
checker covers all 88 Zig paths, 14 dynamic names, and 39 native-Birnam policy
sources, rejects dirty candidate trees, and validates the complete rollback
vector against the residue manifest. The focused matrix and standalone strict
components measured the receipts above; the exact strict verify summary is
`307/307` steps with `516/522` tests and six skips.

C0.2 is landed in the archive gate. `check-production-prefix.py` now compares
each canonical provider with the independently built Rust fixture and the
complete rollback prefix's Zig fixture, requiring Rust equality and Zig
inequality. The Z3 archive smoke runs separate confined `Module loadArtifact:`
traces for both production prefixes and the rollback prefix, and rejects Zig
fixtures, Rust fixtures in rollback, source-tree paths, and ambient paths.

## 3. Invariants and non-negotiable decisions

1. **One runtime owner.** A process has one IdApi floor, heap, interner, Store,
   generation registry, and canonical family graph. No conversion step may
   load a Rust provider and its Zig counterpart in one production process.
2. **Dynamic boundaries remain explicit.** The production floor is a dynamic
   library with exactly one `id_init() -> IdApi*`; each higher rung has exactly
   one `module_init(IdApi*, void*) -> void*`. Rust workspace membership must
   not statically fold another provider's initializer, identity, or projection
   into the selected dylib.
3. **Dynamic dependency manifests name libraries only.** Rust crate paths,
   Zig source paths, static TCC objects, test executables, and policy files are
   graph/coverage evidence, never entries in `dynamic_dependencies`.
4. **Object dictionary authority and MOP law.** The live vtable dictionary
   remains the dispatch authority. Store is the derived MOP mirror and the
   supported post-bootstrap mutation chokepoint. Every change runs
   authorizer-before-change, then follows the frozen ordering: add is
   `mutate → assert`; replace is `retract → mutate → reassert`; remove is
   `retract → remove`. The tracked dictionary and Store mirror commit
   together, followed by exactly one post-commit observer. For each direct
   binding the mirror is exactly the live Method, `#primitive` for an
   unreified backfill, or absent; it never copies inherited methods or names a
   stale method. Raw bootstrap/private mutation may create the initial state
   with an honest absence; backfill is direct-only and never reveals inherited
   methods.
5. **Native policy is not reimplemented.** `stdlib.birnam`,
   `project-policy.birnam`, and native module sources remain the protocol and
   language-policy authority. Rust may observe, validate, load, and publish;
   it may not install a competing policy vtable.
6. **Bootstrap independence.** Production can be cold-bootstrapped from a
   source checkout using a declared cold stage-0 chain and the native-Birnam
   compiler/driver built by that chain. Bootstrap rejects prebuilt/generated
   Birnam executables, compilers, hosts, dylibs, images, compiled modules, and
   generated output as inputs and does not discover a hidden Zig runtime
   artifact. This plan does not require deleting the Zig compiler toolchain;
   toolchain retirement is a separate decision.
7. **Re-entry and ownership.** No Rust borrow, lock, temporary pointer, or
   mutable state borrow spans `send`, `dlopen`, TCC, finalizers, root/trace
   callbacks, or module initialization. Unsafe boundaries must cover resolved
   Method/DNU calls, mutation hooks, Alien callbacks, dynamic-extent collector
   marks, rooted callback state, and process-lifetime handles. Collector-mark
   callbacks are synchronous and dynamic-extent; their marks cannot escape.
   Callback-visible native objects and state remain rooted for the entire
   callback lifetime. C callbacks never unwind a Rust panic; every callback
   path has an explicit owner and lifetime.
8. **Fail closed and atomically.** Invalid ABI, identity, path, descriptor,
   image, wire, generation, or dependency input rejects before activation and
   leaves the previous dictionary, Store, and filesystem target unchanged.
9. **Recovery is explicit.** A Zig fixture may remain only in a named isolated
   prefix or checked-in compatibility archive with fixed identity/digest. No
   source deletion is allowed until the recovery evidence is stronger than the
   source being deleted.

**Complete Zig vector** has one exact meaning in this plan: the canonical Zig
host fixture `bin/birnam-zig-oracle` plus all 14 dynamic fixtures listed in the
`vector` and `dynamic_load_order` fields of `abi/rollback-prefix-v1.json`.
Those 14 are the floor and 13 higher providers; a subset is not a rollback
vector. Any mixed-vector design would require a separately versioned and
reviewed contract.

## 4. Conversion ledger and dispositions

The ledger is machine-checked and has one row per Zig path and one row per
canonical dynamic name. Every row has a unique `key`: path keys are
`zig:path:<repo-relative path>` and provider keys are
`dynamic:name:<canonical dylib name>`. Each row contains:

```text
key, kind, current_owner, target_owner, production_status,
rollback_status, identity_source, digest_source, required_fixtures,
next_phase, removal_authority, last_receipt
```

Allowed `kind` values are `zig-source`, `dynamic-provider`, `build-oracle`,
`shared-contract`, `policy-source`, and `evidence-fixture`. Allowed
`current_owner` values are `rust-dylib`, `zig-floor`, `zig-rung`, `zig-host-oracle`,
`zig-artifact-oracle`, `zig-preflight-oracle`, `zig-build`, `shared-contract`,
`native-birnam-policy`, and `evidence`. Allowed `target_owner` values are
`rust-dylib`, `rust-host`, `build-only`, `native-birnam-policy`,
`shared-contract`, and `evidence-fixture`. Allowed `production_status` values
are `canonical`, `never-production`, and `retired`; allowed
`rollback_status` values are `required`, `pinned`, and `not-applicable`.
Allowed `next_phase` values are `C0`, `C1`, `C2`, `C3`, `C4`, and `none`;
allowed `removal_authority` values are `none`, `vector-review`,
`toolchain-review`, and `policy-review`.

`identity_source` and `digest_source` are required for `dynamic-provider`
rows; `required_fixtures` is required for rows with `rollback_status=required`
or `pinned`; `last_receipt` is required after a gate and otherwise explicitly
`null`. `next_phase`, `removal_authority`, and those source/fixture/receipt
fields may be `null` only when their condition is not applicable; omitted
fields are invalid. Canonical `dynamic-provider` rows must use
`current_owner=rust-dylib` and reference a name and identity manifest in
`abi/remaining-zig-v1.json`; fixture references must resolve to a declared
vector entry or checked evidence receipt.

The separate `policy_owners` section has keys
`birnam:path:<repo-relative .birnam path>` and covers every `.birnam` source
under `src/id/` or `modules/`; each key names exactly one
`native-birnam-policy` owner and is never represented as a dynamic-provider
row.

The checker validates unique keys, enum values, nullable fields, and
referential integrity against `abi/remaining-zig-v1.json` and every provider
identity manifest. The ledger records an immutable `baseline_commit`. The
checker requires either an explicit candidate commit plus a clean worktree or
an explicit tree hash, validates that checked tree, and emits the candidate in
the receipt; it is not stored in the ledger, avoiding a self-referential commit
field.

The initial ledger must classify the residue as follows:

| Residue | Target owner | Production disposition | Recovery disposition |
| --- | --- | --- | --- |
| `main.zig` and host helpers | `rust-host` | never-production | retain explicit Zig host oracle until release decision |
| `build.zig` | `build-only` | never-production | retain until a separately reviewed toolchain-retirement plan |
| `artifact_inspector.zig` | `rust-dylib` | never-production | retain the Zig Artifact-provider oracle until vector retirement |
| `artifact_preflight.zig` | `rust-host` | never-production | retain parity and sentinel fixtures |
| `artifact.zig` | `shared-contract` | never-production | keep for Zig oracle/ABI evidence |
| `idapi.zig` | `shared-contract` | never-production | keep for Zig oracle/ABI evidence |
| 13 Zig floor/lower-rung sources | corresponding `rust-dylib` | never-production | retain the complete rollback vector; no per-rung deletion |
| ABI tests, probes, sentinels, benchmarks | `evidence-fixture` | never-production | retain or replace only with equivalent evidence |
| `.birnam` policy modules | `native-birnam-policy` | canonical | never duplicate in Rust |

No implementation step may silently change a row's target owner. A changed
owner requires an invariant review and a fresh strict gate.

## 5. Phase C0 — freeze the current conversion baseline

**Goal:** make the current residue and accepted Rust production closure
machine-readable before changing any fixture or helper.

### C0.1 Update and check the ledger

1. Add `abi/remaining-zig-conversion-v1.json` with the rows and dispositions
   above, referencing (but not duplicating) the 14 dynamic names in
   `abi/remaining-zig-v1.json`. Include the immutable baseline commit,
   selected defaults, inventory receipt, Rust identity/digest sources, and
   exact strict-R6 receipt. The candidate commit is supplied to the checker
   and emitted in its receipt rather than written into this ledger.
2. Add `scripts/check-remaining-zig-conversion.py`. It fails on an unclassified
   Zig file, a `rust-dylib` row without a canonical dynamic owner, a production
   path containing `_zig` or `birnam-zig-oracle`, a missing rollback reason, an
   ambiguous `.birnam` policy owner, or a `dynamic_dependencies` entry that is
   not a declared dylib name. It also checks the schema enums/nullability,
   unique `zig:path:`/`dynamic:name:` keys, references to
   `abi/remaining-zig-v1.json`, and provider identity manifests.
3. Add `zig build test-remaining-zig-conversion`, assert its exact receipt in
   the target, and include that target in the scoped R6 `latest_tests` matrix.
   The checker must be read-only and deterministic from any cwd. Wire it into
   the ordinary `zig build test` graph only after its focused receipt is
   stable, then remeasure and repin the focused and strict step counts.

The ledger transaction is ordered: (a) candidate checkpoint commit and checked
tree hash, (b) focused and strict gates at that exact clean tree, (c) receipt
commit that records the measured counts and digests, and (d) a post-receipt gate
run. The candidate checker rejects a dirty `HEAD`; an explicit tree hash may
be supplied only when it identifies the exact reviewed tree. The receipt
commit is therefore not asked to predict its own post-commit hash.

**C0.1 gate:** the checker, its exact scoped-R6 receipt, ordinary test graph,
`git diff --check`, and the scoped plus strict R6 commands pass. No provider
selection changes. The focused and strict step counts are remeasured after the
new target is wired; the `516/522` test total, six skips, live Alien receipt,
Z3 receipts, and two benchmark allowances are retained only if the measured
run still matches them.

### C0.2 Prove production/fixture separation

1. Extend the existing production-prefix checker to require that every
   canonical Rust dylib matches its `_rust` fixture and differs from its `_zig`
   fixture; equality with a Zig fixture is a packaging failure.
2. Extend the load trace to reject Zig source, `_zig` dylibs, named oracle
   binaries, cwd paths, and source-tree paths for both ordinary host and
   `Module loadArtifact:` paths.
3. Record a separate oracle trace that intentionally selects the full Zig
   vector. It must run in a clean process and may not be used by production
   acceptance.
4. Run two independent production archive prefixes and one full-Zig rollback
   prefix. Store only receipts and fixed fixture digests in the production
   acceptance output.

**C0.2 gate:** production traces are Rust-only, rollback traces are complete
Zig-only, both are confined to their selected prefixes, and strict R6 plus Z3
archive receipts pass.

## 6. Phase C1 — complete Rust host and artifact conversion

**Goal:** remove remaining Zig host/helper behavior from the canonical host
path while preserving Zig source only as an explicit oracle.

### C1.1 Host observation parity

1. Compare every operation in `main.zig`, `project.zig`,
   `project_build.zig`, `project_check.zig`, and `runtime_resources.zig` with
   the Rust host owner. The ledger must name the Rust function/test receipt
   for every operation; “implemented elsewhere” is not sufficient.
2. Add relocated `/tmp` project, build, run, test, LSP, source-fallback, and
   resource-override cases. Unset `BIRNAM_LIB_DIR`, `BIRNAM_ID_DYLIB`, and
   `BIRNAM_RUST_WORKSPACE_DIR` for canonical runs.
3. Add a static host-boundary test that rejects new Rust references to the Zig
   helper files and rejects production `birnam-zig-oracle` resolution.
4. Preserve the Zig host executable as an explicit rollback fixture. Its
   source and loader are not a canonical production dependency.

**C1.1 gate:** Rust host unit tests, relocated project/LSP tests, source
bootstrap, production/rollback traces, and strict R6 pass; no policy or
dynamic provider owner changes.

### C1.2 Artifact codec/preflight split

1. Keep `birnam-artifact-codec` pointer-free and provider-free: it must not
   export a provider identity, `module_init`, IdApi globals, activation, or
   closure policy. The codec identity field is absent, not merely distinct.
   `birnam-artifact` is the dynamic Artifact provider and exports exactly one
   `module_init`; its provider identity is distinct from the codec crate.
   Apply the same distinction to provider `birnam-image` and provider-free
   `birnam-image-codec`, whose codec identity field is also absent.
2. Keep `libbirnam_id_artifact.dylib` as the sole dynamic Artifact projection;
   the Rust host alone performs filesystem sibling closure and preflight.
3. Keep native-Birnam `Module loadArtifact:` as the only runtime ordering,
   Store/MOP publication, initializer delegation, and activation authority.
4. Run the shared eight-row descriptor corpus, malformed/duplicate/path-escape
   cases, the real aborting sentinel, and the failed-preflight atomicity check
   through both Rust and explicit Zig oracle evidence.
5. Do not remove `artifact.zig`, `artifact_inspector.zig`, or
   `artifact_preflight.zig` until the rollback archive contains their fixed
   receipts and the ledger marks them evidence-only.

**C1.2 gate:** compiled export/dependency audits prove that the Artifact and
Image providers export only their own initializer and identity, while both
codec crates and Codegen export neither provider's initializer nor identity;
the exact codec/projection/host/native ownership audit passes, and strict
R6/Z3 pass.

**C1 receipt (2026-07-27):** the Rust-host helper boundary, relocated project
check/build/run/install/test, LSP and source-fallback paths, runtime-resource
parity, Artifact/Image codec ownership audit, and shared Rust/Zig preflight
corpus all passed in strict R6. The archive gate additionally records
provider-vs-fixture digests and confined `Module loadArtifact:` traces for two
independent Rust production prefixes and the complete Zig rollback prefix.

## 7. Phase C2 — per-rung Rust canonicalization and Zig fixture decisions

The floor and all 13 higher rungs already have Rust canonical providers. C2
converts the remaining Zig implementations by proving that they are never
production authorities and then recording one separately reviewed fixture
decision at a time. The order is:

```text
floor → Store → PEG → Lang → TCC → Codegen → Prelude → Collection → Error
→ Isolate → Relational → Alien → Image → Artifact/host closure
```

For each rung, create a receipt containing:

1. canonical Rust identity, digest, install name, exact exported symbols, and
   dynamic dependencies;
2. Zig fixture identity/digest, owner, and isolated selection command;
3. ABI/layout/selector/export parity, MOP consistency, GC/trace/finalizer,
   image/wire/compiled-unit, and hostile-input evidence applicable to that
   rung;
4. two same-floor and two cross-floor processes, never one process containing
   both implementations;
5. one Rust production archive and one complete-vector Zig rollback archive;
6. separate `readiness` and `rollback_status` fields. `readiness` is one of
   `blocked`, `retain-ready`, or `vector-retirement-ready`; `rollback_status`
   is one of `pinned`, `required`, or `not-applicable`. The transition to
   `vector-retirement-ready` is permitted only for the complete vector and
   after the Design 66 release-cycle predicate. Per-rung deletion is not an
   allowed outcome: the complete Zig vector must remain intact until an
   explicitly reviewed atomic vector retirement; and
7. a fresh strict R6 receipt after the choice.

### C2.1 First three bounded rung slices

These are the first implementation slices after plan review. They produce
readiness receipts only; none may delete one provider from the rollback vector.

**C2.1a — floor:** update the floor ledger row with current Rust/Zig digests,
rerun the 256-test lower-rung matrix, old native-client/image corpus, forced-GC
and MOP checks, and record the decision. The invariant-based default is
`readiness=blocked` and `rollback_status=pinned` until the Design 66
release-cycle predicate, a complete Rust release, and full-Zig rollback history
exist; no source deletion or vector-retirement readiness is implied.

**C2.1b — Store:** rerun the Store consistency-law suite under Zig and Rust
Debug/ReleaseSafe, including tracked/raw mutation, backfill, authorizer,
observer, image boot, and inherited revelation. Retain the Zig fixture unless
the Design 66 release-cycle predicate is satisfied and the receipt proves
recovery without it; record `readiness=blocked` and
`rollback_status=pinned` until then and never remove the Store mirror contract.

**C2.1c — PEG:** rerun CST/metadata/diagnostic/source corpus and generated-C
parity under Rust and the explicit Zig parser fixture. Preserve the parser
fixture and record `readiness=blocked` and `rollback_status=pinned` while any
old image, parser diagnostic, generated-C recovery path, or the Design 66
release-cycle predicate depends on it.

Each slice is one commit after focused tests, followed by strict R6 before the
next slice. A vector-retirement commit may not include another rung or a
policy change and must remove the complete Zig vector atomically.

**C2.1 receipt contract (2026-07-27):**
`abi/c2-rung-receipts-v1.json` and `scripts/check-c2-rung-receipts.py` record
the three bounded slices independently from the conversion ledger. The
checker requires Debug/Release Rust digests, the complete-vector Zig fixture
mapping, exact install names and export sets, direct dynamic dependencies and
runtime prerequisites, isolated Rust/Zig selection commands, two same-floor
and two cross-floor process cells, and the strict-R6 receipt. Every row is
currently `readiness=blocked` and `rollback_status=pinned`; the checker rejects
any per-rung promotion or fixture retirement.

**C2.1a floor receipt (2026-07-27, receipt commit `57615b9`):** the isolated
lower-rung matrix passed 256/256 tests for the Zig floor and Rust Debug and
Release floors. Floor shell, GC metadata/layout, dispatch-reentry, old-client,
image, and MOP evidence remain explicit fixtures. The ledger row for
`src/id/id.zig` records the Rust `rust-id-v1` profiles and retains
`libbirnam_id_zig.dylib`; no source or rollback-vector member was removed.
The post-receipt strict R6 checkpoint at `422e739` passed with the pinned
`307/307` verify, `516/522` tests, six skips, live Alien, Z3 archive/trace,
and benchmark receipts.

**C2.1b Store evidence (2026-07-27):** the Store consistency-law suite passed
with the Zig Store/Zig floor oracle and Rust Store against the Rust floor in
Debug and Release. The image backfill, tracked/raw mutation, authorizer,
observer, inherited-revelation, image-boot, REPL, project, and LSP cases passed
through the existing `rust-store-mop` and `rust-store-stack` receipts. The
Store row now records that evidence as a focused blocked/pinned receipt; no
Store mirror contract or Zig rollback fixture was removed. Its strict R6
checkpoint at `4c1f66a` passed with the same `307/307` verify,
`516/522` tests, six skips, live Alien, Z3 archive/trace, and benchmark
receipts before PEG advanced.

**C2.1c PEG evidence (2026-07-27):** the explicit Rust PEG passed the pattern,
language, full-language, CST, diagnostics, source-corpus, generated-C,
metadata, stress, installed-relocation, and codegen suites. The frozen CST,
source-corpus, generated-C, and stress receipts are recorded in the C2 artifact;
the Zig PEG parser fixture remains pinned for rollback and old-image/parser
diagnostic recovery. The PEG row remains blocked/pinned pending its focused
ledger commit and strict R6 checkpoint.

## 8. Phase C3 — native-Birnam policy closure

1. Re-run the checked 120-callsite owner inventory after each C2 rung and
   preserve its owner class for every callsite.
2. Preserve the reviewed `z2_1_seam.status=none` disposition from Design 67;
   do not reopen policy movement or change an owner class without a new
   inventory row, invariant review, and receipt.
3. For each message sent by Rust host/helper code, name its checked owner class
   and Store/MOP observation receipt. Only a callsite classified as
   `policy-class` must name a native-Birnam policy definition; a missing owner
   is an implementation gap, not permission to add a Rust method.
4. If a genuine duplicated policy seam is found, port the policy to the
   existing `.birnam` owner first, add a cross-floor test, and only then remove
   the duplicated Rust/Zig observation path. If no seam is found, record
   `status=none` and make no runtime change.
5. Re-run source bootstrap, project/LSP, image, isolate, and relational tests
   after every policy-boundary change.

**C3 gate:** the owner ledger is complete for all existing classes, every
`policy-class` send names a native-Birnam owner, the MOP law remains green, no
Rust native vtable shadows a `.birnam` policy method, and strict R6 passes.

## 9. Phase C4 — final fixture retirement and clean bootstrap

After the named release/rollback observation for every member of the complete
Zig vector, C4 chooses exactly one separately reviewed mode:

1. **Retain mode:** make no deletion, keep every vector member
   `rollback_status=pinned`, retain a fixed recovery archive, and rerun both
   prefix-confined production and complete-vector rollback traces.
2. **Retire mode:** remove the complete Zig host plus all 14 dynamic fixtures
   from ordinary build/test and distribution paths in one atomic change; set
   every row's `rollback_status=not-applicable`; do not run a rollback trace
   after removal, and require production-only archive/bootstrap, relocation,
   old image/native-client, cross-floor, hostile-input, GC, and strict R6
   checks with fixtures unavailable.
3. Update the conversion ledger and inventory disposition in the same focused
   commit. A partial retained vector is forbidden unless a separately
   versioned mixed-vector design is reviewed. Tag the prior complete-vector
   state; a Git tag supplements but does not replace the recovery archive.

The final C4 gate has no Zig production provider, no oracle binary in a
production bundle, no ambient fallback, and no dynamic dependency on a Zig
implementation. Retain mode additionally requires the complete rollback
archive and trace; retire mode requires two independent Rust production
archives and the complete source/bootstrap proof with fixtures unavailable.

## 10. Commit and review discipline

- Every numbered implementation item is a small commit after its focused
  tests. Do not combine a ledger/checker change with a provider or policy
  change.
- Before C0 changes: `git diff --check`, inventory checker, focused R6, and
  strict R6 when the receipt or production boundary changes.
- Before C1/C2/C3/C4 owner, loader, fixture, or distribution changes:
  `BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh`, the relevant
  archive/relocation/cross-floor corpus, and the exact receipt checker.
- Keep the worktree clean between slices. Never use a broad destructive cleanup
  to manufacture a clean archive; use isolated temporary prefixes and caches.
- On a failure, first appeal to the invariants: preserve the dynamic boundary,
  dictionary authority, bootstrap source authority, and explicit recovery path.
  Stop for human input only if the failure would require changing a frozen
  ABI/layout/protocol, removing the only recovery path, or choosing between
  incompatible policy authorities with no evidence-based resolution.

## 11. Acceptance checklist

- [x] Conversion ledger and checker cover every current Zig file and all 14
      canonical dynamic names.
- [x] Before C4, production and complete-vector rollback traces are
      prefix-confined and mutually exclusive; C4 retain mode keeps both, while
      retire mode records `rollback_status=not-applicable` and runs production
      traces only.
- [x] Rust host and artifact preflight own observation/closure; native Birnam
      owns activation and policy.
- [ ] Every rung has a Rust identity/digest, Zig fixture policy, and applicable
      ABI/MOP/GC/image/wire evidence.
- [ ] The first three rung slices (floor, Store, PEG) have focused commits and
      strict R6 receipts.
- [ ] All later rung decisions are separately reviewed and reversible.
- [ ] No Rust provider statically exports another rung's `module_init` or
      identity.
- [ ] Two Rust production archives, plus the explicit complete-vector rollback
      archive in retain mode (or the no-fixture source bootstrap in retire
      mode), relocation, and hostile-input gates pass.
- [ ] The final strict R6 receipt is remeasured after C0 and every boundary
      change; it retains `516/522`, six skipped, and only the two pinned
      benchmark crossings only when the live run proves those values. The
current baseline is `307/307` verify steps.
- [ ] Documentation, inventory, distribution, and rollback instructions match
      the implemented ownership state.

## 12. Stop conditions

Do not delete a Zig source, fixture, ABI definition, image, or native client
when it is the only reproducible recovery evidence. Do not add a Rust/native
replacement merely to make a count smaller. Preserve the current authority
model and record a retained fixture when the invariants require it; that is a
successful conversion of production authority, not an unfinished hidden
fallback.
