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
| Relational | Rust `libbirnam_id_relational.dylib` | `libbirnam_id_relational_zig.dylib` | Retain both; Store owns facts, MOP mirror is derived |
| Alien | Rust `libbirnam_id_alien.dylib` | `libbirnam_id_alien_zig.dylib` | Retain both; TCC and callback ABI remain frozen |
| Image | Rust `libbirnam_id_image.dylib` | `libbirnam_id_image_zig.dylib` | Retain both; frozen images stay compatibility evidence |
| Codegen | Zig `libbirnam_id_codegen.dylib` | no separate Zig rollback name yet | Port Rust, then switch canonical name; install Zig as explicit rollback |
| Host | Rust `bin/birnam` | `bin/birnam-zig-oracle` | Keep oracle for rollback/frozen images |
| Project policy | `project-policy.birnam` and other Birnam modules | Zig oracle bridges in `project*.zig` | Keep policy in Birnam; Rust host owns observation/IO glue |
| Test/ABI helpers | Zig tests and probes under `src/id` | checked-in fixtures | Retain; they are evidence, not runtime authorities |

The source inventory in `docs/design/64-remaining-module-contract-inventory.md`
is the contract reference, but its historical execution record is immutable.
Phase C0 appends a dated current-owner projection and updates only explicitly
current fields. `abi/remaining-rungs-v1.json` is a residual-rung diagnostic
manifest, not a complete runtime dependency graph; adding Codegen requires a
schema/scope note and a Codegen row before Prelude in `load_order`. The runtime
may select only installed dynamic libraries by their declared canonical or
explicit fixture names.

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

Each rung edge is satisfied by the ordered dynamic loader and one
`module_init` call; once loaded, rungs communicate through imported IdApi
objects and messages. Image, Alien, and the host consume Codegen services but
do not become libraries that Codegen loads. The Rust Codegen crate may use
pure Rust helpers internally, but its production surface is still one dynamic
`module_init` owner named `libbirnam_id_codegen.dylib`. It must not statically
link a second floor, Store, PEG, TCC, Image, or host.

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
4. **Toolchain bootstrap is not a self-hosting requirement, but cold
   independence is.** Rust Codegen need not compile itself before it can be
   selected, and the implementation may use a compiler/bootstrap language.
   Before canonical selection, however, two independent clean builds must
   compile the complete Birnam source closure from Rust PEG/Lang/TCC/Codegen
   sources without a Zig compiler implementation or prebuilt Birnam artifact.
5. **MOP consistency law.** The object dictionary is authoritative for method
   lookup. Store owns its fact timeline, while only the Store's MOP method
   mirror is derived from that dictionary; relational queries consume Store
   facts and do not become a second method authority. Codegen's observer and
   authorizer must never create a second method truth. Exact, `#primitive`, and
   absent mirror states, ordering, inheritance, backfill, drift refusal, and
   rollback all remain observable.
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

## 5. Phase C0 — reconcile owners and rollback metadata

C0 is deliberately split. The first slice only installs explicit fixtures and
keeps Zig Codegen canonical. Rust Codegen is not selectable until C1 proves the
canonical family contract.

### C0a — metadata and fixture closure (first commit)

1. Update `abi/remaining-rungs-v1.json` with a schema/scope note for Codegen,
   place Codegen before Prelude in `load_order`, and add its canonical Zig,
   Rust fixture, explicit Zig fixture, dependencies, identity, and selectors.
   Append a dated current-owner projection to Design 64; do not rewrite its
   historical execution record.
2. Add `-Dcodegen-impl=zig|rust` to `build.zig`, but make `rust` fail closed
   as `codegen-impl=rust is not yet selectable` until C1's gate is enabled.
   Always install `libbirnam_id_codegen_zig.dylib` and the current opt-in
   `libbirnam_id_codegen_rust.dylib`; keep canonical
   `libbirnam_id_codegen.dylib` on Zig.
3. Make PEG selection install its explicit Zig fixture whenever Rust is
   canonical, so rollback paths are symmetric and do not depend on a checkout
   rebuild. Keep all other existing fixture names unchanged.
4. Extend the manifest, native-rung selection, install/relocation, and clean
   prefix checks to assert permitted Zig selection, refused Rust selection,
   exact canonical/fixture identities and digests, install root, and absence
   of static/runtime fallback paths.
