# Remaining Zig disposition and native-Birnam migration plan

Status: **reviewed GO for the residue map and authority boundaries; Z0 is
R6-accepted, while Z1 evidence closure is still in progress** (2026-07-27).
The strict compatibility audit is green, but Z1–Z3
authority/distribution changes remain gated on the explicit strict gate as
specified below.

Design 63 and Design 65 port the production dynamic rungs and the Rust host.
This plan starts at that boundary. It covers the Zig code that remains after
the Rust canonical switch: the Zig oracle executable, Zig host/build helpers,
artifact-preflight evidence, and retained rollback fixtures. It does not
re-port a rung already owned by Rust, and it does not turn native Birnam policy
into Rust policy.

The outcome is a production Rust/native-Birnam closure that can be built,
installed, relocated, and operated without loading a Zig implementation. Zig
remains available as an explicit, isolated oracle/rollback fixture until a
separate release decision retires each fixture. “Converted” therefore means
removed from the production authority path, not silently deleted from the
repository.

## 1. Current boundary

The current default build selects Rust for the floor, Store, PEG, Lang, TCC,
Codegen, Prelude, Collection, Error, Isolate, Relational, Alien, Image,
Artifact, and host. The canonical dynamic names and `module_init` ABI are
unchanged. The scoped R6 gate is green at 173/173 build steps and 99/99 tests,
with the Rust Alien boundary at 66/66 steps and 9/9 tests; the comparator
admits only `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`.

The broad strict audit remains separate from the default scoped invocation, but
it is now green. The pre-rewrite image fixture runs through the explicit Zig
oracle and Zig Image/Codegen rollback dylibs; stale compiled-unit authority is
skipped for source recovery rather than rejecting the image. The strict command
`BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh` reports `304/304`
verify steps, `516/522` tests, and `6` skipped. This closes the compatibility
follow-up without widening R6's benchmark allowance or changing a frozen
contract.

### 1.1 Remaining Zig categories

| Category | Current files | Desired disposition | Production rule |
| --- | --- | --- | --- |
| Zig host oracle | `src/id/main.zig` | retain as explicit `birnam-zig-oracle` until a separately approved retirement | Rust `bin/birnam` is the only canonical host |
| Zig host helpers | `project.zig`, `project_build.zig`, `project_check.zig`, `runtime_resources.zig` | keep only for the oracle/build while Rust host proves equivalent observation | Rust owns filesystem/process/resource observation; Birnam modules own policy |
| Artifact oracle/build seam | `artifact.zig`, `artifact_inspector.zig`, `artifact_preflight.zig` | split pure codec, dynamic IdApi projection, and host closure; Zig copies remain fixtures until retirement | no second closure/preflight authority |
| Dynamic Zig rungs | `id.zig`, `store.zig`, `peg.zig`, `lang.zig`, `tcc.zig`, `codegen.zig`, `prelude.zig`, `collection.zig`, `error.zig`, `isolate.zig`, `relational.zig`, `alien.zig`, `image.zig` | retain named rollback/oracle fixtures; retire one only after its fixture/recovery decision | never selected by ambient search or canonical default |
| Evidence tools | `*_test.zig`, `gc_baseline.zig`, `membership_bench.zig`, ABI probes, sentinels | retain as evidence or replace with an equivalent Rust/native test | test code is not a runtime provider |
| Native policy | `*.birnam` under `src/id` and `modules` | remain canonical native Birnam; convert only duplicated policy, never the policy surface itself | one message/protocol authority |

`idapi.zig` is a build/Zig-oracle ABI dependency, not a disposable test helper.
`artifact_preflight_sentinel.zig` is an explicit negative fixture. Neither is
silently removed during this plan.

### 1.2 Artifact ownership split

The artifact surface has four non-overlapping roles:

1. `birnam-artifact` is a pure, stateless descriptor codec/parser. It owns no
   IdApi, globals, closure decisions, activation, or `module_init`.
2. `libbirnam_id_artifact.dylib` is the sole `Artifact`/`ArtifactVt` IdApi
   projection and its dynamic `module_init` owner.
