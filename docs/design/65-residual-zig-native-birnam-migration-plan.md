# Residual Zig owners → Rust/native Birnam migration plan

Status: design plan, not yet executed. Prepared after the Rust floor, all
selected rungs, Rust Image, and Rust host passed the R6 acceptance gate on
2026-07-26.

## 1. Outcome and scope

The remaining production Zig authority is the Codegen dynamic rung:
`src/id/codegen.zig` → `libbirnam_id_codegen.dylib`. Its Rust successor exists
as an opt-in `libbirnam_id_codegen_rust.dylib` (`birnam-codegen-runtime`), but
it is not yet the canonical `Codegen` owner. This plan completes that port,
proves it against the existing Zig authority, switches the canonical dynamic
name, and keeps the Zig implementation as an explicit rollback fixture.

The Rust host is already the canonical `bin/birnam`. The remaining Zig host and
its helper sources are retained as the explicit `birnam-zig-oracle` and as
compatibility/test evidence. Host policy that is already expressed in
`*.birnam` stays native Birnam; host-side filesystem, process, and FFI glue
stays Rust. No second policy authority is introduced.

This plan does not rewrite the language, change the object model, redesign the
image format, or remove frozen fixtures. It only moves an implementation behind
the existing contracts and then makes the ownership metadata honest.

## 2. Current owner audit (2026-07-26)

| Surface | Current canonical owner | Explicit Zig artifact | Planned outcome |
| --- | --- | --- | --- |
| Id floor | Rust `libbirnam_id.dylib` | `libbirnam_id_zig.dylib` | Retain both; no floor rewrite in this plan |
| Store | Rust `libbirnam_store.dylib` | `libbirnam_store_zig.dylib` | Retain Zig oracle; no duplicate Store authority |
| PEG | Rust `libbirnam_id_peg.dylib` by default | selection path must produce an explicit Zig fixture | Harden install metadata/rollback path before Codegen switch |
| TCC | Rust `libbirnam_id_tcc.dylib` | `libbirnam_id_tcc_zig.dylib` | Retain both; generated-C ABI remains frozen |
| Lang | Rust `libbirnam_id_lang.dylib` | `libbirnam_id_lang_zig.dylib` | Retain both |
| Prelude | Rust `libbirnam_id_prelude.dylib` | `libbirnam_id_prelude_zig.dylib` | Retain both |
| Collection | Rust `libbirnam_id_collection.dylib` | `libbirnam_id_collection_zig.dylib` | Retain both |
| Error | Rust `libbirnam_id_error.dylib` | `libbirnam_id_error_zig.dylib` | Retain both |
| Isolate | Rust `libbirnam_id_isolate.dylib` | `libbirnam_id_isolate_zig.dylib` | Retain both; wire-v1 remains frozen |
| Relational | Rust `libbirnam_id_relational.dylib` | `libbirnam_id_relational_zig.dylib` | Retain both; Store remains derived |
| Alien | Rust `libbirnam_id_alien.dylib` | `libbirnam_id_alien_zig.dylib` | Retain both; TCC and callback ABI remain frozen |
| Image | Rust `libbirnam_id_image.dylib` | `libbirnam_id_image_zig.dylib` | Retain both; frozen images stay compatibility evidence |
| Codegen | Zig `libbirnam_id_codegen.dylib` | no separate Zig rollback name yet | Port Rust, then switch canonical name; install Zig as explicit rollback |
| Host | Rust `bin/birnam` | `bin/birnam-zig-oracle` | Keep oracle for rollback/frozen images |
| Project policy | `project-policy.birnam` and other Birnam modules | Zig oracle bridges in `project*.zig` | Keep policy in Birnam; Rust host owns observation/IO glue |
| Test/ABI helpers | Zig tests and probes under `src/id` | checked-in fixtures | Retain; they are evidence, not runtime authorities |

The source inventory in `docs/design/64-remaining-module-contract-inventory.md`
is the contract reference, but its historical status fields must be refreshed
by Phase C0 below. The runtime may select only installed dynamic libraries by
their declared canonical or explicit fixture names.

