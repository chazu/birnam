# Remaining module contract inventory

Status: R0 execution checkpoint for
[`63 — Remaining Zig-module migration plan`](63-remaining-zig-module-migration-plan.md).

This is the frozen inventory for the Zig production owners that remain after
the Rust floor/compiler slices. It records the current dynamic boundary, object
families, payload/layout contract, roots/finalizers, foreign dependencies, and
behavior corpus. A Rust replacement must match this inventory before its
canonical-switch gate; a selector not listed here is not silently inferred from
the implementation.

## Inventory rules

- `module_init` is the only production entry point for a dynamic rung. The
  module must be loaded through the declared dependency closure; a test may use
  an explicit side-by-side fixture name, but production may not use that name.
- `OOP` means an exact traced slot; `raw` means an exact non-traced word or
  native allocation. `null` in a layout registration means every payload word
  uses the default kind supplied by the module.
- A selector is **native** only when this inventory assigns it to a vtable. A
  selector implemented by `stdlib.birnam` remains derived and is not ported as
  a second native method.
- Negative behavior (invalid arity/type, nil versus Error, and invariant abort)
  is part of each module's focused test corpus.

## `src/id/prelude.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | Rust Prelude is canonical via `module_init`; Zig Prelude remains an explicit rollback fixture and neither implementation owns Error |
| Exported objects/vtables | `Block`, `BlockVt`, `System`, `SystemVt`; Boolean is floor-owned; Error is imported lazily |
| Payload/layout | Block instances: captured OOP slots (`slot_oop`); System: zero payload; numeric/word payloads remain floor-owned |
| Roots/finalizers | No module root provider or finalizer; callbacks are bounded by the active send/GC root contract |
| Foreign dependencies | `Codegen` for `load:`, `sourceRevisionOf:`, `recordModuleSource:named:`, `evalPersisting:from:`, `lastError`; `Module` for `markLoaded:`; `dlopen`/`dlsym` for `loadModule:`; filesystem, environment, stdin/stdout, GC API |
| Native symbols | `module_init`; `loadModule:` resolves a foreign `module_init` and keeps the handle live |
| Tests | `src/id/prelude_test.zig`, `src/id/rust_prelude_test.zig`, numeric/Boolean/block/error tests, subprocess/bootstrap/project and R6 gates |

Native selectors by owner:

```text
Integer: + - * / mod: = < <= > >= printString asFloat to:do:
Float:   + - * / = < <= > >= negated asInteger abs floor ceiling rounded sqrt printString
Word:    printString = asInteger isNull
Object:  printString asString
Symbol:  printString
Boolean: ifTrue:ifFalse: ifTrue: ifFalse: & | and: or: not printString
Block:   whileTrue: whileFalse:
System:  print: println: readFile: writeFile:contents: getEnv: load:
         loadModule: readLine gc liveObjects autoGc: preciseGc:
```

The Rust Prelude’s current Error-compatible helpers are test scaffolding only.
The production Error family remains the standalone Error rung below.

## `src/id/collection.zig`