3. The Rust host is the sole filesystem sibling-closure, path-containment,
   and preflight-permission gate. `Module loadArtifact:` remains the sole
   runtime dependency-ordering, initializer-delegation, Store/MOP-publication,
   and activation authority. The host never invokes an artifact initializer
   directly. It may call the pure codec and load the dynamic projection, but
   it exports neither `Artifact` nor a competing `module_init`.
4. Native-Birnam `Module loadArtifact:` is the runtime dependency-ordering,
   initializer-delegation, Store/MOP-publication, and activation owner. It
   consumes the host's permission result and is the only path allowed to run a
   validated artifact initializer.

Parity tests use identical inert bytes at all four boundaries and assert that
no layer publishes a second schema, closure, or activation authority; the host
preflight and `Module loadArtifact:` activation boundaries are tested
independently.

## 2. Invariants and design decisions

1. **One floor, one owner.** A process has one IdApi, heap, interner, Store,
   generation registry, and canonical family. An oracle fixture is loaded only
   in a named isolated process or operator-selected rollback prefix.
2. **Dynamic boundary.** Every production rung remains a dynamic library with
   exactly one `module_init(IdApi*, void*)`. Rust crates are build inputs, not
   hidden static runtime authorities.
3. **Frozen ABI/layout.** IdApi, Oop/vtable placement, payload layouts,
   generated-C symbols, image-v4 framing, Isolate wire-v1, and existing
   diagnostic identities do not change in this work.
4. **MOP consistency law.** The object dictionary is authoritative for
   dispatch. Store owns its fact timeline and its MOP mirror is derived. For
   every direct `(vtable, selector)` binding, the mirror is exactly the live
   Method, `#primitive` for an unreified direct binding, or absent. It may never
   name a different method, copy inherited methods into direct slots, or become
   a second Rust method registry. Replacement retracts then mutates then
   reasserts; removal retracts then removes.
5. **Store authority.** Relational reads consume Store facts. No host/helper
   port may create a second global environment or dispatch table.
6. **GC/re-entry.** No Rust borrow, lock, temporary pointer, or `&mut` crosses
   `send`, a finalizer, TCC, `dlopen`, or a callback. Native state is rooted for
   its callback lifetime and process-lifetime handles stay loaded while their
   function/layout pointers remain live. C boundaries abort on invariant
   violations and never unwind a panic.
7. **Fail closed and atomic.** Missing/malformed artifacts, paths, manifests,
   source, images, generations, or dependencies reject before activation. A
   failed compile, map, publication, install, or rollback leaves the prior
   dispatch/Store/path unchanged.
8. **No ambient fallback.** Runtime resolution uses only the installed prefix
   and declared dynamic names. It never searches cwd, checkout source, ambient
   loader paths, stale compatibility names, or an unpinned manifest.
9. **Bootstrap independence.** Production Rust/native closure may be
   bootstrapped from another language. For this plan, “no Zig compiler” means
   no Zig *Birnam compiler implementation or runtime artifact* is required or
   loaded; the Zig toolchain may still orchestrate packaging until a separate
   toolchain-retirement plan exists. A clean archive build must not require a
   prebuilt Birnam image or compiled module.
10. **Native-Birnam policy remains single-owned.** `project-policy.birnam`,
    `stdlib.birnam`, `modules/*.birnam`, and related source modules remain the
    language-level policy/protocol authority. Rust replaces observation and IO
    glue only; it does not shadow policy in native vtables.

## 3. Phase Z0 — freeze the residue map (first implementation phase)

Z0 is documentation/diagnostic only. It must land before moving or deleting a
Zig helper.

### Z0.1 Build the authoritative file graph

1. Enumerate repo-wide `rg --files -g '*.zig'` (including `build.zig`,
   `abi/`, vendor/generated files, probes, and scripts), all
   `modules/*.birnam`, `src/id/*.birnam`, Rust crates, build targets, install
   destinations, and test scripts. Record the current commit and selected
   defaults in a generated, reviewable report. Every vendor/generated/build
   file must receive an explicit classification rather than being dropped.
2. For each Zig file, classify it as canonical production, Zig oracle,
   build-only, test/evidence, or native-policy bridge. A file may have more
   than one use; record each edge rather than collapsing it to “test-only.”
