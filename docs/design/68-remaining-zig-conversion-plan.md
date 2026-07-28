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
- each of the 14 canonical dynamic provider names has exactly one Rust owner,
  one `module_init` authority, one identity/digest, and an explicit rollback
  fixture policy;
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
- `src/id/artifact_inspector.zig` and `artifact_preflight.zig`: Zig oracle and
  negative/compatibility evidence after Rust codec and host preflight became
  canonical;
- the 14 Zig dynamic rung sources: named rollback/oracle fixtures;
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
2. **Dynamic boundaries remain explicit.** A production rung is a dynamic
   library with exactly one `module_init(IdApi*, void*)`; Rust workspace
   membership must not statically fold another provider's initializer,
   identity, or projection into the selected dylib.
3. **Dynamic dependency manifests name libraries only.** Rust crate paths,
   Zig source paths, static TCC objects, test executables, and policy files are
   graph/coverage evidence, never entries in `dynamic_dependencies`.
4. **Object dictionary authority.** The live vtable dictionary remains the
   dispatch authority. Store is the derived MOP mirror and the supported
   post-bootstrap mutation chokepoint. For each direct binding the mirror is
   exactly the live Method, `#primitive` for an unreified backfill, or absent;
   it never copies inherited methods or names a stale method.
5. **Native policy is not reimplemented.** `stdlib.birnam`,
   `project-policy.birnam`, and native module sources remain the protocol and
   language-policy authority. Rust may observe, validate, load, and publish;
   it may not install a competing policy vtable.
6. **Bootstrap independence.** Production can be cold-bootstrapped from a
   source checkout using declared non-Birnam toolchains, without a prebuilt
   Birnam binary/image/module or a hidden Zig runtime artifact. This plan does
   not require deleting the Zig compiler toolchain; toolchain retirement is a
   separate decision.
7. **Re-entry and ownership.** No Rust borrow, lock, temporary pointer, or
   mutable state borrow spans `send`, `dlopen`, TCC, finalizers, root/trace
   callbacks, or module initialization. C callbacks never unwind a Rust panic.
8. **Fail closed and atomically.** Invalid ABI, identity, path, descriptor,
   image, wire, generation, or dependency input rejects before activation and
   leaves the previous dictionary, Store, and filesystem target unchanged.
9. **Recovery is explicit.** A Zig fixture may remain only in a named isolated
   prefix or checked-in compatibility archive with fixed identity/digest. No
   source deletion is allowed until the recovery evidence is stronger than the
   source being deleted.

## 4. Conversion ledger and dispositions

The ledger is machine-checked and has one row per Zig path and one row per
canonical dynamic name. Each row contains:

```text
path/name, kind, current_owner, target_owner, production_status,
rollback_status, identity_source, digest_source, required_fixtures,
next_phase, removal_authority, last_receipt
```

Allowed `target_owner` values are `rust-dylib`, `rust-host`,
`native-birnam-policy`, `shared-contract`, and `evidence-fixture`. Allowed
production statuses are `canonical`, `never-production`, and `retired`; allowed
rollback statuses are `required`, `pinned`, and `not-applicable`.

The initial ledger must classify the residue as follows:

| Residue | Target owner | Production disposition | Recovery disposition |
| --- | --- | --- | --- |
| `main.zig` and host helpers | `rust-host` | never-production | retain explicit Zig host oracle until release decision |
| `artifact_inspector.zig` / `artifact_preflight.zig` | `rust-host` + `shared-contract` | never-production | retain parity and sentinel fixtures |
| 14 Zig dynamic rung sources | corresponding `rust-dylib` | never-production | one named fixture decision per rung |
| `idapi.zig`, `artifact.zig` | `shared-contract` | retained contract | keep for Zig oracle/ABI evidence |
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
   `abi/remaining-zig-v1.json`.
2. Add `scripts/check-remaining-zig-conversion.py`. It fails on an unclassified
   Zig file, a `rust-dylib` row without a canonical dynamic owner, a production
   path containing `_zig` or `birnam-zig-oracle`, a missing rollback reason, an
   ambiguous `.birnam` policy owner, or a `dynamic_dependencies` entry that is
   not a declared dylib name.
