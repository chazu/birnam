# Remaining Zig disposition and native-Birnam migration plan

Status: **reviewed GO for the residue map, authority boundaries, and Z1/Z2.0
closure; Z0 and the Z1/Z2 strict/archive evidence are accepted** (2026-07-27).
Z3 was reopened by the adversarial review recorded below and remains gated on
the hardened strict gate and a follow-up review.

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
its prior baseline is green. The pre-rewrite image fixture is retained as
compatibility evidence: rollback uses the explicit Zig oracle and Zig
Image/Codegen dylibs, while production uses its own Rust host/Image/Codegen
providers. The hardened strict command now runs the distribution-selection and
clean-archive Z3 targets after the optimized verify graph; its final result is
recorded below without widening R6's benchmark allowance or changing a frozen
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
at `305/305`, `516/522`, and `6` skipped; and the worktree is clean. No helper
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
tests), complementing the Zig oracle's eight-case preflight fixture. The
negative checks run before any activation path and leave the authority split
unchanged; inventory is now 72 test scripts and 121 build targets.

The combined Z1 observation/preflight run passes at `100/100` build steps for
`test-rust-resource-resolution`, `test-rust-artifact-authority`,
`test-rust-artifact-preflight`, relocated `test-project-check`, and relocated
`test-lsp-project`. This closes the diagnostic portion of Z1.1/Z1.2 without
changing provider selection, schema, activation, or native policy. The
pre-rewrite-image strict prerequisite that originally gated Z1.3 is now
satisfied by `4bcaa5d` and `13819ab`; the Z1.3 test/docs-only parity closure is
complete, with no ownership or behavior gap identified.

The parity closure then found and fixed one typed-diagnostic drift. A missing
Rust sibling previously surfaced as `artifactUnavailable`, while the Zig
oracle's preflight contract is `artifactDependencyInvalid`; `26290af` pinned
the red case and `7796f2e` normalized only that recursive dependency error.
This is a separately diagnosed diagnostic-normalization fix, not an authority
change. `96e555c` adds one checked-in inert descriptor-byte corpus consumed by
both Rust and Zig. Its eight common rows cover path-only root, supplied
identity mismatch, wrong kind before walk, nested wrong kind, sibling identity
mismatch, missing sibling, ordered valid closure, and the real aborting
sentinel dylib; both processes report normalized typed outcomes, and the
nested wrong-kind case preserves `artifactKindMismatch`. The corpus also keeps
the existing malformed,
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
pass. The Zig oracle still builds explicitly. The named archive acceptance
below passes from a clean archive, including install and normalized parity, and
the strict gate has passed after the closure commits. Z1 is closed by the
archive, strict rerun, and final adversarial review recorded below.

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

At archive commit `67714fe`, the command above passed from a clean temporary
archive. Cargo checked and tested the workspace with the pinned Rust 1.95.0
toolchain, the Rust-selected archive built and ran from its relocated prefix,
and the explicit Zig oracle prefix also built and ran. The archive exercised
the same eight checked-in fixture rows, including the nested wrong-kind
dependency and the real aborting sentinel dylib, in all four required modes:

| runner/profile | accepted count |
| --- | ---: |
| Zig / Debug | 8 shared rows |
| Zig / ReleaseSafe | 8 shared rows |
| Rust / debug | 11 filtered tests (8 shared rows) |
| Rust / release | 11 filtered tests (8 shared rows) |

The relocated Rust project smoke covered check, JSON check, build, run,
install, and test from an unrelated cwd; the relocated LSP smoke also passed.
The archive also diffed the eight normalized shared-corpus rows (outcome,
stable detail, and first-failure token) emitted independently by Rust and Zig.
No prebuilt Birnam artifact was present and no source/checkout fallback was
used. The remaining Zig edges are the explicit oracle/rollback prefix and the
noncanonical Zig fixture libraries; no production provider changed.

### Z1 strict R6 acceptance record (2026-07-27)

After the archive closure, the strict gate was rerun at commit `9075d26`:

```text
mise exec -- env BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
```

It passed the scoped Rust replacement matrix and live Rust Alien boundary, then
the full optimized graph at `305/305` steps, `516/522` tests, and `6` skipped.
The accepted benchmark comparator remained unchanged: exactly
`query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`, with no missing or
unusable cases. The only gate repair was the fail-closed inventory update in
`9075d26`, registering the already-executed
`scripts/artifact-preflight-parity-smoke.sh` in `abi/remaining-zig-v1.json`.

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

### Z2.0.1 source-coverage checkpoint (2026-07-27)