3. Check the report with a checked-in `abi/remaining-zig-v1.json` manifest and
   `scripts/check-remaining-zig-inventory.py`, exposed as
   `zig build test-remaining-zig-inventory`. The checker must fail on an
   unclassified Zig file, missing owner/disposition, or a dynamic dependency
   that is not a declared library name.
4. Trace runtime edges from Rust `bin/birnam`: resource resolution, dynamic
   names, `dlopen` calls, embedded source, Codegen/Image calls, project policy
   messages, and artifact validation. The graph must distinguish build-time
   Zig compilation from runtime loading.
5. Add a manifest scope note that names only dynamic runtime providers. Rust
   crate names, Zig source paths, static TCC objects, and test executables are
   not runtime dependencies.

### Z0.2 Freeze negative and ownership fixtures

1. Add a clean-prefix test containing decoy Zig dylibs and decoy source files;
   run the Rust host and assert the decoys are never opened.
2. Add a Rust-host load trace that requires the canonical Rust dynamic name for
   every production rung and rejects `_zig` fixture paths.
3. Add an oracle-only test that intentionally selects the Zig host/rung prefix
   and records its identity/digest. This proves rollback remains explicit and
   does not make it production.
4. Add a policy-authority fixture: mutate a self-hosted `.birnam` method,
   observe the Store mirror transition, and assert no Rust native vtable is
   installed as a competing policy implementation.
5. Pair each load trace with identity/digest receipts: every canonical
   production dylib must byte-match its `_rust` fixture and must differ from
   its `_zig` fixture. Equality with `_zig` is a hard packaging failure, never
   an accepted identical-fixture decision. Name `test-floor-rust` as the
   existing Debug/ReleaseSafe floor matrix.
6. Run `zig build test-install test-project-check test-lsp-project
   test-rust-codegen-distribution -Doptimize=ReleaseSafe`, the Debug/Release
   Rust floor matrix, and the scoped R6 gate. Record exact counts and the
   strict-audit pre-rewrite image compatibility boundary separately.

### Z0.1 implementation checkpoint (2026-07-27)

Commit `6defcfb` freezes the first diagnostic slice. `abi/remaining-zig-v1.json`
records the captured defaults (`host_impl=rust`, `codegen_impl=rust`,
`ReleaseSafe`), every repo-wide Zig path, the native-Birnam source graph, Rust
crate manifests, build-step names, test scripts, install destinations, and the
14 canonical dynamic library names. `scripts/check-remaining-zig-inventory.py`
fails closed on an unclassified or stale Zig path, missing owner/disposition,
graph drift, unsafe paths, or a dependency that is not a declared
`libbirnam_*.dylib`. `zig build test-remaining-zig-inventory` exposes the same
checker through the build graph and is included in the ordinary test step.

The gate currently reports 88 Zig files, 91 Birnam sources, 18 Rust crate
manifests, 65 test/evidence scripts, 115 build targets, and 14 dynamic
providers. This is an ownership/diagnostic graph only; no provider selection,
runtime loading, or artifact authority changed in Z0.1.

### Z0.2 implementation checkpoint (2026-07-27)

The committed `test-remaining-zig-boundary` fixture builds independent
ReleaseSafe Rust-canonical and Zig-host oracle prefixes. It emits 14 receipts
per prefix and requires every canonical Rust artifact to match its independent
Rust digest and differ from the Zig fixture; the oracle performs the inverse
check. A `DYLD_PRINT_LIBRARIES` trace from a separate decoy cwd proves all 14
canonical names load from the installed prefix and no `_zig`, `_rust`, cwd, or
source-decoy path is opened. The oracle trace records the same canonical load
order without an implicit Rust fixture.

The current build has no `-Dstore` or `-Dalien` selector. The Zig-host oracle
receipt therefore labels those two canonical loads `rust-shared` while proving
their Zig fixtures remain distinct; Z3 must add explicit production/rollback
distribution selection before either can claim a full Zig rollback vector.
The policy fixture extends `modules/project-policy-test.birnam` with a method
edit, dispatch assertion, and non-`#primitive` MOP marker, and the installed
Rust host runs the complete source-backed suite: `346 passed, 0 failed`.

The bounded slice is diagnostic/negative evidence only; it does not move a
helper or alter canonical provider selection.

### Post-Z0.2/Z0.3 R6 checkpoint (2026-07-27)