5. Add a named `test-codegen-selection-refusal` target that asserts the exact
   `codegen-impl=rust is not yet selectable` diagnostic and does not treat a
   shell comment or an unexamined non-zero command as the gate.
6. Commit as `build: make residual owner and rollback metadata explicit`.

Required C0a verification:

```text
zig fmt --check build.zig
bash -n scripts/native-rung-selection.sh scripts/r6-acceptance.sh
zig build test-remaining-rung-manifest test-native-rung-selection \
  test-remaining-rung-clean-prefix test-install \
  -Dhost-impl=rust -Dcodegen-impl=zig
zig build test-codegen-selection-refusal -Dhost-impl=rust
zig build verify -Doptimize=ReleaseSafe --summary all -Dcodegen-impl=zig
```

### C0b — exact contract gate (after C1.6, before any Rust selection)

Only after the exact Codegen/Reflect selector, export, and layout diff in C1.6
is green may the build option permit `-Dcodegen-impl=rust`. Until then,
`-Dcodegen-impl=rust` must remain a deliberate, tested refusal rather than a
partially working Rust authority.

## 6. Phase C1 — freeze the Codegen replacement contract

This phase adds tests and Rust-side typed boundaries without selecting Rust
until C1.6. Each numbered item is a separate commit.

### C1.1 Selector/export/layout manifest

Extract the frozen `CodegenVt` (47 selectors), `ReflectVt` (12 selectors),
families, and auxiliary `ReflectFactCollectorVt` from `src/id/codegen.zig` and
`src/id/fixtures/codegen-authority-v1.json`. Enumerate the required object
exports explicitly: `CodegenVt`, `Codegen`, `ReflectVt`, `Reflect`, and
`ReflectFactCollectorVt`, including the collector's raw layout and
`value:value:value:` behavior. Add an automated exact-set diff: missing,
extra, renamed, and wrong-arity selectors fail closed. The current Rust
runtime's `RustCodegen`/`RustCodegenVt` names and Rust-only direct methods are
not accepted as canonical aliases. Include `discardCandidate:` and
`releaseCandidate:` and every frozen Reflect family/selector in the required
set. C1.1 intentionally lands with an expected-rejection test for the current
Rust fixture; parity is not claimed until C1.6. The named
`test-codegen-contract` target must report the Rust selector-count/set diff,
the opt-in `RustCodegen` export, and the absence of canonical `Codegen`,
`Reflect`, and `ReflectFactCollectorVt` exports before it passes.

### C1.2 Pure emitter parity

Complete `rust/birnam-codegen`'s source-preserving lowering for every currently
accepted Codegen expression, block, send, literal, embedded file, collection,
logic-variable, and diagnostic case. Compare generated bytes and normalized
diagnostics against the Zig oracle; preserve spans and lexical bytes instead of
reformatting source through Rust numeric/string formatters. Stress the existing
bounded depth and source-size limits.

C1.2 acceptance checkpoint (2026-07-26):

- Rust global references now use the frozen baked-symbol ABI: each distinct
  selector is emitted as a deterministic `__g<i>` slot and read through
  `birnam_global_b`; JIT units bind the interned symbol addresses and artifact
  units re-derive them in `__birnam_unit_init`.
- `test-rust-codegen-runtime` includes the expected-rejection contract probe
  and an 11-source pure-emitter byte/diagnostic corpus against the Zig oracle.
  The corpus covers literals, sends, lexical lets, vectors, maps, blocks,
  symbols, globals, strings, and invalid syntax.
- The R6 pins now require the resulting `93/93` focused tests (`170/170`
  steps), `28` Rust Codegen runtime tests, and optimized verify at `295/295`
  steps with `508/514` tests passed and six explicit skips. The complete R6
  wrapper passed with only the two previously accepted benchmark crossings.

### C1.3 Runtime IdApi adapter

Move the compiler-facing Rust runtime to the frozen `Codegen` and `Reflect`
family names, with the exact C1.1 selector sets. Implement canonical family
creation, compile/eval, metadata/source, Reflect, rooting, and error handling
through the existing PEG/Lang/TCC dynamic modules. Do not list a Rust-only
`emit:` or a non-frozen `compile:` alias as canonical. Candidate, generation,
and lifecycle protocols belong exclusively to C1.4. Keep all temporary Oops
rooted and release every Rust lock before an IdApi send.