| Family/export | Payload and layout | Native selectors |
| --- | --- | --- |
| `Collection`/`CollectionVt` | abstract shared parent; no instances; no payload | derived higher-order protocol is installed by `stdlib.birnam` over native folds |
| `Vector`/`VectorVt` | `Vec = {count: raw usize, shift: raw usize, root: OOP, tail: OOP, tail_len: raw usize}`; exact kinds `[raw, raw, OOP, OOP, raw]` | `size`, `at:`, `add:`, `do:`, `inject:into:`, `printString`, `,`, `sortBy:`, `indexOf:`, `copyFrom:to:`, `reversed` |
| `VectorNode` | trie node; all payload slots are OOP | no public selector; traced node storage |
| `Range`/`RangeVt` | `Range = {start: raw i64, end: raw i64}`; all raw | `size`, `includes:`, `do:`, `inject:into:`, `at:`, `reversed`, `printString`, `,` |
| `Map`/`MapVt` | `Map = {root: OOP, count: raw usize}`; kinds `[OOP, raw]` | `at:put:`, `at:`, `at:ifAbsent:`, `includesKey:`, `size`, `keys`, `values`, `do:`, `inject:into:`, `keysAndValuesDo:`, `printString` |
| `MapNode` | `HNode` starts with raw `{tag:u32,n:u32,hash:u32,pad:u32}` (16 bytes), followed by `2*k` OOP entry slots; header is never traced | no public selector; bitmap/collision HAMT storage |
| `StringBuilder`/`StringBuilderVt` | `SbState = {ptr: raw native buffer, len: raw usize, cap: raw usize}`; all raw; `sbFinalize` frees the buffer exactly once | `new`, `clone`, `add:`, `addByte:`, `size`, `asString`, `printString` |
| floor Object/nil/String/Integer extensions | no new payload | Object/nil `isNil`, `notNil`, nil `printString`; Integer `to:`; String `,`, `printString`, `replace:with:`, `asSymbol`, `=`, `size`, `at:`, `byteAt:`, `copyFrom:to:`, `split:`, `trimmed`, `indexOf:`, `bracketDepth`, `asInteger`, `asNumber` |

Exports are the prototypes and vtables listed above. Constructors/owners are
part of the contract: `Vector`, `Map`, and `Range` export their prototype and
own their family; `Collection` is the abstract family handle; StringBuilder is
an ephemeral raw-buffer owner and does not participate in collection folding.

| Field | Current contract |
| --- | --- |
| Dynamic identity | Zig collection rung; `module_init`; production Rust name is `libbirnam_id_collection.dylib`; test fixtures use explicit `_zig`/`_rust` names |
| Roots/finalizers | temporary trie/map nodes are rooted across callbacks; StringBuilder uses `sbFinalize`; no collection-wide root provider |
| Foreign dependencies | floor `Integer`, `String`, `Symbol`, `Block`, `Error`, `System`, and GC/layout APIs |
| Tests | `collection_test.zig`, `collection_gc_test.zig`, `gc_layout_mark_test.zig`, conformance/null-args tests, image/store/codegen/relocation and R6 gates |

## `src/id/error.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | standalone Error rung; `module_init`; future Rust production name `libbirnam_id_error.dylib` |
| Exports | `ErrorVt`, `ErrorFactoryVt`, `Error` factory object; reload imports the existing family instead of minting it |
| Payload/layout | Error payload is exactly two OOPs `{kind, message}`; `ErrorVt` uses OOP default layout |
| Native selectors | `ErrorVt`: `doesNotUnderstand:`, `isError`, `kind`, `message`; factory: `kind:message:`, `kind:`, `message:`; floor Object: `isError`, `doesNotUnderstand:` |
| Roots/finalizers | no module root provider or native finalizer |
| Foreign dependencies | floor Object/symbol/string/intern/send and canonical `Error` family consumers |
| Native symbols | `module_init` |
| Tests | `error_test.zig`, DNU/numeric-fault/loop propagation, image/source/bootstrap, reload and preload-order gates |

## `src/id/isolate.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | Isolate rung; `module_init` plus four host C ABI exports |
| Exports | `IsolateVt`, `Isolate`, `IsolateHandleVt`, `IsolateFailureVt` |
| Payload/layout | `Handle = {token: raw u64}`; `Failure = {class, kind, message, exit_status, request_id}` (five OOPs); Isolate is zero payload |
| Native selectors | Isolate: `start:selector:image:`, private `_isolateSleep:`, `_isolateCrash:`; Handle: `request:timeout:`, `request:`, `stop`, `state`; Failure: `isError`, `class`, `kind`, `message`, `exitStatus`, `requestId` |
| Roots/finalizers | process registry is native state; `finaliseHandle` marks a handle entry orphaned; registry entries own pid/fd lifecycle |
| Foreign dependencies | Vector/Map codecs, Error family, process/pipe/wait/clock APIs, image path and worker executable policy |
| C ABI symbols | `birnam_isolate_worker_driver`, `birnam_isolate_worker_boot_error`, `birnam_isolate_drain`, `birnam_isolate_shutdown`; signatures, ownership, reentry, and worker/host lifetimes are frozen before Rust port |
| Wire contract | `BISO` wire-v1; 20-byte header; max frame 16 MiB, depth 64, container 65,536, nodes 1,000,000; exact `CodecError` enum and transferable-value rules |
| Tests | `isolate_test.zig`, wire corpus, malformed/hostile child, timeout/stop/reap, forced termination, relocation, and R6/verify gates |