The current scoped gate was rerun after the boundary fixture, policy-test, and
explicit Codegen-audit changes. It remains green at `173/173` focused build
steps and `99/99` tests;
the sequential live Rust Alien slice is `66/66` steps and `9/9` tests. The
benchmark comparator reports no new regressions or missing cases and admits
only the frozen `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`
crossings. The native-rung harness is now part of this accepted scoped graph;
strict mode is a separate compatibility audit whose pre-rewrite image fixture
now passes through the explicit Zig oracle/rollback boundary.

### Z0 exit gate

The checked-in graph/checker covers every repo-wide Zig file; every residue
edge has an owner and disposition; digest/decoy/negative fixtures fail closed;
the scoped R6 gate is green at `173/173` and `99/99`; the strict audit is green
at `304/304`, `516/522`, and `6` skipped; and the worktree is clean. No helper
has moved yet. Z1 diagnostic/observation work may proceed, and the strict
snapshot is now the required baseline for Z1 authority changes.

### Z1.1 diagnostic checkpoint (2026-07-27)

Commit `6d29519` adds the fail-closed Rust-host helper-boundary audit. It scans
both Rust host source files for forbidden `project.zig`, `project_build.zig`,
`project_check.zig`, and `runtime_resources.zig` references, requires the
explicit Rust resource resolver and `BIRNAM_LIB_DIR`/`BIRNAM_ID_DYLIB` override
markers, and rejects a future path-based `dlopen` coupling. The focused target
passes at `2/2` steps; the existing Z0.2 decoy/relocation load trace remains
the dynamic evidence. The inventory checker now reports 67 test scripts and
117 build targets. No provider selection or runtime authority changed.

Commit `c1e3c9d` adds `test-rust-resource-resolution`, an explicit Z1.1
observation-boundary smoke. It runs the three executable-relative layout and
narrow-override cases in `src/id/runtime_resources.zig` and the corresponding
Rust `birnam-host` tests, including the rule that a `lib_dir` override supplies
the default `libbirnam_id.dylib` path. The target passes, and the inventory
checker now reports 68 test scripts and 118 build targets. This proves the
resolver observations remain aligned without selecting a provider or moving
any Zig helper.

Commit `6b4df47` hardens the Rust project-check smoke so human/JSON check,
build, run, and test all execute from `/tmp` with `BIRNAM_LIB_DIR`,
`BIRNAM_ID_DYLIB`, and `BIRNAM_RUST_WORKSPACE_DIR` unset. The relocated
project flow passes through the executable-relative Rust resolver, preserving
the existing diagnostic and policy boundaries.

Commit `9a52221` applies the same relocation boundary to the LSP smoke: the
Rust host starts from `/tmp` with all runtime resource overrides unset while
serving absolute project URIs. Workspace diagnostics, UTF-16 positions,
versioned changes, navigation, and shutdown still pass, so LSP observation
does not depend on cwd or checkout paths.

Commit `ecbe02d` adds `test-rust-artifact-authority`, a fail-closed static
ownership check. It requires the allocation-free Rust descriptor codec, one
dynamic `ArtifactVt`/`module_init` projection, host preflight before the
`loadArtifact:` send, and native-Birnam `Module loadArtifact:` policy. It
rejects a Rust-host artifact projection or exported `module_init`; the target
passes, with the inventory at 69 test scripts and 119 build targets. No
artifact schema, activation, or provider behavior changed.

Commit `7a8ee06` adds the Rust host preflight negative corpus as
`test-rust-artifact-preflight`. It exercises dependency-first closure,
identity mismatch/missing sibling, and wrong-kind rejection (`3/3` Rust host
tests), complementing the Zig oracle's seven-case preflight fixture. The
negative checks run before any activation path and leave the authority split
unchanged; inventory is now 70 test scripts and 120 build targets.

The combined Z1 observation/preflight run passes at `100/100` build steps for
`test-rust-resource-resolution`, `test-rust-artifact-authority`,
`test-rust-artifact-preflight`, relocated `test-project-check`, and relocated
`test-lsp-project`. This closes the diagnostic portion of Z1.1/Z1.2 without
changing provider selection, schema, activation, or native policy. The
pre-rewrite-image strict prerequisite that originally gated Z1.3 is now
satisfied by `4bcaa5d` and `13819ab`; the remaining Z1.3 work is test/docs-only
parity closure until a separately diagnosed ownership or behavior gap exists.