### C1.4 Candidate and lifecycle parity

Port inspect/map/prepare/commit/discard/reconcile, phase transitions,
generation projections, observer/authorizer callbacks, and rollback. Prove
atomic activation and exact Store MOP-mirror facts with the existing Zig Store
and Rust Store fixtures. The Rust implementation must refuse inherited drift
and preserve the predecessor on rejected rollback without treating the Store
fact timeline as the method dictionary.

### C1.5 Generated artifact/TCC boundary

Run generated-C compilation through the dynamically selected TCC rung. Audit
install names, exported `birnam_*` helpers, callback signatures, disposable
handles, and hostile C inputs. Compare artifacts with the Zig Codegen oracle by
identity and behavior, not by private Rust representation.

### C1.6 Exact parity and opt-in enablement

Change C1.1's expected-rejection comparator to require exact Codegen, Reflect,
and `ReflectFactCollectorVt` parity in both fixtures, including the frozen
exports, selector sets, layouts, diagnostics, and negative cases. Only this
commit changes the build refusal into a permitted `-Dcodegen-impl=rust`
selection; canonical Zig remains selected until C3.

## 7. Phase C2 — image and host integration

1. Exercise Rust Codegen as the Image authority for v4 preflight, map, replay,
   source recovery, generation derivation, and forced rebuild. Run all four
   save/map/rebuild cross-products across Zig/Rust Image and Zig/Rust Codegen,
   plus the frozen pre-rewrite image through the explicit Zig oracle.
2. Run the Rust Image canonical path with Rust Codegen and with every retained
   Zig rollback reader. The Image transaction must restore Rust Codegen methods
   after a temporary Zig reader load and must leave no partial publication.
   `preflightImage:`, `mapImageCode:`, and `replayImageFacts:` are Image-owned
   extensions: remove them from the initial canonical Codegen surface and
   install them from the Rust Image boundary with the same ownership and
   selector behavior as the Zig Image path.
3. Switch the Rust host's canonical Codegen load only through the selected
   dynamic name. Project/check/build/run/install/test/LSP and source-free
   relocation must pass without importing `src/id/codegen.zig` directly.
4. Add a Rust-host smoke that proves no host helper silently loads the Zig
   Codegen fixture. Keep the explicit Zig oracle path for frozen compatibility
   and operator rollback.
5. Before C3, run two independent clean archive/prefix builds that compile the
   complete Birnam source closure using Rust PEG, Lang, TCC, and Codegen. Each
   run must begin without a prebuilt Birnam binary, image, compiled module, or
   Zig compiler implementation, and must pass source bootstrap, project,
   image, and relocation checks. Rust Codegen need not self-host, but the
   resulting complete compiler must be independent of the Zig implementation.

## 8. Phase C3 — canonical switch and observation window

1. Change the default `-Dcodegen-impl` to Rust in one focused commit; retain
   `-Dcodegen-impl=zig` as the documented rollback selection.
2. Run focused Codegen, Store/MOP, Image, TCC, Alien, project, LSP, and
   relocation suites in Debug and ReleaseSafe. Run `BIRNAM_R6_ISOLATE_IMPL=rust
   mise run r6-acceptance` and record exact counts and comparator output.
3. Run a second prefix/cwd and, after the canonical switch, build and execute
   a packaged distribution whose `bin/birnam` actually loads Rust Codegen.
   Verify the canonical Rust Codegen digest and the explicit Zig rollback
   digest in every receipt. No benchmark allowance is admitted without a
   dated, named comparator decision.
4. Observe one clean Rust-Codegen release/rollback cycle before changing
   distribution defaults. A distribution containing only Zig Codegen is not
   evidence for this plan. Do not delete Zig source or fixtures in this phase.

## 9. Phase C4 — host/helper disposition

After C3 is stable, classify every remaining Zig file by runtime role:

- `src/id/main.zig`: retained as the explicit Zig oracle; no production
  canonical path may depend on it after the Rust host gate.
- `project.zig`, `project_build.zig`, `project_check.zig`, and
  `runtime_resources.zig`: Rust host owns filesystem/process/resource
  observation; Birnam `project-policy.birnam`, TOML, Module, and Config remain
  the language-level policy surface. These helpers remain real dependencies of
  the Zig oracle/build until a clean archive proves the Rust host does not load
  them; remove one only in a separately reviewed retirement slice.