## 3. Target ownership and dependency graph

The target graph is:

```text
Rust IdApi floor
  ├─ Rust Store ───────────────┐
  ├─ Rust PEG → Rust Lang      │
  ├─ Rust TCC                  │
  └─ Rust Error/Collection     │
                                ▼
                         Rust Codegen (canonical)
                                │
             ┌──────────────────┼──────────────────┐
             ▼                  ▼                  ▼
       Rust Image          Rust Alien          Rust host
       (already canonical) (already canonical) (already canonical)
```

Every edge is a runtime `dlopen`/`dlsym` dependency with one `module_init`
call. The Rust Codegen crate may use pure Rust helpers internally, but its
production surface is still one dynamic `module_init` owner named
`libbirnam_id_codegen.dylib`. It must not statically link a second floor,
Store, PEG, TCC, Image, or host.

## 4. Invariants and non-negotiable design decisions

1. **One floor, one owner.** A process has one IdApi, heap, interner, Store
   authority, Codegen generation registry, and canonical family for each
   selector. Explicit rollback fixtures are loaded only by named tests or an
   operator-selected rollback prefix.
2. **Dynamic boundary.** Every production rung is a dynamic library with one
   `module_init(IdApi*, void*)` entry. The Codegen port cannot become a Rust
   static library hidden behind the Zig loader, and no C/Rust static link may
   smuggle in a second authority.
3. **Frozen ABI/layout.** `IdApi`, `Oop`, vtable placement, payload layouts,
   exported names, generated-C helper symbols, image-v4 framing, and
   Isolate wire-v1 are unchanged. Any necessary ABI change is a separate
   manifest/fixture decision, not a Codegen implementation detail.
4. **Toolchain bootstrap is not a runtime invariant.** Rust Codegen may be
   bootstrapped from the existing Zig toolchain or Birnam source. It need not
   compile itself before it can be selected, provided the installed artifact
   is fully self-contained and the source/bootstrap gates remain green.
5. **MOP consistency law.** The object dictionary is authoritative. Store is a
   derived mirror for facts/observation; Codegen's observer and authorizer must
   never create a second method truth. Exact, `#primitive`, and absent mirror
   states, ordering, inheritance, backfill, drift refusal, and rollback all
   remain observable.
6. **GC and re-entry.** No Rust borrow, mutex guard, temporary pointer, or
   `&mut` runtime reference crosses `send`, a finalizer, TCC, `dlopen`, or a
   callback. Callback state is rooted for the synchronous dynamic extent;
   process-lifetime handles are not unloaded while function/layout pointers
   remain live. Invariant violations abort at the C boundary.
7. **Fail closed and atomic.** Malformed source, candidate artifacts, image
   records, generated code, missing dependencies, and generation drift reject
   before activation. No failed compile, map, publication, or rollback may
   partially mutate dispatch, Store, lifecycle, or the target path.
8. **No ambient fallback.** Installed runtime resolution never searches cwd,
   checkout source, unpinned manifests, ambient loader paths, or a stale
   compatibility name. Source recovery is explicit and recorded by the
   existing Codegen/Image protocol.
9. **Dynamic-library-only runtime specification.** Production dependency and
   install tables name dynamic libraries and their `module_init` identities
   only. Rust crates, Zig sources, static TCC objects, and test executables are
   build inputs or evidence, not runtime providers.
10. **Native-Birnam policy remains single-owned.** Existing `*.birnam` policy
    modules remain the language-level authority. Porting a host helper to Rust
    must not recreate that policy in a native vtable or silently change the
    message protocol.

## 5. Phase C0 — reconcile owners and rollback metadata (first commit)

This is a metadata/selection slice; it must not change runtime behavior.

1. Update `abi/remaining-rungs-v1.json` and Design 64 status fields to the
   audited state above. Add a Codegen row that distinguishes canonical Zig,
   Rust opt-in, dependencies, selectors, and the explicit Zig rollback name.