The parity closure then found and fixed one typed-diagnostic drift. A missing
Rust sibling previously surfaced as `artifactUnavailable`, while the Zig
oracle's preflight contract is `artifactDependencyInvalid`; `26290af` pinned
the red case and `7796f2e` normalized only that recursive dependency error.
This is a separately diagnosed diagnostic-normalization fix, not an authority
change. `96e555c` adds one checked-in inert descriptor-byte corpus consumed by
both Rust and Zig. Its seven common rows cover path-only root, supplied
identity mismatch, wrong kind before walk, sibling identity mismatch, missing
sibling, ordered valid closure, and the real aborting sentinel dylib; both
processes report normalized typed outcomes, and the nested wrong-kind case
preserves `artifactKindMismatch`. The corpus also keeps the existing malformed,
duplicate, path-escape, and native-requirement Rust cases. `5991f8f` records
source/revision digest rejection, absolute install-root rejection, stale
receipt replacement, and the failed-preflight existing-target assertion. The
valid path returns an ordered permission plan and no activation call; the
failed path leaves its target and Store/Module boundary unchanged. `752e948`
adds a frozen-image regression that changes the image/code generation authority
together and proves stale code is not mapped (`mappedCode == false`) until
recorded source replay is requested. No provider, schema, activation, Store,
or MOP authority changed in this closure.

## 4. Phase Z1 — Rust host and artifact closure

Z1 removes Zig helper dependence from the canonical host path while leaving
the Zig oracle build intact.

### Z1.1 Resource and project observation seam

1. Compare Rust `Resources`/startup resolution with `runtime_resources.zig`:
   canonical prefix, lib directory, floor identity/digest, dynamic module
   order, source-free relocation, and explicit rollback overrides.
2. For each `project*.zig` operation, identify the Rust-host equivalent and
   its Birnam policy message boundary. If no Rust equivalent exists, add a
   typed Rust observation function that returns the existing kinded diagnostic
   record; do not copy policy evaluation into Rust.
3. Make Rust host project/check/build/run/install/test/LSP execute from a
   second cwd with `BIRNAM_LIB_DIR` and `BIRNAM_ID_DYLIB` unset. Any source or
   checkout read must be an explicit embedded-source recovery operation.
4. Add a compile-time/runtime assertion that Rust host production modules do
   not import or `dlopen` `project.zig`, `project_build.zig`,
   `project_check.zig`, or `runtime_resources.zig`.
5. Keep the Zig oracle bridge tests and compare their human/JSON diagnostics,
   resolution digest, artifact plan, and failure ordering against Rust.

### Z1.2 Artifact inspector/preflight authority

1. Enumerate the exact descriptor, dependency, identity, digest, and kind
   rules in `artifact.zig`, `artifact_inspector.zig`, and `artifact_preflight.zig`.
   Mark which are schema parsing, which are host closure policy, and which are
   activation checks. Mark host permission checks separately from
   `Module loadArtifact:` dependency ordering, initializer delegation, and
   Store/MOP publication.
2. Enforce the four-role split from §1.2: `birnam-artifact` remains pure,
   `libbirnam_id_artifact.dylib` remains the sole `Artifact`/`ArtifactVt`
   projection, and Rust host remains the sole filesystem closure/preflight
   permission authority. `Module loadArtifact:` remains the sole runtime
   activation/publication authority; the host never calls an initializer
   directly. No crate may silently add a second `module_init` authority.
3. Port negative cases first: wrong kind, identity/digest mismatch, missing
   dependency, incomplete closure, malformed bytes, wrong install root,
   duplicate providers, stale build receipts, and aborting initializer
   unreachable during preflight.
4. Require the same typed failure kind/detail and failure ordering from Rust
   and Zig oracle processes using the checked-in fixture bytes. Run the real
   `artifact_preflight_sentinel.zig` dylib through both inert preflight paths,
   and pair a failed preflight over an existing target with a zero-activation
   assertion and unchanged target/Store/MOP boundary. Keep the sentinel as the
   explicit abort/unreachable fixture.
5. Add Debug/ReleaseSafe tests for decoy cwd, relocated prefixes, stale
   receipts, and a failed preflight over an existing target path.