Commit `abe237d` establishes the read-only coverage seam in
`abi/rust-host-message-owner-v1.json` and
`scripts/check-rust-host-message-owner-inventory.py`, exposed as
`zig build test-rust-host-message-owner-inventory`. The checker currently
freezes all 120 direct Rust-host `IdApi.send` callsite lines and all 31 named
`global_named` lookup lines, rejects stale or missing lines, and records the 14
named-global owner classes (native Birnam, Rust dynamic, or Id runtime). It
does not alter provider selection or runtime behavior.

The first semantic expansion in `9a04538` was intentionally rejected by
review: five coarse function groups did not prove the per-callsite owner and
branch semantics. The inventory is now checker-backed per callsite in the
working tree: every row records the resolved selector literal and arity,
concrete receiver expression/acquisition, exact owner and definition
path/selector, effect, pre/post branches, retained validation, test, and
read-only disposition. The checker resolves selector bindings (including direct
intern literals), hashes the resolved selector surface, verifies every
definition path contains the selector, and rejects any stale/missing row.
`DispatchGeneration` is explicitly owned by `src/id/compiled-unit.birnam`, not
`isolate.birnam`. No runtime behavior changed.

The Z2.0 review also records an explicit seam conclusion: after all 120
callsites were inspected, no Z2.1 removal seam is proven. The sends are
receiver acquisition, protocol dispatch, boundary validation, or lifecycle
ordering; none may be removed without a new authority decision. The inventory
therefore keeps runtime unchanged and names the next evidence needed before a
candidate can be nominated.

Z2.0 cannot exit on discovery alone. Its inventory, fail-closed checker, and
strict R6/archive pass must be reviewed together; the review may either select
one exact policy seam for Z2.1 or explicitly conclude that no removal seam is
proven. Until that review checkpoint, the inventory authorizes no removal,
addition, or provider change.

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

### Z3.1 execution record — distribution selector slice (2026-07-27)

Commit `69f9895` adds the explicit `-Ddistribution=production|rollback|test`
selection to the build driver. The default `test` distribution preserves the
existing fixture-rich development graph. `production` requires every
canonical implementation selection to be Rust and omits every `_zig` dylib
and `bin/birnam-zig-oracle`. `rollback` defaults and validates the complete
Zig vector, including Store, Alien, and the canonical Zig host; it installs
that host both as `bin/birnam` and as the explicit `bin/birnam-zig-oracle`.

The focused `test-distribution-selection` smoke builds isolated production
and rollback prefixes, checks all fourteen canonical dynamic names, rejects
Zig artifacts from production, and compares every rollback canonical digest
with its `_zig` fixture. The existing `test-prelude-selection` smoke remains
green under the unchanged default test distribution.

This closes only the selector/content sub-step. The Z3 exit gate remains open:
independent archive prefixes, `DYLD_PRINT_LIBRARIES` load receipts,
`abi/rollback-prefix-v1.json`, old-client/image/generated-C fixtures, and the
release-bundle no-ambient-fallback test are still required.

### Z3.2 execution record — rollback manifest/checker (2026-07-27)

Commit `e751b2d` adds `abi/rollback-prefix-v1.json` with the fourteen-name
dynamic load order, the canonical Zig host/oracle mapping, fixture identities,
and the digest/mixed-vector policy. `scripts/check-rollback-prefix.py` fails
closed on missing or undeclared `_zig` fixtures, missing canonical names,
unsafe paths, host mismatch, or any canonical digest that differs from its
declared Zig fixture. `test-distribution-selection` now emits the checker
receipt as part of its isolated rollback-prefix build.

This is a receipt contract, not the full release proof: the production
load-trace, two independent archive prefixes, and old compatibility fixture
legs remain open under Z3.

### Z3.3 execution record — canonical loader traces (2026-07-27)

Commit `8cfc9f2` extends `test-distribution-selection` with isolated
`DYLD_PRINT_LIBRARIES=1` runs from decoy working directories. The production
trace contains every declared canonical dynamic name and no `_zig` dylib or
Zig oracle executable. The rollback trace contains the same canonical names
and no `_rust` fixture. This proves the selector is exercised by the loader,
not merely by prefix directory enumeration.

The trace is still a local-prefix receipt. Z3 remains open for two independent
clean archive production prefixes, persisted load/digest receipts, and the
full compatibility/release fixture matrix.

### Z3.4 execution record — independent archive prefixes (2026-07-27)

Commits `2a73745`, `4b98975`, `0ef5cfc`, and `99a1a45` add
`test-z3-archive-distribution`. The smoke creates two separate `git archive
HEAD` checkouts with independent Zig and Cargo caches, removes only the
checked-in compatibility fixture that is explicitly excluded from bootstrap,
and builds both as `-Ddistribution=production`. It then creates a third clean
archive rollback prefix, runs `check-rollback-prefix.py`, and traces all three
from decoy working directories.

The accepted result is:

```text
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

This closes the independent-prefix/load-receipt sub-step. The Z3 exit gate
still requires the source/bootstrap, project, relocation, image, wire,
generated-C, old-client, and release-bundle fixture legs on these prefixes.

### Z3.5 execution record — archive fixture matrix (2026-07-27)

Commit `d2045de` extends the archive smoke across all three prefixes. Each
clean production/rollback archive runs source evaluation, Rust-host or
Zig-host project check/build/run/install/test, LSP, image save, Isolate wire
fixtures, and a separate-process Rust-TCC generated-C artifact activation.
The accepted receipt is:

```text
z3-archive: production-b source/project/lsp/image/wire/generated-C fixtures=ok
z3-archive: rollback source/project/lsp/image/wire/generated-C fixtures=ok
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

The remaining Z3 fixture work is specifically the checked-in old image/native
client/generated-C compatibility corpus and the release-bundle/no-ambient-
fallback assertion; the ordinary generated-C and runtime wire legs are now
covered on all three prefixes.

### Z3.6 execution record — pre-rewrite image/native-client corpus (2026-07-27)

Commit `9eca519` adds the checked-in `abi/fixtures/idapi-v1` verifier to the
three-prefix smoke. The rollback run uses the explicit Zig oracle host and Zig
Image/Codegen providers; each production run uses its own Rust host, Rust Image,
and Rust Codegen providers. The frozen native client, mapped image, and
source-rebuild image paths all pass for both Rust production prefixes and the
full-Zig rollback prefix; source recovery remains an explicit diagnostic, not
an ambient fallback.

The final Z3 release-bundle leg is still open: package a production bundle
without the oracle, package the operator-selected rollback bundle, and prove
both from unrelated cwd/loader environments with no ambient path fallback.

### Z3.7 execution record — release bundles and relocated production smoke (2026-07-27)

Commits `6493b2d` and `0328707` close the release-bundle leg. `scripts/dist.sh`
now selects `-Ddistribution=production`, rejects oracle and `_zig` artifacts
in the staged output, ad-hoc signs the production-only bundle, and runs the
relocated `BIRNAM_DISTRIBUTION=production` install smoke from an unrelated
working directory. The production image loader no longer assumes that the
optional Zig Image compatibility dylib exists; when it is absent, Rust Image
continues through its source-recovery path. Explicit Zig retry remains
available when a rollback fixture is actually installed.

The real release command passed:

```text
install-smoke: relocated compiled closure, tools, projects, images, isolates, and diagnostics passed
==> done: zig-out/dist/birnam-0.11.0-aarch64-macos.tar.gz
```

The archive smoke also passes the production and rollback release-bundle
checks, including `DYLD_PRINT_LIBRARIES` traces and no ambient-path fallback.
The Z3 exit gate is now reduced to the post-change strict R6 acceptance run.

### Z3.8 execution record — post-change strict R6 acceptance (2026-07-27)

Commit `a3cb4c8` makes the relocated install smoke pipe-safe: command output is
captured before matching, so Rust cannot receive a false `SIGABRT` from a
consumer that exits after the first matching line. Commit `bc70701` refreshes
the Z0.1 and Z2.0 drift receipts after the production image-loader change and
keys the two exceptional owner-inventory cases by function/selector rather
than source line.

The final strict command passed:

```text
mise exec -- env BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
Build Summary: 305/305 steps succeeded; 516/522 tests passed; 6 skipped
r6-acceptance: PASS
  accepted only for this snapshot: query.aggregate +5.8%, compile.stage1-stdlib +11.9%
```

This closes the Z3 exit gate. Production and rollback bundles remain separate,
all three clean archive prefixes have full fixture receipts, and the strict
R6 comparator still admits only the two pre-recorded tradeoffs.

That closure record is historical. The hardened strict gate and a follow-up
adversarial review supersede it below.

### Z3.9 adversarial review — reopened gate (2026-07-27)

The next adversarial pass returned **NO-GO** for Z3 closure. It found five
concrete evidence gaps: the production old-image leg still used rollback
providers; rollback generated-C used the Rust TCC fixture; production receipts
were not retained with observed identities, SHA-256 digests, and resolved
loader paths; no-ambient checks did not reject every loaded `libbirnam*.dylib`
outside the selected prefix/bundle; and strict R6 did not execute the Z3
distribution targets. Z3 was reopened; no invariant or benchmark allowance
was weakened.

### Z3.10 execution record — hardened receipts, confinement, and compatibility (2026-07-27)

Commit `08f4fa5` hardens the distribution smoke and strict gate. Production
old-image/native-client runs now select each production prefix's Rust
host/Image/Codegen trio; rollback selects the complete Zig trio and canonical
Zig TCC. Production receipts cover the host and all fourteen canonical dynamic
paths with resolved-prefix checks, SHA-256 digests, and digest-derived observed
identities; rollback receipts retain the manifest identity plus the same
observed digest/path evidence. Decoy floor/stdlib files and cleared loader
override variables are exercised, and every traced Birnam dylib is required to
resolve under the selected prefix or release bundle.