3. Add `zig build test-remaining-zig-conversion` and make `zig build test` run
   it. The checker must be read-only and deterministic from any cwd.
4. Record the current commit, selected defaults, inventory receipt, Rust
   identity/digest sources, and exact strict-R6 receipt in the ledger.

**C0.1 gate:** the checker, ordinary test graph, `git diff --check`, and the
scoped plus strict R6 commands pass. No provider selection changes.

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

1. Keep `birnam-artifact` and `birnam-artifact-codec` pointer-free and
   stateless. They must not export `module_init`, IdApi globals, activation, or
   closure policy.
2. Keep `libbirnam_id_artifact.dylib` as the sole dynamic Artifact projection.
   The Rust host alone performs filesystem sibling closure and preflight.
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

All 14 rungs already have Rust canonical providers. C2 converts the remaining
Zig implementations by proving that they are never production authorities and
then making one separately reviewed fixture decision at a time. The order is:

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
6. a statement choosing `retain-pinned-fixture` or `retire-fixture`, with a
   reversible archive/tag and the exact removal commit if retiring; and
7. a fresh strict R6 receipt after the choice.

### C2.1 First three bounded rung slices

These are the first implementation slices after plan review.

**C2.1a — floor:** update the floor ledger row with current Rust/Zig digests,
rerun the 256-test lower-rung matrix, old native-client/image corpus, forced-GC
and MOP checks, and record the decision. The invariant-based default is
`retain-pinned-fixture` until a complete Rust release and full-Zig rollback
history exists; no source deletion is implied.

**C2.1b — Store:** rerun the Store consistency-law suite under Zig and Rust
Debug/ReleaseSafe, including tracked/raw mutation, backfill, authorizer,
observer, image boot, and inherited revelation. Retain the Zig fixture unless
the receipt proves recovery without it; never remove the Store mirror contract.

**C2.1c — PEG:** rerun CST/metadata/diagnostic/source corpus and generated-C
parity under Rust and the explicit Zig parser fixture. Preserve the parser
fixture while any old image, parser diagnostic, or generated-C recovery path
depends on it.

Each slice is one commit after focused tests, followed by strict R6 before the
next slice. A fixture-retirement commit may not include another rung or a
policy change.

## 8. Phase C3 — native-Birnam policy closure

1. Re-run the checked 120-callsite owner inventory after each C2 rung.
2. For every message sent by Rust host/helper code, name the native-Birnam
   policy definition and the Store/MOP observation receipt. A missing owner is
   an implementation gap, not permission to add a Rust method.
3. If a genuine duplicated policy seam is found, port the policy to the
   existing `.birnam` owner first, add a cross-floor test, and only then remove
   the duplicated Rust/Zig observation path. If no seam is found, record
   `status=none` and make no runtime change.
4. Re-run source bootstrap, project/LSP, image, isolate, and relational tests
   after every policy-boundary change.

**C3 gate:** the owner ledger is complete, the MOP law remains green, no Rust
native vtable shadows a `.birnam` policy method, and strict R6 passes.

## 9. Phase C4 — final fixture retirement and clean bootstrap

Only after the named release/rollback observation for a rung:

1. remove that Zig fixture from ordinary build/test and distribution paths;
2. retain a fixed test-only archive or explicit recovery bundle if the
   recovery policy requires it;
3. update the conversion ledger and inventory disposition in the same focused
   commit;
4. rerun production/rollback traces, archive/bootstrap, old image/native
   client, cross-floor, hostile-input, GC, and strict R6 checks; and
5. tag the prior fixture state before proceeding to the next rung.

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
- [ ] The final strict R6 receipt remains `306/306`, `516/522`, six skipped,
      with only the two pinned benchmark crossings.
- [ ] Documentation, inventory, distribution, and rollback instructions match
      the implemented ownership state.

## 12. Stop conditions

Do not delete a Zig source, fixture, ABI definition, image, or native client
when it is the only reproducible recovery evidence. Do not add a Rust/native
replacement merely to make a count smaller. Preserve the current authority
model and record a retained fixture when the invariants require it; that is a
successful conversion of production authority, not an unfinished hidden
fallback.
