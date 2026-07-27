# Remaining Zig disposition and native-Birnam migration plan

Status: **reviewed GO; Z0 implementation may begin** (2026-07-27). Z1–Z3
authority/distribution changes remain gated on strict R6 as specified below.

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
unchanged. The scoped R6 gate is green at 171/171 build steps and 99/99 tests,
with the Rust Alien boundary at 66/66 steps and 9/9 tests; the comparator
admits only `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%`.

The broad strict audit remains separate: on 2026-07-27 it failed in the
all-migrated `test-native-rung-harness` install fixture because relocated
docgen still lacks `tag:body:` registration. That failure is evidence to
resolve, not permission to widen R6 or weaken a contract.

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
   strict-audit `tag:body:` boundary separately.

### Z0 exit gate

The checked-in graph/checker covers every repo-wide Zig file; every residue
edge has an owner and disposition; digest/decoy/negative fixtures fail closed;
the scoped R6 gate is unchanged and green; and the worktree is clean. No
helper has moved yet. The known strict `tag:body:` failure may be recorded at
Z0, but Z1 authority changes are blocked until strict mode is green.

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
   duplicate providers, and aborting initializer unreachable during preflight.
4. Require the same typed failure kind/detail and atomic no-publication result
   from Rust and Zig oracle processes. Keep `artifact_preflight_sentinel.zig`
   as the explicit abort/unreachable fixture.
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

Before Z1.3 commits 3–4 (the resource/artifact authority changes), repair the
relocated docgen fixture's `tag:body:` registration and require:

```text
BIRNAM_R6_REQUIRE_VERIFY=1 bash scripts/r6-acceptance.sh
```

to pass. The scoped gate alone is sufficient for Z0 diagnostics, not for a
host/artifact authority change.

From a clean archive and unrelated cwd, Rust host project/check/build/run/
install/test/LSP and artifact preflight pass with no Zig helper loaded. Rust
and Zig oracle negative corpora agree; the MOP/policy fixture remains
single-owned; source-free relocation and strict path checks pass. The Zig
oracle still builds explicitly.

## 5. Phase Z2 — native-Birnam policy closure

Z2 proves that policy remains in `.birnam` while Rust owns observation.

1. Inventory every message sent by Rust host to `Module`, `Config`, `Toml`,
   `Project`, `CompiledUnit`, `ProjectPolicy`, and `stdlib`. For each message,
   name its native-Birnam definition and its Rust observation adapter.
2. Remove only Rust branches that independently choose providers, ordering,
   compile/reuse, or publication outcomes already owned by `.birnam`. Retain
   boundary validation of policy results: shape/vocabulary, path containment,
   target/ABI, source and revision digests, checksums, and atomic-publication
   preconditions. If a missing policy operation is discovered, add it to the
   appropriate `.birnam` module and test it through ordinary Codegen/dispatch.
3. Exercise method replacement/removal, Store backfill, inherited lookup,
   failed candidate activation, and rollback while project policy is loaded.
   Assert the exact MOP consistency law after every transition.
4. After the Z1 strict prerequisite is repaired, run project
   check/build/run/test, docgen, LSP, source fallback, image save/rebuild, and
   isolate smoke from a relocated prefix. Any recurrence of the `tag:body:`
   failure is a stop; retain the assertion rather than deleting it.
5. Make one narrow commit per policy seam, followed by a documentation
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

The scoped gate remains `bash scripts/r6-acceptance.sh`; it is not widened to
hide the current `tag:body:` strict fixture failure. Z0 may report that
failure separately, but Z1/Z2/Z3 authority or distribution changes require
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

The final follow-up returned **GO**: the four artifact roles, strict R6
preconditions, production/rollback separation, full Zig rollback manifest,
native-Birnam activation ownership, digest checks, bootstrap meaning, and
dual-production reproducibility evidence are explicit and landable. No
implementation Z1–Z3 authority change may bypass the strict gate.
