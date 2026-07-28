# 69 — Remaining Zig Modules: Rust Host and Native-Birnam Plan

> **Status:** Draft awaiting adversarial review. Baseline: Design 68 C4 retain
> receipt at `68c5bce`; production already selects the Rust floor, all 13 Rust
> higher providers, and the Rust host. This plan converts the remaining Zig
> *authorities* and build/oracle seams; it does not delete recovery evidence.

## 1. Objective and boundary

Convert every remaining Zig module that could still be mistaken for a
production authority to one of these explicit owners:

1. Rust host/build/resource observation and unsafe OS/FFI mechanism;
2. the existing native-Birnam policy/protocol surface; or
3. a named shared contract or evidence/rollback fixture with no production
   selection path.

The scoped implementation residue is:

| Zig surface | final production owner | allowed retained role |
| --- | --- | --- |
| `src/id/main.zig` | `rust/birnam-host` executable | explicit Zig host oracle only |
| `project.zig`, `project_build.zig`, `project_check.zig`, `runtime_resources.zig` | Rust host observation/build API | oracle/evidence until the Rust build driver graduates |
| `artifact_inspector.zig` | Rust Artifact dylib projection and codec | Zig rollback/oracle vector |
| `artifact_preflight.zig` | Rust host preflight permission | negative oracle/sentinel fixture |
| `build.zig` | staged Rust build driver, with native-Birnam policy unchanged | Zig stage-0 orchestrator until driver graduation |
| `id.zig`, `store.zig`, `peg.zig`, `lang.zig`, `tcc.zig`, `codegen.zig`, `prelude.zig`, `collection.zig`, `error.zig`, `isolate.zig`, `relational.zig`, `alien.zig`, `image.zig` | Rust dynamic providers (carried forward from Design 68) | pinned complete rollback vector only |
| `idapi.zig`, `artifact.zig`, `abi/idapi_manifest.zig` | frozen shared ABI/schema | contract input to the named rollback oracle |
| Zig probes, tests, and sentinels | evidence fixtures | failure/compatibility evidence |
| `src/id/*.birnam`, `modules/*.birnam` | native-Birnam policy | canonical policy; never shadow in Rust |

“Converted” means no canonical production path can load or call the Zig
implementation. It does not mean deleting a fixture that is still the only
reproducible rollback or negative evidence. Design 68's complete Zig rollback
vector—`bin/birnam-zig-oracle` plus the 14 dynamic fixtures in
`abi/rollback-prefix-v1.json`—remains intact and mutually exclusive from
production throughout this plan.

## 2. Invariants and frozen decisions

1. **Toolchain language is not constitutional.** Rust, Zig, C, or a later
   native-Birnam driver may implement mechanism. A clean archive may be
   bootstrapped from another language, but may not consume a prebuilt Birnam
   executable, image, dylib, compiled module, or generated Birnam output.
2. **One runtime owner.** A process has one `IdApi`, heap, interner, Store,
   generation registry, and family graph. The host never loads a second floor
   or a Zig provider beside its Rust provider.
3. **Dynamic names only.** Runtime dependency manifests contain installed
   dylib names only. Rust crates, Zig source paths, static TCC objects,
   executables, and `.birnam` policy are graph/build evidence, not dynamic
   dependencies.
4. **Frozen ABI.** `id_init() -> IdApi*`, every
   `module_init(IdApi*, void*) -> void*`, Oop/vtable and payload layouts,
   generated-C symbols, image-v4, isolate wire-v1, install names, and
   diagnostic identities do not change.
5. **MOP law.** The live object dictionary is dispatch authority; Store's MOP
   mirror is derived and direct-only. For each direct binding the mirror is
   exactly the live Method, `#primitive` for an unreified direct binding, or
   honest absence; it never copies inherited methods or reveals inherited
   facts. Supported mutation is authorizer-before, `mutate → assert` for add,
   `retract → mutate → reassert` for replace, and `retract → remove` for
   remove, followed by exactly one observer. Raw mutation is bootstrap/private
   only; direct-only backfill cannot reveal inherited methods. Native policy
   owns semantic decisions; Rust only performs mechanism and validates
   results.