Commits `d54e2b9` and `cfbb36d` close the old-image Rust path without weakening
malformed-image rejection: a structurally valid stale generation is admitted
to an explicit source-recovery transaction, where Rust Codegen replays facts
through a recovery-only selector; ordinary preflight and replay continue to
reject stale authorities. Focused Rust Image/Codegen tests pass, and the full
archive matrix now reports:

```text
z3-archive: production-a pre-rewrite image/native-client fixtures=ok
z3-archive: production-b pre-rewrite image/native-client fixtures=ok
z3-archive: rollback pre-rewrite image/native-client fixtures=ok
z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok
```

The strict gate is still pending this hardened execution record and its
follow-up review.

### Z3.11 execution record — hardened strict R6 acceptance (2026-07-27)

The first post-hardening strict run reached the existing verify graph but
exposed a real artifact-install ordering race: the selected Zig artifact
checker could start before its explicit `_zig` receipt path was copied. Commit
`9472c96` makes that fixture install a direct prerequisite of the selected
artifact step; the targeted `native-rung-harness.sh --rung=artifact-inspector=zig`
then passed.

The clean rerun passed:

```text
mise exec -- env BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
Build Summary: 305/305 steps succeeded; 516/522 tests passed; 6 skipped
distribution-selection: production omits Zig; rollback is a complete Zig vector
test-z3-archive-distribution success
r6-acceptance: PASS
  accepted only for this snapshot: query.aggregate +5.8%, compile.stage1-stdlib +11.9%
```

The hardened Z3 execution gate is now green: the strict invocation itself
executes the distribution-selection and clean-archive targets after optimized
verify, and the archive receipt includes both independent Rust production
prefixes plus the full-Zig rollback prefix. Z3 closure still awaits the
follow-up adversarial review below.

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
green at the pinned `305/305`, `516/522`, `6`-skipped snapshot, and
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

The first follow-up returned **GO** on the authority boundaries. A subsequent
adversarial pass was temporarily **NO-GO to close Z1** pending the clean
archive, normalized eight-row parity, real sentinel, failed-preflight atomic,
nested-kind, digest/install-root/stale-receipt, and Z2.0/Z2.1 evidence. Those
gaps are now closed by archive commit `67714fe` and strict-gate fix `9075d26`.
The final 2026-07-27 review is **GO to close Z1**: no remaining invariant,
authority, artifact-owner, MOP/Store, rollback, archive, parity, or strict-gate
gap was found. Z2.0's read-only message/owner inventory is the next seam; its
existing review gate remains unchanged, and no implementation Z1–Z3 authority
change may bypass strict R6.

The next 2026-07-27 adversarial pass was **NO-GO to exit Z2.0** until the
inventory was per-callsite and checker-backed, selector bindings resolved to
literals (or explicit dynamic markers), native definitions were verified, the
`DispatchGeneration` owner was corrected to `compiled-unit.birnam`, and the
review recorded either one exact Z2.1 seam or an explicit no-seam conclusion.
The bounded read-only hardening landed in `f3f109a` (source-derived callsite
behavior, canonical Rust owners, and separate Zig oracle references),
`c008d87` (brace-bounded post-use windows, effect/handling consistency, and
checker-backed focused/integration/uncovered test evidence), and `6e964c9`
(honest host-start/Store-MOP coverage plus the explicitly uncovered
EvolutionIntent `fulfill` arm). No runtime or MOP invariant changed.

The final 2026-07-27 adversarial review is **GO to close Z2.0**. The manifest
contains 120 send callsites across 56 reviewed functions, 31 named global
lookups, canonical Rust/native owners, resolved selector literals, verified
definitions, source-observed validation, and `z2_1_seam.status=none`. The
strict R6 gate passed at `6e964c9` with the pinned `305/305` verify steps,
`516/522` tests, six skips, live Alien green, `missing=0`, `unusable=0`, and
only the accepted snapshot regressions `query.aggregate +5.8%` and
`compile.stage1-stdlib +11.9%`. The archive/source-bootstrap smoke also
passed on its identical retry, covering Rust production, explicit Zig
rollback, artifact parity, and relocated project/LSP behavior. A single
first archive attempt hit a transient Alien-loader SIGABRT; the identical
rerun passed and no Alien/runtime change is attributable to this read-only
inventory.

Z2.1 has no removal seam in this inventory. Do not remove, retarget, or
reinterpret a runtime/provider owner based on Z2.0 alone; any future seam
requires its own invariant review, explicit owner/disposition change, and a
fresh strict R6 gate.