### Z1.3 First bounded commits and verification

Land these as separate commits, each with focused green tests:

1. `docs: freeze remaining Zig ownership graph` — Z0 report/manifest note and
   no behavior change.
2. `test: prove Rust host ignores Zig helper paths` — decoy/load-trace and
   negative relocation fixtures.
3. `test: prove Rust resource observation boundary` — only typed Rust
   observation assertions, with project/check/install/LSP parity; do not
   rewrite already-landed host behavior without a diagnosed gap.
4. `test: freeze Rust artifact authority split` — schema/closure/activation
   ownership manifest, negative corpus, and no-partial-publication proof. A
   behavior refactor is allowed only if Z1.2 identifies a specific parity or
   ownership failure and it lands as its own diagnosed commit.
5. `docs: record Z1 cross-floor acceptance` — exact counts, digests, and
   remaining oracle edges.

### Z1 precondition and exit gate

The historical precondition for Z1.3 commits 3–4 was repair or disposition of
the pre-rewrite image compatibility fixture. That prerequisite is satisfied by
`4bcaa5d` and `13819ab`; require the following strict gate before and after any
future behavior or ownership change:

```text
BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
```

to pass. The current Z1.3 parity closure has no authority mutation; one
separately diagnosed diagnostic-normalization fix is recorded above. No owner
change is implied until its exit matrix is complete and a separate review
identifies a concrete gap.

From a clean archive and unrelated cwd, Rust host project/check/build/run/
install/test/LSP and artifact preflight pass with no Zig helper loaded. Rust
and Zig oracle negative corpora must agree on the typed matrix; the MOP/policy
fixture remains single-owned; source-free relocation and strict path checks
pass. The Zig oracle still builds explicitly. Z1 remains open until the named
archive acceptance below is exercised from a clean archive/relocated prefix
and the strict gate is rerun after the closure commits.

The required archive acceptance is:

```text
mise exec -- env LC_ALL=C BIRNAM_HOST_IMPL=rust BIRNAM_CODEGEN_IMPL=rust \
  bash scripts/source-bootstrap-smoke.sh
```

The result record must name the archive commit, Rust selections,
source/bootstrap and relocated-prefix results, the shared preflight matrix in
Debug and ReleaseSafe, and the remaining explicit Zig-oracle edges. A green
repo-local `zig build verify` does not substitute for this archive boundary.

### Z1 archive acceptance record (2026-07-27)

At archive commit `d6f91dd`, the command above passed from a clean temporary
archive. Cargo checked and tested the workspace with the pinned Rust 1.95.0
toolchain, the Rust-selected archive built and ran from its relocated prefix,
and the explicit Zig oracle prefix also built and ran. The archive ran the
shared preflight fixture in Debug and ReleaseSafe: 8 Zig rows and 11 Rust
rows passed in each mode, including the real aborting sentinel dylib. No
prebuilt Birnam artifact was present and no source/checkout fallback was used.
The remaining Zig edges are the explicit oracle/rollback prefix and the
noncanonical Zig fixture libraries; no production provider changed.

## 5. Phase Z2 — native-Birnam policy closure

Z2 proves that policy remains in `.birnam` while Rust owns observation. Split
the diagnostic inventory from any later policy movement.

### Z2.0 read-only message/owner inventory

1. Inventory every Rust-host `IdApi.send` call and every named global lookup,
   not only `Module`, `Config`, `Toml`, `Project`, `CompiledUnit`,
   `ProjectPolicy`, and `stdlib`. Include Codegen, System, Alien, Globals,
   Test, Cli, Repl, DispatchGeneration, EvolutionIntent, collection/result
   objects, and any receiver acquired indirectly.
2. For each row record the Rust callsite, receiver acquisition, selector/arity,
   actual owner (native `.birnam`, dynamic rung, or data object), native
   definition file/selector where applicable, effect class (observation,
   validation, IO/FFI, orchestration, policy, or activation/publication),
   pre/post-send Rust branches, retained validation, test, and proposed
   disposition.
3. Add a fail-closed checker so a new Rust send/global lookup cannot land
   without an inventory row. Z2.0 makes no runtime removals or additions;
   duplicate policy discovery becomes a named reviewed seam and a stop
   condition.