## `src/id/relational.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | Relational rung; `module_init` |
| Exports | `LogicVarVt`, `LogicVarFactoryVt`, `LogicVar`, `EntityVt`, `RelCollectorVt`, `db` (the Store object) |
| Payload/layout | LogicVar: one OOP name symbol; RelCollector: one raw pointer to the native triple list; Entity and factory are zero payload |
| Native selectors | Store: `entity`, `findAll:where:`, `findAll:where:asOf:`, `findAll:where:since:`, `findAll:where:between:and:`, `count:where:`, `sum:where:`, `min:where:`, `max:where:`, `avg:where:`, `groupBy:aggregate:of:where:`, `defineRule:params:body:`, `rebuildRules`, `pull:spec:`; LogicVar factory `named:`; LogicVar `name`, `printString`; Entity `printString`; collector `value:value:value:` |
| Roots/finalizers | `gcRegisterRootProvider(&gcRoots)` pins the canonical logic-variable cache; collector callback state is rooted for the send lifetime; no native finalizer |
| Foreign dependencies | frozen Store object/message protocol (`Store`, `StoreVt`, live-triple callbacks), Vector/Map factories and vtables, canonical Integer/String/Symbol/Error |
| Native symbols | `module_init` |
| Tests | `relational_test.zig`, Store/MOP tests, image rule-cache rebuild, GC/reentry, query benchmark, Rust/Zig Store rollback matrix and R6 gates |

## `src/id/alien.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | Alien/FFI rung; `module_init`; TCC remains a declared dynamic dependency |
| Exports/layout | `AlienVt`, `Alien` (zero payload); Word is floor-owned and receives raw-buffer methods |
| Native selectors | Alien: `wrap:signature:as:on:`, `callback:signature:`, `releaseCallback:`, `libraryDir:`, `dlopen:`, `dlclose:`, `alloc:`; Word: `free`, `peekIntAt:`, `peekByteAt:`, `peekPtrAt:`, `peekDoubleAt:`, `peekStringAt:`, `pokeInt:at:`, `pokeByte:at:`, `pokePtr:at:`, `pokeDouble:at:` |
| Roots/finalizers | `gcRegisterRootProvider(&cbRoots)` pins callback `(block, unit)` pairs; TCC units use their disposable finalizer; library handles/path bytes are native tracked state |
| Foreign dependencies | `Tcc`, `Error`, `Vector`, `Store`, platform `dlopen`/`dlsym`/`dlclose`, libtcc generated C shims and platform ABI classifier |
| Native symbols | `module_init`; generated wrapper/callback C retains frozen `birnam_*` runtime symbols |
| Tests | `alien_test.zig`, ABI-shape/signature corpus, callback lifetime/release, GC, unload, hostile C, relocation and R6/verify gates |

## `src/id/image.zig`

