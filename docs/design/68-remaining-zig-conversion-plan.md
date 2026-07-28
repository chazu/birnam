# 68 — Remaining Zig Conversion and Native-Birnam Completion Plan

> **Status:** draft pending adversarial review. This plan starts from the
> accepted Rust floor/native-rung state at `rust-libid-floor` and is the
> execution plan for the Zig residue that remains after the Rust conversion.
> It supersedes neither Design 66's residue map nor Design 67's acceptance
> records; it turns their current evidence into a bounded conversion and
> retirement sequence.

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
- the canonical floor has exactly one Rust `id_init` owner and each of the 13
  higher dynamic provider names has exactly one Rust `module_init` authority,
  one identity/digest, and an explicit rollback fixture policy;
- host observation and artifact preflight are Rust-owned, while
  `Module loadArtifact:` and all `.birnam` policy remain native-Birnam-owned;
- every retained Zig provider is reachable only through an explicit isolated
  oracle/rollback selection and cannot be discovered by production resolution;
- the IdApi, Oop/vtable representation, payload/layout descriptors, generated-C
  ABI, image-v4 records, isolate wire-v1 records, and Store/MOP law are unchanged;
- the conversion ledger, inventory checker, archive/bootstrap, relocation,
  cross-floor, hostile-input, GC, image, and strict R6 receipts pass at the
  same commit; and
- each fixture-retirement decision is separately reviewable and reversible.

## 2. Current evidence and residue boundary

The current checker reports:

```text
remaining-zig-inventory: ok zig_files=88 birnam_sources=91 rust_crates=20 test_scripts=78 build_targets=124 dynamic_dependencies=14 explicit_edge_rows=32 derived_coverage_rows=290
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
focused: Build Summary: 216/216 steps succeeded; 99/99 tests passed
verify:  Build Summary: 306/306 steps succeeded; 516/522 tests passed; 6 skipped
live Alien: 66/66 steps succeeded; 9/9 tests passed
distribution-selection: production omits Zig; rollback is a complete Zig vector
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
benchmark: regressions=2, missing=0, unusable=0
accepted: query.aggregate +5.8%, compile.stage1-stdlib +11.9%
```

This is the baseline for every authority or distribution change. Counts and
digests are measured receipts, not values to be guessed or widened.

## 3. Invariants and non-negotiable decisions

1. **One runtime owner.** A process has one IdApi floor, heap, interner, Store,
   generation registry, and canonical family graph. No conversion step may
   load a Rust provider and its Zig counterpart in one production process.
2. **Dynamic boundaries remain explicit.** The production floor is a dynamic
   library with exactly one `id_init(IdApi*, void*)`; each higher rung has
   exactly one `module_init(IdApi*, void*)`. Rust workspace membership must not
   statically fold another provider's initializer, identity, or projection
   into the selected dylib.
3. **Dynamic dependency manifests name libraries only.** Rust crate paths,
   Zig source paths, static TCC objects, test executables, and policy files are
   graph/coverage evidence, never entries in `dynamic_dependencies`.
4. **Object dictionary authority and MOP law.** The live vtable dictionary
   remains the dispatch authority. Store is the derived MOP mirror and the
   supported post-bootstrap mutation chokepoint. Every change runs
   authorizer-before-change, performs the tracked add/replace/remove update,
   commits the live dictionary and mirror together, then invokes exactly one
   post-commit observer. For each direct binding the mirror is exactly the
   live Method, `#primitive` for an unreified backfill, or absent; it never
   copies inherited methods or names a stale method. Raw bootstrap/private
   mutation may create the initial state with an honest absence; backfill is
   direct-only and never reveals inherited methods.
5. **Native policy is not reimplemented.** `stdlib.birnam`,
   `project-policy.birnam`, and native module sources remain the protocol and
   language-policy authority. Rust may observe, validate, load, and publish;
   it may not install a competing policy vtable.
6. **Bootstrap independence.** Production can be cold-bootstrapped from a
   source checkout using a declared cold stage-0 chain and the native-Birnam
   compiler/driver built by that chain. Bootstrap rejects prebuilt/generated
   Birnam dylibs, images, modules, and generated output as inputs and does not
   discover a hidden Zig runtime artifact. This plan does not require deleting
   the Zig compiler toolchain; toolchain retirement is a separate decision.
7. **Re-entry and ownership.** No Rust borrow, lock, temporary pointer, or
   mutable state borrow spans `send`, `dlopen`, TCC, finalizers, root/trace
   callbacks, or module initialization. Unsafe boundaries must cover resolved
   Method/DNU calls, mutation hooks, Alien callbacks, dynamic-extent collector
   marks, rooted callback state, and process-lifetime handles. C callbacks never
   unwind a Rust panic; every callback path has an explicit owner and lifetime.
8. **Fail closed and atomically.** Invalid ABI, identity, path, descriptor,
   image, wire, generation, or dependency input rejects before activation and
   leaves the previous dictionary, Store, and filesystem target unchanged.
9. **Recovery is explicit.** A Zig fixture may remain only in a named isolated
   prefix or checked-in compatibility archive with fixed identity/digest. No
   source deletion is allowed until the recovery evidence is stronger than the
   source being deleted.

## 4. Conversion ledger and dispositions

The ledger is machine-checked and has one row per Zig path and one row per
canonical dynamic name. Path keys are `zig:path:<repo-relative path>` and
provider keys are `dynamic:name:<canonical dylib name>`. Each row contains:

```text
path/name, kind, current_owner, target_owner, production_status,
rollback_status, identity_source, digest_source, required_fixtures,
next_phase, removal_authority, last_receipt
```