### Z2.1 separately reviewed policy seams

1. Remove only Rust branches that independently choose providers, ordering,
   compile/reuse, or publication outcomes already owned by `.birnam`. Retain
   boundary validation of policy results: shape/vocabulary, path containment,
   target/ABI, source and revision digests, checksums, and atomic-publication
   preconditions. If a missing policy operation is discovered, add it to the
   appropriate `.birnam` module and test it through ordinary Codegen/dispatch.
2. Exercise method replacement/removal, Store backfill, inherited lookup,
   failed candidate activation, and rollback while project policy is loaded.
   Assert the exact MOP consistency law after every transition.
3. With the Z1 strict prerequisite now green, run project
   check/build/run/test, docgen, LSP, source fallback, image save/rebuild, and
   isolate smoke from a relocated prefix. Any recurrence of the relocated
   docgen or pre-rewrite image failure is a stop; retain the assertion rather
   than deleting it.
4. Make one narrow commit per policy seam, followed by a documentation
   checkpoint. No dynamic Zig fixture is retired in Z2.

### Z2 exit gate

All shipped policy behavior is reached through native Birnam modules; Rust
contains only observation/IO/FFI glue; MOP/Store traces remain exact; and
Rust-host project workflows and strict verification pass. A named strict
fixture blocker is a stop condition here, not an accepted exception.

## 6. Phase Z3 — production/fixture separation

Z3 makes the distribution boundary honest without deleting rollback evidence.

1. Add an explicit distribution selection to the existing build driver (for
   example `-Ddistribution=production|rollback|test`) so a production bundle
   can omit `_zig` libraries and `birnam-zig-oracle` while test/rollback bundles
   retain them. The Zig toolchain may still orchestrate this build; the
   production process must not load a Zig Birnam implementation or runtime
   artifact. Build two independent **production** prefixes from a git archive
   with Rust selected; build the rollback prefix separately.
2. Verify every production `dlopen` with `DYLD_PRINT_LIBRARIES` (or the
   platform equivalent) and compare against the declared dynamic-only graph.
   Any `_zig` load is a hard failure.
3. Keep `_zig` libraries and `birnam-zig-oracle` only in the separate
   test/rollback prefix with identity/digest receipts. Production bundles
   never contain a rollback-labeled `_zig` library or oracle executable.
   `abi/rollback-prefix-v1.json` must map the complete rollback vector:
   `bin/birnam` from `birnam-zig-oracle`, and each canonical dynamic name
   (`libbirnam_id`, `libbirnam_store`, `libbirnam_id_artifact`,
   `libbirnam_id_peg`, `libbirnam_id_lang`, `libbirnam_id_tcc`,
   `libbirnam_id_codegen`, `libbirnam_id_prelude`,
   `libbirnam_id_collection`, `libbirnam_id_error`,
   `libbirnam_id_isolate`, `libbirnam_id_relational`,
   `libbirnam_id_alien`, and `libbirnam_id_image`) to its checked-in `_zig`
   fixture and digest. The rollback checker rejects any unspecified mixed
   vector; it is full Zig, not an informal “loads Zig” claim.
4. Run old image, generated-C, BISO, and native-client fixtures through the
   explicit oracle and current Rust production paths. A fixture mismatch is a
   release stop, not a contract change.
5. Add a release-bundle test proving the default `bin/birnam` loads Rust and
   that an operator-selected rollback prefix loads Zig, with no ambient path
   fallback.

### Z3 exit gate

All three outputs pass: production archive/prefix A, an independently built
production archive/prefix B, and a separately built full-Zig rollback
archive/prefix. Both production receipts satisfy the Rust closure/load/digest
checks; the rollback receipt satisfies `abi/rollback-prefix-v1.json`. All
three exercise source/bootstrap, project, relocation, image, wire, and
generated-C fixtures. Production bundles have no runtime Zig dependency or
`_zig` artifact; rollback remains recoverable and explicit in its separate
prefix.

## 7. Phase Z4 — rung fixture retirement decisions

This is a sequence of separately reviewed decisions, not an automatic delete.
For one rung at a time:

1. Name the canonical Rust dylib, Zig fixture digest, old-image/client corpus,
   recovery path, and release/rollback owner.