6. **Unsafe/re-entry boundary.** No Rust borrow, lock, temporary pointer, or
   mutable reference crosses a resolved Method/DNU call, mutation hook,
   `dlopen`, TCC, finalizer, root/trace provider, collector mark, module
   initialization, Alien callback, or loader call. Copy callback-visible
   values out, end the Rust borrow, call foreign code, then reacquire. C
   callbacks cannot unwind a Rust panic; dynamic-extent marks cannot escape;
   callback-visible objects remain rooted; process-lifetime dylib handles stay
   loaded for every live Method, layout, and callback pointer.
7. **Host/artifact split.** Rust owns filesystem closure, path containment,
   descriptor decoding, and preflight permission. Native-Birnam `Module
   loadArtifact:` alone owns dependency ordering, initializer delegation,
   Store/MOP publication, and activation. The host never calls an artifact
   initializer as an effect.
8. **Policy single ownership.** A `.birnam` selector/protocol is never copied
   into a Rust vtable. A policy move requires an owner row, a red test proving
   the old branch is unused, and cross-floor MOP/source-corpus evidence.
9. **Fail closed and atomic.** Bad manifests, paths, descriptors, identities,
   dependency closures, generated-C, or build records reject before effects;
   dictionary, Store, publication target, and prior build remain unchanged.
10. **Recovery is explicit.** The complete Zig rollback vector is the Zig host
    oracle plus all 14 dynamic fixtures named by
    `abi/rollback-prefix-v1.json`. It is selected only by a named
    rollback/oracle prefix. Partial vectors, ambient loader fallback, and
    source-tree discovery from relocated production are rejected.

## 3. Current evidence and completion definition

Baseline receipts at `68c5bce`:

```text
remaining-zig-inventory: ok zig_files=88 birnam_sources=91 rust_crates=20 test_scripts=80 build_targets=125 dynamic_dependencies=14 explicit_edge_rows=32 derived_coverage_rows=290
remaining-zig-conversion: ok zig_paths=88 dynamic_names=14 policy_sources=39
focused: 218/218 steps; 99/99 tests
verify: 307/307 steps; 516/522 tests; 6 skipped
live Alien: 66/66 steps; 9/9 tests
distribution-selection: production omits Zig; rollback is a complete Zig vector
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

The plan is complete only when:

- `bin/birnam` is a stable Rust-host identity (the `experimental` name is no
  longer the canonical user-facing contract), and helper-boundary checks show
  no Rust dependency on Zig helpers;
- check/build/install/run/test/LSP, resource resolution, source bootstrap, and
  artifact preflight are Rust-owned and parity-tested from two unrelated
  directories;
- every native-Birnam policy callsite has one owner and the owner inventory is
  unchanged or has a reviewed transfer record;
- a source-built Rust build driver can reproduce the declared install graph,
  stage-1 module compilation, archive relocation, and test isolation without
  taking a prebuilt Birnam artifact as input;
- `build.zig`, Zig host helpers, and Zig Artifact/preflight code are either
  unreachable from production and explicitly classified as evidence/oracle or
  retired only by a separately reviewed atomic vector/toolchain decision; and
- inventory, owner, ABI/MOP, bootstrap, archive, hostile-input, and strict R6
  receipts pass at the same clean commit.

## 4. Phase M0 — authority ledger and seam freeze

M0 is documentation/checker-only and must land before code changes.

1. Extend the existing authoritative `abi/remaining-zig-conversion-v1.json`
   only through a versioned supplemental
   `abi/remaining-zig-native-actions-v1.json`; do not duplicate its 88-path,
   14-provider, or 39-policy owner rows. Every helper, build, oracle, and
   policy action has a unique `action_key`, an `owner_key` foreign key to an
   existing ledger row or a declared supplemental mechanism-owner registry
   entry, `action_kind`, `selection_context`, `delegation`, checked function
   and/or tool reference, test reference, receipt reference, `build_authority`,
   `abi_surface`, `removal_authority`, and `last_receipt`. The supplemental
   mechanism-owner registry may add only non-path owners such as
   `owner:rust-build-driver`; each entry cites its canonical ledger scope and
   cannot duplicate a source/provider row. The closed
   `action_kind` enum is `host-observation`, `artifact-preflight`,
   `policy-review`, `build-action`, `provider-install`, `stage1-compile`,
   `test`, `package`, `rollback`, `bootstrap`, or `oracle`; the closed
   `selection_context` enum is `production`, `rollback`, `oracle`, `build`,
   or `evidence`. `delegation` is `none` or `explicit-shadow` and is rejected
   in a canonical Rust-driver production receipt. `build_authority` is one of
   `canonical`, `shadow`, `oracle`, or `retired`, independent of runtime
   `production_status`. Non-build actions must use `delegation=none`; only
   shadow build actions may use `explicit-shadow`.
2. Add explicit helper operation rows for URI decoding, project discovery,
   manifest/dependency resolution, build-record publication, resource lookup,
   artifact preflight, and startup. Each row names the Rust function and the
   Zig oracle function; no row may claim Rust policy ownership.
3. Add only selector/action review rows for native policy using that same
   schema. Their `owner_key` must reference a canonical `birnam:path:` row in
   Design 68's ledger. `no-seam` is a selector-transfer disposition, never a
   new source owner. Duplicate action keys, missing owner keys, invalid enum
   values, missing function/tool/test/receipt references, and an owner key that
   is neither a canonical ledger row nor a declared mechanism-owner entry are
   hard failures; policy actions still require a native-policy ledger owner.
4. Extend the existing read-only checker rather than adding a second runtime
   gate. It must reject a production path containing `project.zig`,
   `artifact_preflight.zig`, `_zig`, or `birnam-zig-oracle`, and reject any
   source/crate/static object placed in `dynamic_dependencies`.
5. Use the transaction `candidate checkpoint → clean-tree focused gates →
   receipt commit referencing the candidate → post-receipt gate`. The checker
   must accept an explicit candidate commit/tree hash and refuse a dirty tree;
   it must never receipt an uncommitted candidate. Run inventory,
   helper-boundary, owner-inventory, Artifact-authority, and strict R6 at the
   recorded checkpoints.

**M0 exit:** every supplemental action foreign-keys the canonical ledger,
every `.birnam` action references one canonical native-policy owner, build/runtime
axes are separate, dynamic dependencies remain library-only, and
production/rollback selection is unchanged.

## 5. Phase M1 — make the Rust host canonical and observable

M1 changes names/packaging and adds parity evidence; it does not alter the
IdApi or provider graph.

### M1.1 Canonical binary identity

1. Keep the Cargo target named `birnam` as the canonical host and keep
   `bin/birnam` as the only production install path. Remove the secondary
   `birnam-rust-experimental` target and its user-facing wording; any temporary
   probe binary must be a test-only target with a distinct name and must not be
   installed into a production prefix.
2. Preserve the existing `birnam` CLI contract while changing only the error
   prefix/help text needed to remove the experimental name.
3. Add a receipt asserting the Rust host has no Zig helper imports, no source
   checkout fallback, and no dynamic Zig load.

### M1.2 Host parity matrix

1. For each M0 helper row, run the Rust and explicit Zig oracle from unrelated
   cwd values with `BIRNAM_LIB_DIR`, `BIRNAM_ID_DYLIB`, and workspace overrides
   unset; normalize only paths and timestamps.
2. Cover check/build/install/run/test/LSP, URI/path errors, missing and cyclic
   dependencies, source digest changes, stale build records, resource
   overrides, and project test worker isolation.
3. Add a red fixture for every mismatch class before any implementation fix;
   fixes must stay in Rust mechanism or native-Birnam policy as dictated by
   the owner row.

**M1 exit:** Rust and Zig traces agree on result shape, diagnostic identity,
first-failure order, and filesystem effects; Rust is the sole production host.

## 6. Phase M2 — artifact and preflight retirement seam

1. Prove the Rust codec is pointer-free and pure; prove the Rust dynamic
   Artifact projection has exactly one `module_init` and no foreign provider
   exports. Run dependency, identity, format, path-escape, native-kind,
   duplicate, truncation, and aborting-sentinel cases in Debug and ReleaseSafe.
2. Assert every rejection leaves publication target, Store, MOP, and prior
   build record unchanged. The host must return a typed permission result and
   never invoke an initializer.
3. Keep `artifact_inspector.zig` and `artifact_preflight.zig` only in the
   explicit Zig oracle/negative corpus. Update their ledger rows to
   `never-production`; do not remove them while the complete vector or
   negative recovery archive depends on them.

**M2 exit:** Rust host/preflight and native `Module loadArtifact:` are the only
production closure/activation owners, with a normalized Rust/Zig negative
corpus receipt.

## 7. Phase M3 — native-Birnam policy audit and (only if proven) transfer

1. Re-run `abi/rust-host-message-owner-v1.json` and the 120-callsite owner
   checker. Preserve the checked owner class for every send/global lookup:
   `policy-class` rows name a native selector/source, while runtime,
   provider, ABI, and observation classes retain their existing non-policy
   owner. Compare every callsite with its checked owner; only `policy-class`
   rows require a native selector/source, and not every callsite requires a
   `.birnam` definition.
2. If `z2_1_seam.status=none` remains true, record a no-op receipt and make no
   runtime change. This is the expected invariant-derived outcome.
3. If a genuine duplicate is found, write an authority-transfer row naming the
   old Rust branch, native selector, result-shape validation, Store/MOP effects,
   and bootstrap dependency. Add a red test, move only that branch into the
   existing `.birnam` owner, then run policy/MOP, source-corpus, image, and
   cross-floor gates.

**M3 exit:** no Rust policy vtable shadows native Birnam, or one reviewed
transfer is proven with the old branch unreachable.

## 8. Phase M4 — Rust build-driver bootstrap

`build.zig` is the only remaining Zig build authority. Replace it in slices;
do not rewrite the whole graph at once. Until the final cutover it remains
`build_authority=canonical` for production builds; the Rust driver is
`shadow` and delegates only in explicitly recorded experiments.

### M4.1 Freeze the build-action DAG (first code slice)

1. First inventory the checked build-action DAG from the current 125 build
   targets: tool, inputs, outputs, feature/flags, environment, dependency
   edges, install destination, and digest policy. Include Cargo workspace
   builds, 14 providers, host, vendored libtcc C sources/flags, generated
   options, source/resource installation, the 26 ordered stage-1 modules,
   tests, packaging, and rollback fixtures. Mark each action as production,
   rollback, oracle, build, or evidence.
2. Add `abi/birnam-build-actions-v1.json` and a drift checker. It must carry
   unique action keys, canonical owner keys, tool/input/output/flag/
   environment/dependency/install fields, runtime-vs-build selection axes,
   delegation (`none` or `explicit-shadow`), and explicit production,
   rollback, oracle, build, or evidence context. The checker must prove all
   current 125 targets/actions are classified, reject delegation in any
   canonical Rust-driver production receipt, and emit an exact clean-tree
   receipt. Commit this manifest/checker before implementing the driver.

**M4.1 exit:** the current `build.zig` action DAG is frozen in a supplemental
manifest, all actions have owners and contexts, and drift fails closed.

### M4.2 Manifest parser and dry-run driver

1. Add a Rust `birnam-build` binary in `rust/birnam-host` (or a small new
   workspace crate) with a versioned manifest schema containing the action-DAG
   identity, target triple, profile, selected floor/provider identities,
   source modules, dependency order, dynamic install names, output prefix,
   tool paths, normalized environment, and input/output digests. Runtime
   dependencies list only dylib names.
2. Implement strict parsing, path containment, identity checks, and a dry-run
   graph printer. It must reject `_zig` in production, mixed vectors, missing
   source digests, and undeclared dynamic names before spawning a compiler.
3. Add unit tests for malformed manifests, path escape, duplicate names,
   dependency cycles, stale digests, and mixed Rust/Zig selection. Commit this
   slice independently.

### M4.3 Compiler invocation and stage-1 module compilation

1. Implement a process runner that invokes declared non-Birnam stage-0 tools
   (the Zig toolchain may remain the temporary C/Zig compiler) without loading
   a Birnam runtime artifact. `birnam-build` and the Rust host/compiler used
   for `--compile-module` must be built from source earlier in the same cold
   run, selected by absolute staged paths, and verified by identity/digest;
   prebuilt Birnam driver/host/compiler executables are rejected. Every
   command is logged with normalized argv, cwd, inputs, outputs, and digest.
2. Reproduce floor/provider dylib installation and the existing ordered
   `.birnam` stage-1 list. Compile modules through the canonical Rust host
   `--compile-module` boundary; policy remains in the source modules.
3. Use atomic temporary prefixes and rename only after all receipts pass. A
   failed child, panic, or digest mismatch leaves no published target.

### M4.4 Driver parity and cold bootstrap

1. Run the Rust driver in shadow mode and `build.zig` (still canonical)
   against the same clean archive and compare normalized install manifests,
   dynamic names, generated-C, module digests, diagnostics, and source-corpus
   receipts. Require exact bytes only for artifacts proven reproducible; use
   an explicit normalization allowlist for timestamps, UUIDs, and other
   declared Mach-O/archive fields.
2. Run two independent source archives with no prebuilt Birnam artifacts,
   relocated project/LSP/install smoke, and explicit full-Zig rollback.
3. Do not switch the default build until two driver-generated Rust production
   archives pass the normalized receipt contract, and a Rust-driver-native
   production/bootstrap gate succeeds with `build.zig` unavailable. Remeasure
   and review-pin any changed inventory, focused-step, verify-step, or
   benchmark counts rather than requiring the old R6 numbers unchanged.

**M4 exit:** Rust driver can reproduce the production closure and stage-1
modules from source; `build.zig` remains canonical and the Rust driver remains
shadow until M5's atomic cutover.

## 9. Phase M5 — staged build-driver cutover and Zig quarantine

1. While `build.zig` remains canonical, cut over one build family at a time in
   **shadow** mode: resource resolution, floor/provider install, stage-1
   modules, project build/install, tests, packaging, then archive/rollback.
   Each family is one commit and one focused normalized-receipt comparison.
2. Do not quarantine `build.zig` during partial cutover. The shadow driver may
   delegate an unported family to it only when the action row says
   `delegation=explicit-shadow`, and that delegation is never claimed as a
   Rust-driver production receipt.
3. After every family passes, perform one atomic default-driver cutover commit
   that simultaneously changes all Rust-driver production actions
   `build_authority=shadow → canonical`, all corresponding `build.zig` actions
   `canonical → oracle`, switches the default entry point, updates
   source-bootstrap/distribution-selection/Z3 routing, and passes the already
   landed Rust-driver-native `build.zig`-unavailable production gate. From
   that commit onward, only the named rollback or toolchain-oracle path may
   use `build.zig`.
4. After every family and at the atomic cutover, run focused tests, the
   owner/inventory checkers, the Rust-driver-native archive gate, and strict R6.
   Any changed benchmark result fails closed.

## 10. Phase M6 — final disposition

1. After the atomic M5 transition, record the final archive, normalized
   receipts, and retained toolchain/oracle paths in one disposition commit.
   M6 does not perform a second authority switch: the M5 commit already changed
   Rust `shadow → canonical` and `build.zig` `canonical → oracle`.
2. If the Rust driver cannot yet replace `build.zig` orchestration while
   preserving the cold-chain contract, leave its action rows
   `build_authority=canonical` and record the exact blocker; this is an
   invariant-preserving partial completion, not permission to weaken the
   production gate. The Rust driver may continue invoking Zig as a declared
   non-Birnam stage-0 compiler tool.
3. Never delete `idapi.zig`, `artifact.zig`, the ABI manifest, negative
   sentinels, or the complete Zig vector in a per-file cleanup. Any future
   deletion is a separately reviewed atomic vector/toolchain decision.

## 11. Acceptance and commit discipline

Every numbered item is a bounded commit after its focused tests. Before a
production boundary change, run:

```text
python3 scripts/check-remaining-zig-inventory.py
python3 scripts/check-remaining-zig-conversion.py
python3 scripts/check-c2-rung-receipts.py
BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
```

Before the M5 atomic cutover, a Rust-driver-native production/bootstrap gate
must already build and archive with `build.zig` unavailable. The
Zig-orchestrated R6 remains compatibility/oracle evidence. Any new crate,
checker, build target, or driver test requires remeasuring and review-pinning
inventory, focused-step, verify-step, skip, and benchmark receipts; do not
assume the accepted `307/307`, `516/522`, six-skip, `66/66`, `9/9` snapshot is
unchanged. Every commit leaves a clean worktree. A failure is first resolved
by the invariants; stop for human input only if progress requires changing a
frozen ABI/protocol, deleting the sole recovery path, or choosing between
incompatible policy authorities with no evidence-based owner.

## 12. Initial work order

1. Review this document adversarially; correct it until GO.
2. Land M0 manifest/checker and receipt.
3. Land M1 canonical host identity and parity receipt.
4. Land M2 artifact/preflight retirement-seam receipt.
5. Land M3 policy no-seam/transfer receipt.
6. Freeze the M4.1 action DAG/checker, then implement M4.2 dry-run, M4.3
   compiler invocation, and M4.4 parity/native-bootstrap in separate commits
   before any default cutover.
7. Execute M5/M6 only after the Rust driver has two independent cold-archive
   proofs and the complete-vector rollback remains recoverable.