Allowed `target_owner` values are `rust-dylib`, `rust-host`, `build-only`,
`native-birnam-policy`, `shared-contract`, and `evidence-fixture`. Allowed
`production_status` values are `canonical`, `never-production`, and `retired`;
allowed `rollback_status` values are `required`, `pinned`, and
`not-applicable`. `identity_source`, `digest_source`, `required_fixtures`,
`next_phase`, `removal_authority`, and `last_receipt` are either populated
strings/arrays or explicit `null` when not applicable; omitted fields are
invalid. A separate checked owner section covers `.birnam` policy and never
creates dynamic-provider rows.

The checker validates unique keys, enum values, nullable fields, and
referential integrity against `abi/remaining-zig-v1.json` and every provider
identity manifest. The ledger records an immutable `baseline_commit`. The
checker receives the candidate commit explicitly (or derives `HEAD`), validates
it, and emits it in the receipt; it is not stored in the ledger, avoiding a
self-referential commit field.

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
   export `module_init`, IdApi globals, activation, or closure policy.
   `birnam-artifact` is the dynamic Artifact provider and exports exactly one
   `module_init`; its provider identity is distinct from the codec crate.
   Apply the same distinction to provider `birnam-image` and provider-free
   `birnam-image-codec`.
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

**C1.2 gate:** the exact codec/projection/host/native ownership audit passes,
no cross-rung initializer or identity is exported, and strict R6/Z3 pass.

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
6. a statement choosing `retain-pinned-fixture`, `vector-retirement-ready`,
   or `blocked`. Per-rung deletion is not an allowed outcome: the complete
   Zig vector must remain intact until an explicitly reviewed atomic vector
   retirement; and
7. a fresh strict R6 receipt after the choice.

### C2.1 First three bounded rung slices

These are the first implementation slices after plan review. They produce
readiness receipts only; none may delete one provider from the rollback vector.

**C2.1a — floor:** update the floor ledger row with current Rust/Zig digests,
rerun the 256-test lower-rung matrix, old native-client/image corpus, forced-GC
and MOP checks, and record the decision. The invariant-based default is
`blocked` with `retain-pinned-fixture` until the Design 66 release-cycle
predicate, a complete Rust release, and full-Zig rollback history exist; no
source deletion or vector-retirement readiness is implied.

**C2.1b — Store:** rerun the Store consistency-law suite under Zig and Rust
Debug/ReleaseSafe, including tracked/raw mutation, backfill, authorizer,
observer, image boot, and inherited revelation. Retain the Zig fixture unless
the Design 66 release-cycle predicate is satisfied and the receipt proves
recovery without it; record `blocked`/`retain-pinned-fixture` until then and
never remove the Store mirror contract.

**C2.1c — PEG:** rerun CST/metadata/diagnostic/source corpus and generated-C
parity under Rust and the explicit Zig parser fixture. Preserve the parser
fixture and record `blocked`/`retain-pinned-fixture` while any old image, parser
diagnostic, generated-C recovery path, or the Design 66 release-cycle
predicate depends on it.

Each slice is one commit after focused tests, followed by strict R6 before the
next slice. A vector-retirement commit may not include another rung or a
policy change and must remove the complete Zig vector atomically.

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

Only after the named release/rollback observation for every member of the
complete Zig vector, and after a separately reviewed atomic retirement
decision:

1. remove the complete Zig vector from ordinary build/test and distribution
   paths in one atomic change; no individual provider may be deleted;
2. retain a fixed test-only archive or explicit recovery bundle if the
   recovery policy requires it;
3. update the conversion ledger and inventory disposition in the same focused
   commit;
4. rerun production/rollback traces, archive/bootstrap, old image/native
   client, cross-floor, hostile-input, GC, and strict R6 checks; and
5. tag the prior complete-vector fixture state before finalizing retirement.
   A Git tag supplements but does not replace the recovery archive.

The final C4 gate has no Zig production provider, no oracle binary in a
production bundle, no ambient fallback, and no dynamic dependency on a Zig
implementation. It must pass two independent Rust production archive builds,
the complete source/bootstrap proof with fixtures unavailable, and the explicit
rollback archive if any fixture remains.

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

- [ ] Conversion ledger and checker cover every current Zig file and all 14
      canonical dynamic names.
- [ ] Production and rollback traces are prefix-confined and mutually
      exclusive.
- [ ] Rust host and artifact preflight own observation/closure; native Birnam
      owns activation and policy.
- [ ] Every rung has a Rust identity/digest, Zig fixture policy, and applicable
      ABI/MOP/GC/image/wire evidence.
- [ ] The first three rung slices (floor, Store, PEG) have focused commits and
      strict R6 receipts.
- [ ] All later rung decisions are separately reviewed and reversible.
- [ ] No Rust provider statically exports another rung's `module_init` or
      identity.
- [ ] Two Rust production archives, one explicit rollback archive, relocation,
      source bootstrap, and hostile-input gates pass.
- [ ] The final strict R6 receipt is remeasured after C0 and every boundary
      change; it retains `516/522`, six skipped, and only the two pinned
      benchmark crossings only when the live run proves those values. The
      current baseline is `306/306` verify steps.
- [ ] Documentation, inventory, distribution, and rollback instructions match
      the implemented ownership state.

## 12. Stop conditions

Do not delete a Zig source, fixture, ABI definition, image, or native client
when it is the only reproducible recovery evidence. Do not add a Rust/native
replacement merely to make a count smaller. Preserve the current authority
model and record a retained fixture when the invariants require it; that is a
successful conversion of production authority, not an unfinished hidden
fallback.