2. Add `-Dcodegen-impl=zig|rust` to `build.zig`. Always install
   `libbirnam_id_codegen_zig.dylib`, `libbirnam_id_codegen_rust.dylib`, and
   exactly one selected `libbirnam_id_codegen.dylib`.
3. Make the PEG selection install its explicit Zig fixture whenever Rust is
   canonical, so rollback paths are symmetric and do not depend on a checkout
   rebuild. Keep all other existing fixture names unchanged.
4. Extend `test-remaining-rung-manifest`, `test-native-rung-selection`, and the
   clean-prefix smoke to assert canonical/fixture identity, digest, install
   root, and the absence of static/runtime fallback paths.
5. Record the selected owner and dylib digest in the plan. Commit this phase as
   `build: make residual owner and rollback metadata explicit`.

Required verification before commit:

```text
zig fmt --check build.zig
bash -n scripts/native-rung-selection.sh scripts/r6-acceptance.sh
zig build test-remaining-rung-manifest test-native-rung-selection \
  test-remaining-rung-clean-prefix -Dhost-impl=rust
```

## 6. Phase C1 — freeze the Codegen replacement contract

This phase adds tests and Rust-side typed boundaries without selecting Rust.
Each numbered item is a separate commit.

### C1.1 Selector/export/layout manifest

Extract the live `CodegenVt`, `Codegen` family, `lastError`, lifecycle phase,
generation, candidate, source, and image-recovery selectors from
`src/id/codegen.zig`. Assert exact names, arities, nil/error behavior, payload
shape, export identity, and `module_init` idempotence through both Zig and Rust
fixtures. Include negative probes for null args, duplicate generations,
missing Store/PEG/TCC, and method-authority drift.

### C1.2 Pure emitter parity

Complete `rust/birnam-codegen`'s source-preserving lowering for every currently
accepted Codegen expression, block, send, literal, embedded file, collection,
logic-variable, and diagnostic case. Compare generated bytes and normalized
diagnostics against the Zig oracle; preserve spans and lexical bytes instead of
reformatting source through Rust numeric/string formatters. Stress the existing
bounded depth and source-size limits.

### C1.3 Runtime IdApi adapter

Move the full RustCodegen runtime surface from the opt-in emitter to the frozen
`Codegen` family name. Implement `compile:`, `compile:output:`,
`compile:output:from:`, `compile:output:root:`, `eval*`, `load:`, metadata/source
recording, and `lastError` through the existing PEG/Lang/TCC dynamic modules.
Keep all temporary Oops rooted and release every Rust lock before an IdApi send.

### C1.4 Candidate and lifecycle parity

Port inspect/map/prepare/commit/discard/reconcile, phase transitions,
generation projections, observer/authorizer callbacks, and rollback. Prove
atomic activation and exact Store-derived MOP facts with the existing Zig Store
and Rust Store fixtures. The Rust implementation must refuse inherited drift
and preserve the predecessor on rejected rollback.

### C1.5 Generated artifact/TCC boundary

Run generated-C compilation through the dynamically selected TCC rung. Audit
install names, exported `birnam_*` helpers, callback signatures, disposable
handles, and hostile C inputs. Compare artifacts with the Zig Codegen oracle by
identity and behavior, not by private Rust representation.

## 7. Phase C2 — image and host integration

1. Exercise Rust Codegen as the Image authority for v4 preflight, map, replay,
   source recovery, generation derivation, and forced rebuild. Run all four
   save/map/rebuild cross-products across Zig/Rust Image and Zig/Rust Codegen,
   plus the frozen pre-rewrite image through the explicit Zig oracle.
2. Run the Rust Image canonical path with Rust Codegen and with every retained
   Zig rollback reader. The Image transaction must restore Rust Codegen methods
   after a temporary Zig reader load and must leave no partial publication.
3. Switch the Rust host's canonical Codegen load only through the selected
   dynamic name. Project/check/build/run/install/test/LSP and source-free
   relocation must pass without importing `src/id/codegen.zig` directly.
4. Add a Rust-host smoke that proves no host helper silently loads the Zig
   Codegen fixture. Keep the explicit Zig oracle path for frozen compatibility
   and operator rollback.