| Field | Current contract |
| --- | --- |
| Dynamic identity | Image orchestration rung; `module_init`; Rust codec primitives remain a separate lower dependency |
| Exports | `ImageSaveReportVt`, `ImageSaveReport`; System methods are installed on imported `SystemVt` |
| Payload/layout | `ImageSaveReport` is a zero-payload family object; native `SaveReport` is process-global `{status, saved/recovered/rejected fact/slot counters, code_units, bounded detail}`; collector proto has one native callback and is not a durable heap family |
| Native selectors | System: `saveImage:`, `imageSaveReport`, `loadImage:`, `loadImage:rebuild:`, `imageCompositionIdentity`, `bakeExe:`; report: `status`, `savedFacts`, `recoveredFacts`, `rejectedFacts`, `recoveredSlots`, `rejectedSlots`, `codeUnits`, `detail`; collector: `e:a:v:tx:op:` |
| Roots/finalizers | heap walk roots are supplied by the floor/image transaction; no module root provider or finalizer; mapped code/dylib handles obey the image quiescence/rollback transaction |
| Foreign dependencies | `SystemVt`, `Store`, `Codegen`, `ErrorVt`, `Globals`, `IsolateHandleVt`, floor allocation/layout/global/send APIs, Rust/Zig image-v4 codec |
| Native symbols | `module_init`; image ABI retains the declared `birnam_*` floor helpers and image-v4 framing |
| Tests | `image_test.zig`, image heap/layout/swizzle/corrupt/rebuild/mapped-code cross-products, Store/Codegen/relocation, bake, and R6 gates |

## Host and helper owners

`src/id/main.zig` is not a dynamic rung and exports no `module_init`. It owns
the one raw floor `dlopen`/`id_init` startup, ordered runtime-module loading,
stack anchor, REPL/CLI/project dispatch, source/bootstrap recovery, image
boot, project check/build/install/test orchestration, and release resource
resolution. It imports `Artifact`, `Codegen`, `Alien`, `Store`, `Globals`, and
the project-policy bridges through the frozen object/message protocol.

`project.zig`, `project_build.zig`, `project_check.zig`, and
`runtime_resources.zig` are host-side helpers, not new runtime authorities:
they own filesystem/TOML/project resolution, artifact closure checks, install
paths, and declared embedded resources. Their focused tests are
`test-project-check`, install/relocation, source/bootstrap, and R6/verify.
Runtime lookup may not fall back to cwd, checkout source, or an unpinned
manifest.

## R0 acceptance record

- [x] Every remaining module has a checked-in export/vtable/selector inventory.
- [x] Payload sizes and mixed raw/OOP layout shapes are recorded.
- [x] Root providers and finalizers are named, including native state that is
      not an OOP payload.
- [x] Dynamic dependencies and host C symbols are explicit.
- [x] Focused test corpora and cross-floor gates are named.
- [x] Generate `abi/remaining-rungs-v1.json` and validate its canonical names,
      diagnostic identities, dependency identities, digest source, and isolated
      install paths with `test-remaining-rung-manifest`.
- [x] Add `test-remaining-rung-clean-prefix`: a relocated install run from an
      unrelated cwd containing decoy dylibs; runtime resolution ignores cwd,
      checkout, and ambient loader paths.
- [x] Add `test-remaining-rung-cross-floor`: the same current Prelude parity
      corpus runs in separate Zig-selected and Rust-selected processes. The
      missing Rust `load:` surface remains an explicit R0.5 completion item,
      not an erased requirement.

Execution evidence (2026-07-24):

- `0e76db7` froze this inventory; `347feb9` added the identity manifest;
  `ef430d2` added the clean-prefix relocation gate; and `b050b52` added the
  separate Zig/Rust Prelude cross-floor corpus.
- `zig build verify -Doptimize=ReleaseSafe --summary all` passed with
  `249/249` steps succeeded, `456/462` tests passed, and `6` existing fixture
  skips. The new manifest, relocation, and cross-floor gates were included.
- `mise run r6-acceptance` passed after the R0 changes: the focused aggregate
  was `42/42`, Rust Codegen runtime was `25/25`, the stack and Zig-store
  callback variants were `4/4` each, and Rust Prelude was `1/1`. The accepted
  benchmark crossings remained `query.aggregate +5.8%` and
  `compile.stage1-stdlib +11.9%`, with `regressions: 2, missing: 0,
  unusable: 0`.
- R0 is complete. The R0.5 Prelude completion and canonical-switch checkpoint
  below records the follow-on cutover.

R0.5 implementation checkpoint (2026-07-24):

- `7059b6a` removed Rust Prelude Error-family allocation and Object DNU/isError
  ownership. Faults now call the already-exported standalone Error factory and
  answer nil when that dynamic owner is not loaded. The same slice added the
  Codegen-backed `System load:` protocol.