- `artifact_preflight.zig`: retained as a real Zig-oracle/build dependency,
  not misclassified as a test-only fixture. Its sentinel and focused tests are
  evidence, while the Rust host's artifact path is the canonical production
  observer. `artifact_preflight_sentinel.zig`, `gc_baseline.zig`,
  `membership_bench.zig`, ABI probes, and `*_test.zig` remain evidence tools
  unless a replacement test proves identical failure behavior.
- `id.zig`, `idapi.zig`, and the Zig rung sources: retain as the rollback/oracle
  implementation and contract comparator until a separately approved release
  removes them. `idapi.zig` is also a build/oracle dependency, not merely a
  test helper. Do not weaken the old-binary, image, wire, or generated-C
  fixtures.

Any file proposed for deletion requires a separate decision record naming its
replacement, fixture coverage, archive evidence, and recovery path.

## 10. Acceptance matrix

The Codegen switch is accepted only when all cells below pass in isolated
processes and prefixes:

| Cell | Required evidence |
| --- | --- |
| ABI | frozen IdApi manifest, exports, layouts, `module_init` once |
| Codegen/Reflect | exact frozen selector/export/layout set; no Rust-only canonical aliases |
| emitter | byte/diagnostic parity and bounded hostile corpus |
| MOP | dictionary authority, derived Store mirror, observer/authorizer, drift refusal |
| lifecycle | candidate inspect/map/activate/rollback with no partial mutation |
| TCC/Alien | generated C, callbacks, disposable handles, child-abort invariants |
| Image | v4 2×2 save/map/rebuild, corruption rejection, frozen image |
| host | project/check/build/run/install/test/LSP, source-free relocation |
| distribution | packaged Rust canonical and explicit Zig rollback oracle |
| bootstrap | two clean, prebuilt-artifact-free Rust PEG/Lang/TCC/Codegen source builds |
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

### Adversarial review 1 (2026-07-26)

The first adversarial review returned **NO-GO** and identified these concrete
risks: the opt-in Rust dylib currently exports `RustCodegen` rather than the
frozen `Codegen` family; its selector set lacks the frozen Reflect family and
candidate operations while exposing Rust-only direct methods; the original
bootstrap wording weakened the independent cold-compiler gate; the dependency
graph overstated `dlopen` edges; the residual manifest/history and PEG rollback
path were underspecified; `artifact_preflight.zig` and `idapi.zig` were
misclassified as evidence-only; and the Store/MOP law needed to distinguish
Store facts from its derived method mirror.

This revision splits C0 into a non-selecting fixture/metadata slice and a
post-C1 selection gate, requires an automated exact Codegen/Reflect set diff,
requires two clean Rust source-closure bootstrap builds, corrects the graph,
manifest, helper, Store/MOP, PEG, and packaged-observation language, and adds
the missing acceptance rows. No implementation slice may begin until a
follow-up review marks these corrections **GO**.

### Adversarial review 2 (2026-07-26)

The second adversarial review found five remaining plan hazards: C1.1 was not
landable while the current Rust fixture intentionally failed the frozen
contract; C1.3 and C1.4 overlapped; the auxiliary Reflect fact collector was
implicit; Image-owned recovery selectors were incorrectly left in the initial
Codegen surface; and C0a lacked a named refusal gate plus optimized
verification. The plan now lands C1.1 as an expected-rejection comparator,
reserves C1.6 for exact parity and opt-in enablement, separates compiler and
lifecycle scopes, enumerates the collector export/layout, moves the Image
extensions to the Rust Image boundary, and makes C0a verify the exact refusal,
optimized build, and install evidence. A final follow-up review is required
before implementation begins.

The final follow-up review returned **GO**: the phase order is landable, the
Codegen/Reflect/collector contract is fully frozen, dynamic-only ownership and
cold-bootstrap independence are explicit, and C0a can proceed without exposing
the incomplete Rust Codegen implementation.

Every numbered implementation item lands as one small commit after its focused
tests pass. Before each ownership switch, inspect the install tree, run the
cross-floor and forced-GC tests, run optimized `verify`, run serialized R6, and
run from a second prefix/cwd. Stop for human decision only if the invariant
evidence cannot distinguish a safe implementation from a change to a frozen
contract; otherwise resolve by the rules above.