2. Observe one release cycle with Rust canonical and one explicit Zig rollback
   cycle. Run cross-floor, forced-GC, MOP, image/wire/ABI, and hostile-input
   tests in isolated prefixes.
3. If the fixture is no longer required, remove it only in a dedicated commit
   with a reversible archive/tag and update the manifest, install tests, and
   docs. If it is still required, mark it retained and explain why.
4. Never delete `idapi.zig`, old images, generated-C fragments, BISO fixtures,
   or native-client fixtures merely because the Rust implementation is green.

The final goal is not “no `.zig` files”; it is “no unreviewed Zig production
authority.” Zig source may remain as oracle/evidence where compatibility and
recovery require it.

## 8. Acceptance matrix

| Cell | Required evidence |
| --- | --- |
| ownership | checked-in file graph, owner/disposition, dynamic-only manifest |
| host closure | Rust host starts and runs with no Zig helper/runtime load |
| artifact | Rust schema + host preflight parity, negative/atomic corpus |
| policy | `.birnam` remains the only policy authority; MOP law after mutations |
| bootstrap | two independent production archive builds with no Zig Birnam implementation/runtime required; rollback archive tested separately with declared Zig dependencies |
| relocation | second cwd/prefix, decoy paths ignored, exact dynamic load trace |
| compatibility | old image, generated-C, BISO, ABI, and native-client fixtures |
| rollback | explicit Zig prefix/receipt works and cannot be ambiently selected |
| performance | existing R6 comparator; no unrecorded benchmark allowance |
| strict audit | strict R6 passes before and after every Z1–Z3 authority/distribution change |

The scoped gate remains `bash scripts/r6-acceptance.sh`; it includes the
optimized relocated native-rung harness. The strict compatibility audit is
green at the pinned `304/304`, `516/522`, `6`-skipped snapshot, and
Z1/Z2/Z3 authority or distribution changes still require
`BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh` to pass.

## 9. Commit discipline and stop conditions

- Every numbered implementation item is one small commit after focused tests.
- Before Z0 diagnostic-only changes, run `git diff --check`, the relevant
  Debug/ReleaseSafe target, install/relocation smoke, cross-floor corpus, and
  the scoped R6 gate. Before every Z1–Z3 owner/disposition or distribution
  change, run those checks plus
  `BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh`; a strict failure
  is a stop, not a named exception.
- Keep unrelated worktree changes out of commits; preserve explicit rollback
  commits and receipts.
- Stop for human decision only if a required replacement would change a frozen
  ABI/layout/protocol, make policy ambiguous, remove the only recovery path,
  or leave no evidence distinguishing Rust/native production from a Zig
  fixture. Otherwise resolve from the invariants above.

## 10. Estimate and first checkpoint

| Phase | Estimate | Main risk |
| --- | --- | --- |
| Z0 residue map | 1–2 days | hidden build/runtime edge misclassified |
| Z1 host/artifact closure | 3–7 days | diagnostic or preflight drift |
| Z2 policy closure | 2–5 days | accidental policy duplication |
| Z3 prefix/fixture separation | 2–4 days | loader/receipt ambiguity |
| Z4 per-rung retirement decisions | 1–3 days per rung | old-image/client recovery |

The first checkpoint is Z0.1–Z0.2 and must land before any host-helper or
fixture move.

## 11. Adversarial review record

The 2026-07-27 adversarial review initially found artifact-owner ambiguity,
strict-gate contradictions, bootstrap wording that conflated the Zig toolchain
with the Zig Birnam implementation, missing boundary-validation protection,
an incomplete repo-wide residue map, an underspecified rollback vector, and an
acceptance count that did not name both independent production prefixes. The
plan was revised through commits `1c6db24`, `f2782cf`, `d9b4c5f`, and `5305e80`.

The first follow-up returned **GO** on the authority boundaries, but the
latest follow-up remains **NO-GO to close Z1** despite strict R6 passing. It
requires the clean-archive acceptance record, the shared-byte/common-result
matrix, real sentinel coverage, failed-preflight atomic evidence, nested-kind
parity, digest/install-root/stale-receipt cases, and this Z2.0/Z2.1 split.
Those are evidence and planning corrections only; no implementation Z1–Z3
authority change may bypass the strict gate.