- `8eef340` added a source-file parity corpus: valid source evaluates through
  `evalPersisting:from:`, broken source returns `#syntaxError`, and the test
  explicitly loads the Zig Error owner before exercising faults.
- `e1453c7` added `test-rust-prelude-io`, a separate executable with piped
  stdin/stdout proving `readLine` and `println:` outside the in-process test
  runner. R6 now includes this target and pins the focused set at `43/43`.
- `a14732f` added explicit `-Dprelude-impl=rust|zig` selection and clean-prefix
  rollback coverage; `48a719a`, `53d53f2`, `06bbf47`, and `94025f4` completed
  Word, loop, numeric-fault, and block-fault parity; and `8ba03b6` made Rust
  the canonical Prelude while retaining the Zig fixture.
- The post-switch `mise run r6-acceptance` passed with the focused set at
  `43/43`; optimized verify matched only the exact recorded `sortedModules`
  fixture exception, and the comparator retained only the accepted
  `query.aggregate +5.8%` and `compile.stage1-stdlib +11.9%` crossings.
- R0.5 is complete. The Zig Prelude remains available only with explicit
  `-Dprelude-impl=zig` rollback selection; the next implementation slice is
  R1 Collection.

R1 first implementation checkpoint (2026-07-24):

- `1d0c72e` added `rust/birnam-collection` as a locked-workspace `rlib`/`cdylib`
  crate and implemented the persistent 32-way vector storage kernel. Tests
  cover empty/bounds behavior, immutable append, tail-to-trie promotion,
  structural sharing, and deep growth through a root expansion.
- `f7c3934` added the exact `Vector`/`VectorNode` OOP payload layouts,
  `IdApi` method/export protocol, explicit `collection-impl` install choice,
  and the one-test Rust Collection acceptance target. The Rust wrapper is
  still an explicit fixture; Zig remains the canonical Collection owner.
- `75a49af` updated the relocated install smoke for the new workspace member.
  The post-change R6 focused aggregate is `44/44`, with full optimized verify
  and the two previously accepted benchmark crossings unchanged.
- `17d9e1a` completed the first Vector-native protocol slice:
  `inject:into:`, value-aware `indexOf:`, bounded `copyFrom:to:`, and
  `reversed` now run through the Rust wrapper. The subsequent R6 run remained
  green at `44/44`.
- `cc6cb7c` added the Rust Range family with the exact two-raw-word payload,
  `Range`/`RangeVt` ownership and exports, `Integer to:`, and all inventory
  selectors. The cross-language acceptance test covers ascending and empty
  descending ranges; `mise run r6-acceptance` remained green at `44/44`.
- `781b909` added the persistent Rust HAMT Map family with the exact
  raw-header/OOP-entry node shape, mixed Map layout, constructors, key/value
  equality and hashing, folds, iteration, absent fallback, and rendering. The
  expanded target and full R6 acceptance remained green at `44/44`.
- `b5b2448` added Rust StringBuilder with exact all-raw `{ptr, len, cap}`
  state, one-shot finalization, constructors, append/byte/size, and
  materialization/printing. Its cross-language coverage and full R6 acceptance
  remained green at `44/44`.
- `4ee08d0` added the retained native String selectors for concatenation,
  equality, indexing/slicing, split/trim/search, bracket depth, replacement,
  symbol conversion, and numeric parsing. The expanded target and full R6
  acceptance remained green at `44/44`.
- `eb3f97f` completed the Rust Collection bootstrap Object/nil protocol:
  Object `isNil`/`notNil`, nil overrides, and nil `printString` now match the
  Zig contract, with all five behaviors asserted by the cross-language test.
- `f7cf658` hardened the R6 gate to require `test-rust-collection` and its one
  passing integration test, not just the aggregate `44/44` result. The fresh
  full acceptance run passed with the existing exact verify exception and the
  two accepted benchmark crossings unchanged.
- The explicit Rust Collection selection smoke is green for the Rust floor:
  `test-rust-collection`, `test-collection`, `test-collection-gc`, and
  `test-gc-layout-mark` all pass with `-Dcollection-impl=rust`; Zig remains
  the repository default pending the R1 observation window.