## 8. Phase C3 — canonical switch and observation window

1. Change the default `-Dcodegen-impl` to Rust in one focused commit; retain
   `-Dcodegen-impl=zig` as the documented rollback selection.
2. Run focused Codegen, Store/MOP, Image, TCC, Alien, project, LSP, and
   relocation suites in Debug and ReleaseSafe. Run `BIRNAM_R6_ISOLATE_IMPL=rust
   mise run r6-acceptance` and record exact counts and comparator output.
3. Run a second prefix/cwd and a packaged distribution smoke. Verify the
   canonical Rust Codegen digest and the explicit Zig rollback digest in every
   receipt. No benchmark allowance is admitted without a dated, named
   comparator decision.
4. Observe one clean release/rollback cycle before changing distribution
   defaults. Do not delete Zig source or fixtures in this phase.

## 9. Phase C4 — host/helper disposition

After C3 is stable, classify every remaining Zig file by runtime role:

- `src/id/main.zig`: retained as the explicit Zig oracle; no production
  canonical path may depend on it after the Rust host gate.
- `project.zig`, `project_build.zig`, `project_check.zig`, and
  `runtime_resources.zig`: Rust host owns filesystem/process/resource
  observation; Birnam `project-policy.birnam`, TOML, Module, and Config remain
  the language-level policy surface. Remove a Zig helper only after a clean
  archive proves the Rust host does not load it.
- `artifact_preflight*.zig`, `gc_baseline.zig`, `membership_bench.zig`, ABI
  probes, and `*_test.zig`: retain as test/evidence tools unless a replacement
  test proves identical failure behavior. They are not runtime migration
  targets.
- `id.zig`, `idapi.zig`, and the Zig rung sources: retain as the rollback/oracle
  implementation and contract comparator until a separately approved release
  removes them. Do not weaken the old-binary, image, wire, or generated-C
  fixtures.

Any file proposed for deletion requires a separate decision record naming its
replacement, fixture coverage, archive evidence, and recovery path.

## 10. Acceptance matrix

The Codegen switch is accepted only when all cells below pass in isolated
processes and prefixes:

| Cell | Required evidence |
| --- | --- |
| ABI | frozen IdApi manifest, exports, layouts, `module_init` once |
| emitter | byte/diagnostic parity and bounded hostile corpus |
| MOP | dictionary authority, derived Store mirror, observer/authorizer, drift refusal |
| lifecycle | candidate inspect/map/activate/rollback with no partial mutation |
| TCC/Alien | generated C, callbacks, disposable handles, child-abort invariants |
| Image | v4 2×2 save/map/rebuild, corruption rejection, frozen image |
| host | project/check/build/run/install/test/LSP, source-free relocation |
| distribution | packaged Rust canonical and explicit Zig rollback oracle |
| performance | existing comparator; no unrecorded allowances |

The final gate remains `zig build verify -Doptimize=ReleaseSafe --summary all`
plus the serialized R6 script. Any failure is a design stop, not permission to
change an invariant or silently route through Zig.

## 11. Explicit non-goals and retained evidence

- No stack model, continuation model, GC algorithm, MOP law, Store authority,
  IdApi layout, generated-C ABI, image-v4 format, or BISO wire-v1 redesign.
- No static-link replacement for a dynamic rung.
- No deletion of Zig rollback libraries, the Zig oracle, committed native
  client/image fixtures, ABI manifests, or adversarial tests.
- No conversion of already-native `*.birnam` modules into Rust.
- No claim that a Rust crate's existence makes its dynamic rung canonical; the
  selected installed library and its receipt are the authority.

## 12. Commit discipline and stop conditions

Every numbered implementation item lands as one small commit after its focused
tests pass. Before each ownership switch, inspect the install tree, run the
cross-floor and forced-GC tests, run optimized `verify`, run serialized R6, and
run from a second prefix/cwd. Stop for human decision only if the invariant
evidence cannot distinguish a safe implementation from a change to a frozen
contract; otherwise resolve by the rules above.