R2 standalone Error implementation checkpoint (2026-07-24):

- `eef950a` added the standalone Rust Error crate with the exact two-OOP
  `{kind, message}` payload, OOP-default `ErrorVt` layout, family exports and
  reload behavior, factory selectors, propagating Error DNU, Object DNU swap,
  and identity `rust-error-r2-core`.
- `973990e` added explicit `-Derror-impl=zig|rust` installation and the
  `test-rust-error` cross-language target. Both optimization modes pass on
  Zig and Rust floors; Zig remains the default owner.
- `867a68a` updated the relocated install smoke for the new Cargo package and
  both Rust/Zig Error fixture dylibs. `test-install` and
  `test-native-rung-harness` pass.
- `79a84cd` extended the Rust Error corpus through Rust reload and Rust→Zig
  reload, proving the existing `ErrorVt`, factory, two-OOP payload, and exports
  are retained rather than duplicated.
- `bc8c119` executes isolated Rust→Zig and Zig→Rust preload-order runs, with
  both orders retaining one family and stable export/payload identity.
- `3edc2b6` added both Rust Error preload-order runs to the R6 latest-tests pin
  and requires both root-level integration results.
- `8b363f9` added the Rust Prelude/Rust Error-owner run, including the Rust
  identity assertion, to the R6 latest-tests pin.
- `329efca` added five-deep Error propagation and precise-GC retention checks
  to both preload-order runs.
- The latest full R6 run remains green at `48/48`, with the exact recorded
  verify exception and the two accepted benchmark crossings unchanged.

R2 canonical switch checkpoint (2026-07-24):

- `6a19b9b` selected Rust Error as the canonical owner while preserving the
  explicit Zig rollback fixture and option.
- Post-switch focused Prelude/Error, selection, install, and full R6 checks
  passed at `48/48`; no new verify failure or benchmark crossing appeared.
- `test-cross-floor-errors`, `test-cross-floor-images`, and
  `test-remaining-rung-cross-floor` also passed after the switch.

R3.1 implementation checkpoint (2026-07-24):

- `e9d7f96` added the pointer-free Rust BISO wire-v1 header and
  scalar/vector/map codec as an `rlib`, with six focused tests and clippy
  clean under the frozen workspace lock.
- The unrelated whole-workspace Cargo run still aborts in the existing
  `birnam-codegen` unbounded-depth stress test; the new Isolate crate remains
  green in isolation.

R3.2 host ABI checkpoint (2026-07-24):

- Frozen declarations are `int birnam_isolate_worker_driver(IdApi*, int,
  Oop, Oop)`, `void birnam_isolate_worker_boot_error(IdApi*, int, const char*,
  const uint8_t*, size_t)`, `void birnam_isolate_drain(void)`, and `void
  birnam_isolate_shutdown(void)`.
- The worker owns its inherited channel and borrowed process-local OOPs only
  for the synchronous call; boot-error strings are borrowed, not retained;
  drain/shutdown are host-side idempotent registry lifecycle operations. The
  Rust rung must preserve this ABI and ownership model exactly.

R3.3 wire probe and R6 gate checkpoint (2026-07-24):

- `831a6f9` adds the installed Rust Isolate `cdylib` identity/header probe,
  including exact BISO bytes and fail-closed invalid-input cases.
- `ff07369` adds the Rust Isolate package and fixture to relocated install
  metadata; `f6cb648` keeps the canonical Zig Isolate dylib installed beside
  the explicit Rust fixture so clean prefixes retain both rollback paths.
- Clean-prefix install/selection/native-rung checks and the full R6 wrapper
  pass. The focused boundary is pinned at `49/49`, with the existing exact
  verify exception and only the two accepted benchmark crossings; no Isolate
  worker lifecycle or canonical-switch claim is made by this probe.

R3.4 dynamic frame-validation checkpoint (2026-07-24):

- `333fecd` adds the pointer-checked Rust `birnam_isolate_rust_decode_header`
  export and extends the Zig probe with successful field decoding, bad-magic,
  and null-input cases.
- Rust unit/clippy checks and Debug/ReleaseSafe dynamic Isolate probes pass;
  this remains codec/ABI evidence only and does not claim worker lifecycle
  parity or a canonical Rust Isolate selection.

R3.5 owned frame transport checkpoint (2026-07-24):

- `b372c59` adds an owned Rust `Frame` wrapper with exact payload-length,
  empty-control-frame, truncation, and trailing-byte validation; payload bytes
  are copied across the transport seam.
- Seven Rust unit tests and clippy pass. The worker still has no `IdApi` or
  process lifecycle implementation at this checkpoint.

R3.6 control-plane skeleton checkpoint (2026-07-24):

- `bc69774` adds the frozen `birnam-abi` dependency and the four audited Rust
  host symbols. The skeleton performs ready/stop/stopped framing and exact
  boot-error record encoding; `drain`/`shutdown` are no-ops while its registry
  is empty.
- Nine Rust unit tests and clippy pass, including socketpair control-plane
  and boot-error tests. Request dispatch remains fail-closed until the OOP
  value bridge is ported; this is not a worker-parity or selection checkpoint.

R3.7 installed host-symbol checkpoint (2026-07-24):

- `b12ef26` makes the dynamic probe require the four frozen host symbols and
  invokes the lifecycle entry points without selecting the Rust rung.
- Debug and ReleaseSafe probes pass; request dispatch remains intentionally
  unimplemented pending the IdApi/OOP bridge. The subsequent full R6 rerun
  remains green at `49/49` with the existing verify and benchmark boundaries.

R3.8 scalar OOP bridge checkpoint (2026-07-24):

- `b2cac5f` adds the Rust `IdApi` scalar encoder for nil/booleans, Integer,
  Float, String, and Symbol, with caller-owned output sizing and fail-closed
  unsupported-vtable behavior.
- The dynamic corpus covers signed bits, negative zero, embedded-NUL strings,
  symbols, unsupported objects, and short buffers. Vectors/maps and request
  dispatch are explicitly not covered yet.

R3.9 collection OOP bridge checkpoint (2026-07-24):

- `f9b5774` traverses live `VectorVt`/`MapVt` objects only through the frozen
  `size`/`at:`/`keys`/`at:put:` selectors, with bounded counts, map-key checks,
  and active-address cycle rejection.
- The probe loads the Rust Collection fixture and verifies exact vector/map
  bytes in both optimization modes. Decode-to-OOP and worker request dispatch
  remain unimplemented.

R3.10 OOP decode bridge checkpoint (2026-07-24):

- `c2baa52` adds the Rust BISO-to-floor decoder through the frozen `IdApi`.
  Scalars use canonical constructors/interning; Vector and Map values are
  rebuilt through the live Collection selectors rather than a Rust-private
  representation. Depth, node, container, map-key, duplicate-key, malformed,
  and trailing-byte failures remain fail-closed.
- The installed dynamic corpus covers scalar, embedded-NUL String, Symbol,
  Vector, Map, trailing-byte, and unknown-tag cases in Debug and ReleaseSafe.
  Rust unit tests and clippy pass. Worker request dispatch, hostile-child
  lifecycle behavior, and canonical Rust selection remain pending.

R3.11 worker request-dispatch checkpoint (2026-07-24):

- `4f98d8b` replaces the Rust one-frame control skeleton with a repeated worker
  loop. Request payloads decode into floor OOPs, dispatch through the borrowed
  receiver/selector, and return exact `ReplyOk`, `ReplyError`, or
  `ProtocolError` frames; stop returns `Stopped`. Rust owns no process handle,
  channel descriptor, receiver, selector, or transport buffer after return.
- The dynamic probe loads Rust Prelude and Error owners and verifies two
  owner-thread request/reply exchanges plus stop/stopped in Debug and
  ReleaseSafe. Hostile-child lifecycle, timeout/termination parity, and
  canonical Rust selection remain unimplemented.
