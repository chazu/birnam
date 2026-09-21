# Changelog

All notable changes to birnam are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions are
[semver](https://semver.org/). Birnam currently supports arm64 macOS only;
other targets are rejected at build time until their libtcc and shared-library
loader ports are complete.

## [Unreleased]

- Construct TCC, Portable Code and Codegen through Bootstrap. Complete admission
  and Portable Code before their consumers, and supply Store compensation before
  publishing Codegen methods. Remove the three legacy host initialization rows
  and retain temporary constructor roots for the new families. Bind generated C
  helpers explicitly so compilation does not depend on global library visibility.
  Artifact loading explicitly exposes the completed TCC/Codegen C ABI required
  by the existing Mach-O format, without changing Id's local native loader.

- Construct Artifact, PEG and Lang through Bootstrap with completed values and
  explicit PEG input, removing three legacy host entries. Build the real grammar
  and exercise parsing before TCC, Codegen or compiler admission are available.
  Refresh the control/collection export manifests and check them against installed
  libraries in the native loading suite.

- Construct complete collection families and Store through Bootstrap, removing
  their legacy host initialization and deferred family preparation. Keep new
  objects/selectors rooted during construction and pass Vector's exact typed
  concatenation callable to Range.

- Construct Integer/Boolean control, Block loops and printing through Bootstrap
  with explicit completed dependencies; remove their legacy host initialization
  entries and test the group before collection/Store/compiler libraries exist.

- Remove the host's second initialization pass for fourteen completed lower
  modules and its obsolete Float/Word/String preparation path. Upper legacy
  consumers receive the values already constructed by Bootstrap.

- Move the production dependency sequence through Runtime into the loaded
  Bootstrap module. Host prefix diagnostics use that same constructor; Bootstrap
  owns predecessor loading, root adoption, collector activation and Runtime inputs.

- Decode generation method selections once per authority calculation instead of
  rescanning the complete wire table for every method during startup and recovery.

- Resolve generated global reads by name instead of embedding unrooted Symbol
  addresses. Code compiled before a nested source load now sees the globals it
  publishes, including the test framework under forced-source startup.

- Retain anonymous compiled Units through reflected Methods and aliased method
  bindings, including synchronous and resumable Blocks. Lifetime v2 traces
  references held in native state without keeping unreachable cycles alive.
  Removing a binding defers its Unit's release until the outer turn returns.
  Ordinary source Units and their caches now collect when their last live
  reference disappears. Synchronous generated invocation checks arity and
  Block capture storage.

- Add owned Method values for language reflection. Retain exact implementations
  across replacement, removal and collection; preserve native contexts and
  generated-code leases. TCC and Portable Code use checked language sends.
  Image saving rejects process-local Methods with `methodNotImageable`.

- Construct Runtime with completed Integer, Float, Word and String inputs (v15).
  Move compiler admission/catalogs and optimization registries into an explicit
  later native extension, required by Codegen. Base Machine/Block and GC work
  before compiler services mount.

- Construct native Block identity/layout in Runtime and install Block loops at
  their own module entry. Install Object/Symbol printing before collections, and
  load Boolean control, collections and Store before Artifact, PEG, Lang, TCC
  and Codegen; Prelude now extends System.

- Load the remaining Runtime support factories through Id and supply their
  complete operations explicitly to Runtime v14. Remove six host preparations,
  normal dependency-setter exports, and three empty legacy module entries.

- Separate the production Id and Runtime packages. Id depends only on the ABI
  crate and exports its native module entry and identity. The host constructs Id
  through Siphon, then loads the separate Runtime through Id after its native
  predecessors. Install both libraries; keep compiler integration and remaining
  legacy factory preparation entirely above Id.

- Make Id's allocation, Symbol and dictionary implementations private to its
  module. Ordinary Runtime uses native clients; local registry backends compile
  only for tests and explicit diagnostic builds. Share the existing object-header
  geometry through the ABI and keep the payload scanner above Id.

- Retain original Id entries before loading upper modules, and pass them to
  Runtime through the temporary v13 constructor. Remove private Id function
  references from legacy method adaptation and Integer/lookup admission;
  preserve exact function/context checks and effects of earlier replacements.

- Keep Id's production state in Id and pass its native carrier explicitly into
  Runtime construction. Query live initial objects through messages and retain
  query results across construction. Remove the private Rust owner handoff and
  stale Symbol-prototype pointer; publish the temporary Runtime v12 constructor.

- Make saved Id lookup/add/remove entries independently callable with their own
  dictionary context. Remove Id's thread-local dispatch scope and preserve the
  exact native function/context through the upper legacy invocation bridge.
  Restoring a legacy primitive recovers its complete captured Id entry.

- Keep Symbol names and method dictionaries owned by Id through upper construction.
  Runtime uses completed native registry operations and asks Id to perform
  effectful binding without supplying a raw dictionary pointer. Preserve exact
  method/context selection, ordered views, opaque weak-key shadowing, mutation
  epochs and upper executable-retirement accounting.

- Keep production allocation ownership in Id through Runtime construction.
  Heap uses native Id entries and copied allocation facts instead of owning or
  borrowing the allocation registry. Retain working non-collecting primitives
  for deferred materialization; ordinary construction preserves collection and
  accounting. Add exact address/count queries and the temporary Runtime v11 bridge.

- Produce diagnostic allocation and representation snapshots in native Trace
  through Id's message protocol. Runtime joins these facts with its own roots,
  collection permanence and deferred journals without reading Id's private
  storage records. Publish Trace graph v2 with callback-scoped inspection.

- Activate production collection before Runtime: pause Bootstrap publication,
  validate the existing heap, establish a lasting stack boundary and install
  collecting allocation, delegation, interning and clone methods through Id.
  Protect constructor inputs and Runtime construction roots. Failed activation
  leaves collection disabled without sweeping; native trace hooks count as
  payload coverage. Publish Bootstrap v2, Native Stack v4 and Collector v3.

- Make saved early collector entries use Runtime's complete roots and retirement
  extension after adoption. Runtime installs the extension after Execution
  construction; ordinary collection now supplies only its native interval.
  Collector v2 and the temporary Runtime v9 bridge make that contract explicit.

- Admit production native Integer construction and arithmetic to compiler
  optimizations using live dispatch checks and exact function/context pairs.
  Custom allocation, lookup and Namespace operations retain their observable
  effects. Closure admission recognizes the native owner/parent protocols;
  optional numeric v5 declarations keep compiler policy above Id.

- Preserve native method context and typed operands when a legacy custom lookup
  delegates to Id's lookup. Nested lookups keep separate temporary results, and
  replacing a binding does not change the callable already selected. This fixes
  Integer construction through the deferred-identity fixture's custom lookup.

- Moved production tracing, weak metadata retirement, finalizer detachment and
  allocation release into Collect, constructed from completed native owners before
  Runtime. Runtime supplies scoped upper roots and Execution/deferred retirement
  operations. Collection state v2 and the temporary Runtime v8 bridge preserve the
  existing allocations and controls; activation/construction migration continues.

- Moved register capture and native stack interval validation from Id into Native
  Stack's v3 factory. Collect receives the complete operation before Runtime;
  collection and source probes use the same native implementation. Replaced the
  v2 factory/dependency exports and retained the public setter and image formats.

### Changed

- Construct production collection records and controls through Id before Runtime.
  Collect owns the mark table, allocation budget, phase, stack anchor and settings;
  Runtime adopts those same records, preserving early flags/settings/accounting
  and registering only later allocations. Remove four legacy factory preparations
  from normal startup. Explicit collector activation and retirement-policy
  separation remain unfinished.

- Construct the production live Trace graph through Id before Runtime, using
  completed root, Lifetime, Object Model and Native Mark owners. Actual collection
  uses that same graph for payloads, inherited hooks and behavior-owner edges.
  Remove the production Native Mark preparation inside Id and add non-collecting
  allocation/description validation; keep collector activation
  and upper retirement policy explicitly unfinished.

- Complete Design 83's early value/text stage: install String's fourteen text
  and numeric methods and Nil/Boolean printing through native extension entries
  before Block or Runtime. Use completed predecessor contexts, preserve early
  bindings through later mounting, and root temporary printable results across
  collection callbacks. Share the existing pure text algorithms with legacy
  clients; retain byte indexing, NUL conversion and numeric parsing behavior.
- Construct production Word through Id before Block or Runtime (Design 83), with
  the existing unsigned payload, equality, hexadecimal printing and checked
  Integer conversion. Overflow errors use the current Error factory. Skip legacy
  Word preparation, adopt the same table into upper consumers and preserve early
  native methods and replacements through collection.
- Construct production Float through Id after Integer and before Word, Block or
  Runtime (Design 83). Install Float arithmetic, comparison, conversion and
  printing using completed predecessors, then add Integer's mixed-number methods
  through an explicit native extension entry. Preserve early replacements and
  copied-method behavior. Reuse the existing bits codec and tables, skip legacy
  Float preparation in production, and adopt the same objects into Runtime/GC.
- Construct production Integer through Id before Float, Word, Block or Runtime
  (Design 83), with working arithmetic, comparison, printing and Error propagation.
  Adopt the same identities and storage into upper consumers; preserve early
  method replacements when adding mixed-number operations. Root cached selectors
  in Integer, String and Error even after method removal. Cover the real early
  prefix, precise GC and source/compiled/saved/baked execution. The legacy direct
  allocation compiler optimization is declined by native construction; its
  replacement and the later native numeric extensions remain unfinished.
- Construct production Error through Id before Integer, Float, Block or Runtime
  (Design 83). Keep the existing two-Oop representation and factory selectors,
  with working recognition, propagation and missing-message errors using early
  String storage. Preserve native methods, layouts and replacements through
  Runtime adoption and late mounting; transfer Bootstrap's result root to the
  published owner. Check payload reads/writes and cover early construction,
  precise GC, source/compiled execution and saved/baked images.
- Construct production String storage through Id before Error, numbers or Runtime
  (Design 83). Own, copy and read bytes through native operands using the existing
  String representation and Lifetime finalizers; adopt the same storage in upper
  consumers and skip legacy String preparation. Keep numeric/text methods absent
  until their dependencies mount. Check replacement allocation results before
  writing payloads, preserve clone/new bindings, and cover early ownership,
  adoption and collection with the installed-library native fixture.
- Construct Object Model ownership, private behaviors and native enumeration
  through Id before Runtime (Design 83). Adopt the same weak owner records into
  collection, retain the actual native private-construction entry, and remove
  Object Model's production factory preparation. Give upper construction methods
  explicit Runtime contexts for calls through Id. Runtime's temporary constructor
  is now v5; value staging, collector activation and package separation remain.

- Construct the production Namespace through Id after Lifetime/base values and
  before String or Runtime (Design 83). Retain bindings through its native root
  provider, adopt the same table in upper startup, and preserve early names and
  aliases. Remove Namespace's production factory preparation and replace the
  temporary Runtime v3 bridge with v4. Object Model, remaining value stages,
  collector activation and Runtime/compiler separation remain unfinished.

- Construct Nil/Boolean identities and basic operations through Id and Lifetime
  before String, numbers, Block or Runtime (Design 83). Upper startup adopts the
  same objects, layouts and method bindings; later Nil/Boolean entries add
  printing and Block control. Replace the temporary v1/v2 Runtime bridges with
  one v3 constructor accepting these completed values. Remaining value stages,
  collector activation and Runtime/compiler separation are still unfinished.

- Load Bootstrap immediately after Id and retain native initialization results
  before Lifetime exists (Design 83). Transfer the early roots into the actual
  root registry, retain later results there, and release them explicitly when
  callers transfer ownership. Root/Lifetime construction uses this same loader;
  nested loads preserve their results and raw native failures. Legacy module
  construction, collector activation and Id package separation remain unfinished.

- Construct production Lifetime payload descriptions, trace hooks and finalizer
  records through Id before Runtime (Design 83). Runtime, collection and Image
  use that same native owner; behavior/heap tables no longer hold those records.
  Keep behavior metadata retirement separate from finalizer detachment, and
  preserve existing public descriptor APIs. Remaining module construction, staged
  GC activation and the Runtime/compiler package separation remain unfinished.

- Initialize the production root registry through Id before Runtime (Design 83).
  Its native retained-root records and registration operations work without GC or
  Runtime callbacks. Host construction now adopts that exact owner; early native
  module roots participate in later collection and can be released. Complete
  module construction and staged collector activation remain unfinished.

- Move production collection's Id index retirement, allocation detachment and
  release onto the native storage protocol (Design 83). Upper GC metadata retires
  separately; finalizers retain readable detached payloads until the collector
  invokes their one-use release entries. Native Bytes/Word operands distinguish
  these operations from safe language Object sends. Lifetime construction and
  the remaining internal storage-consumer migration are still unfinished.

- Expose Id roots, allocations and representation references through native
  visitor messages (Design 83). The production collector acquires the completed
  storage callables through Id during construction and uses them during marking;
  payload policy and native owners remain upper responsibilities. Snapshot
  visitors may reenter Id, and failed visitors stop with their exact outcome.
  Lifetime construction and the remaining upper storage-consumer migration
  remain unfinished.

- Let native module initializers construct behaviors and objects through Id
  messages before upper modules exist (Design 83). Add native queries for the
  Object prototype, behavior, allocation size/identity and Symbol bytes. Adopt
  the same objects into Runtime, then replace construction with GC-accounted
  implementations. The separate Lifetime/Bootstrap construction path remains
  unfinished; native and image layouts are unchanged.

- Carry complete native function/context entries through upper lookup, inherited
  caching and Machine calls (Design 83). Context-only replacement and effectful
  lookup retain the exact selected implementation. Native call/lookup failures
  become language Errors; raw non-object results require value wrappers. Advance
  Dispatch Cache to ABI v2; retain image runtime ABI 7. Legacy reflective Method
  results and upper ownership still require migration.

- Load the real production Id through a dedicated Siphon and the common typed
  native-module protocol before upper-runtime preparation (Design 83). Adopt
  that same Id graph and carrier during existing runtime construction. Close
  rejected native images before entry and pin entered images even when an
  initializer reports failure after publication. Upper ownership, staged
  construction and legacy consumer migration remain in progress.

- Use typed native operands, method entries and outcomes in production Id
  dictionaries and dispatch (Design 83), with a frozen independent C ABI.
  Keep the existing all-object calling convention in an upper Runtime migration
  adapter. Dictionary value cells grow from 8 to 24 bytes; advance image runtime
  ABI to 7. Upper consumer migration remains in progress.

- Run production Id lookup and metacircular binding without Runtime or Execution
  policy (Design 83). Install its initial mutation methods through Id's own send
  before upper construction. Resumable lookup uses an explicit native adapter;
  Execution registration alone no longer substitutes for a primitive's native
  body during lookup. Typed native loading and upper ownership remain in progress.

- Construct the initial Id object graph before Heap and behavior metadata in
  production bootstrap (Design 83). Adopt the same allocations and identities;
  retain the existing permanence and collection rules. Initial Symbol enumeration
  now starts with `lookup`, `doesNotUnderstand:` and `Symbol`. Host preparation
  and primitive method installation still await the full loading cutover.

- Make Id's behavior dictionaries and Symbol implementation independent of GC
  services (Design 82/83). Keep layout, hook and private-owner metadata above the
  40-byte dictionary payload. Return dictionary indexing and mutation counts to
  Id and remove both pre-Id factories. Advance image runtime ABI to 6; the image
  format and IdApi table shape are unchanged. Upper Runtime relocation and the
  staged production loader remain in progress.

- Separate Id object allocation from Heap's GC bookkeeping (Design 82/83).
  Return the allocation identity counter to Id and remove its Storage factory
  and pre-Id preparation. Preserve object layout and identities across adoption
  of existing allocations. The separate native allocation/GC maps add metadata
  overhead; staged collector activation and independent Id remain in progress.

- Preserve one effectful lookup per missing Machine send: resolve
  `doesNotUnderstand:` directly after the original miss, using the same
  forwarding path as native sends.

- Implement Design 83's managed-turn lifetime policy in the production runtime:
  nested sends share the outer turn, checkpoint GC roots the outward result,
  and reentrant cleanup cannot recursively drain retirements. TCC and mapped
  image finalizers transfer native owners to detached queue records; candidate
  and CodeBinding reaping runs at eligible checkpoints. Replaced bindings can
  now be reaped automatically before a later explicit reap. Moving this owner
  out of Id and replacing production construction remain outstanding.

- Begin Design 83 production migration: generated resumable adapters enter the
  exact implementation selected by lookup, preserving that answer across binding
  replacement. Wire JIT and native artifact callers to the same entry. Advance
  the image runtime ABI to 5 for the required entry symbol. Id still owns the
  upper runtime until the remaining ownership and bootstrap cutover lands.

- Address the adversarial review of minimal Id in Design 83 and revise Design 82:
  specify upper Execution turns instead of per-send execution protection in Id,
  define native loading results and failure lifetime, break construction cycles
  with explicit later extensions, and define safe collector activation over
  early module roots. Record the trusted-native safety boundary. These are design
  changes; the production implementation is unchanged.

- Record Design 82 before production Id decoupling: diagnose the inverted
  dependencies left by the Rust extractions, specify the ad-hoc-to-real Id
  loading protocol, and require independent production Id startup as acceptance.
  Supersede the further-extraction and Integer-first recommendations. Runtime
  code and ABI are unchanged at this design checkpoint.

- Close the preparatory Design 81 Rust extraction phase at 81CA. Defer the
  snapshot-buffer proposal (81CB) and remove its uncommitted prototype, preserving
  the existing runtime and ABI. Consolidate production ownership and remaining
  Birnam implementation work, refresh bootstrap and IdApi documentation, and
  archive superseded local baseline copies while removing disposable build caches.


- Move lazy deferred-clone index storage into Storage (81CA). Id retains
  ordered handle lookup, numeric fallback, displaced-index repair, journal
  charging and materialization. Empty indices remain unallocated and clear
  retains capacity; public formats and identity sequences stay unchanged.

- Move the deferred-allocation identity storage cell into Storage (81BZ).
  Id retains shared clone/numeric issuance, checked exhaustion, journal admission,
  charging and recovery. Physical allocation identities and public formats stay
  unchanged.

- Move the Heap's allocation-identity storage cell into Storage (81BY). Id keeps
  checked issuance, the 59-bit exhaustion bound, packed block metadata and byte
  charging. GC and address reuse do not reset identities; public formats and
  library inventory remain unchanged.

- Move derived Vtable key-index storage into Object Model (81BX). Id retains
  threshold 16, ordered keys and current values, rebuilds, weak compaction and
  mutation accounting. Small dictionaries allocate no native index owner;
  clearing retains capacity. Public formats and library inventory are unchanged.

- Move each Vtables registry's mutation-epoch cell into Object Model (81BW).
  Keep saturation, mutation counts and lookup/native-tier readers in Id. Startup
  and compiled-image checks preserve live dictionary and behavior changes,
  active Frame slot guards, aliases and GC. Public formats are unchanged.

- Move per-Runtime lookup and Machine-slot guard-verdict cells into Dispatch
  Cache (81BV). Keep eligibility evaluation, mutation checks and fallback in Id,
  with independent owners and unchanged GC behavior. Startup and compiled-image
  checks cover active Frame reads/writes and live overrides; library inventory,
  IdApi and image formats are unchanged.

- Extract the per-Runtime inherited-lookup table into Dispatch Cache (81BU).
  Preserve direct lookup, collision handling, live mutation, GC invalidation and
  fresh target classification. The Siphon loads the new native module; IdApi,
  native initialization and image formats are unchanged.

- Move per-Runtime storage-observation epoch and admission flags into Storage
  (81BT). Preserve runtime identity, permission consumption, live dispatch,
  visitor failure and guard cleanup. The host prepares the factory before Id;
  public storage ABI, IdApi and image formats are unchanged.

- Move per-Runtime collection-phase ownership into Collect (81BS), preserving
  allocation and storage admission, dispatch-cache bypass, callback restrictions
  and guard reset. Keep the coordinator's reentry guard separate. The host
  prepares the phase factory before Id; public IdApi and image formats are unchanged.

- Extract per-Heap allocation-byte budget ownership into Allocation Budget
  (81BR). Preserve charge and refusal order, overflow diagnostics, deferred
  materialization, collection timing and reset. The Siphon prepares the new
  dependency before Id initialization; IdApi v20 and image v8 remain unchanged.

- Move native stack word scanning into Native Stack (81BQ) through its v2
  implementation factory. Preserve physical capture in Id, incremental word
  reads, exact sink failures and existing public anchor assignment. IdApi v20,
  native initialization and image v8 remain unchanged.

- Extract native mark-callback scope ownership into Native Mark (81BP), preserving
  per-floor/thread selection, provider and hook ordering, emission reentry,
  unwind cleanup and fail-stop diagnostics. IdApi v20 and image v8 are unchanged.

- Extract native stack-anchor ownership into Native Stack (81BO). IdApi v20
  has 18 fields / 144 bytes. The host sets the anchor through an explicit
  capability; native initialization keeps twelve arguments. Preserve register
  capture, scan bounds, automatic collection readiness and callback-time changes.
  Native consumers require rebuilding; image encoding is unchanged.

- Extract native finalizer registration into Finalizers (81BN). IdApi v19 has
  19 fields / 152 bytes; native initialization receives a twelfth capability,
  forwarded by Native Loader v12. Preserve canonical native allocation, saved
  execution selections, weak lifetime, cloning and ordered live replacement
  during collection. Native consumers require rebuilding; image encoding is
  unchanged.

- Extract native trace-hook registration and bounded inherited selection into
  Trace Hooks (81BM). IdApi v18 has 20 fields / 160 bytes; native initialization
  gains an eleventh capability, carried by Native Loader v11. Preserve saved
  execution selections, live parent/hook reads, raw vtable mirrors, weak record
  lifetime and GC callback order. Native consumers require rebuilding; image
  encoding is unchanged.

- Extract exact-vtable payload descriptions into Payload Layout (81BL), preserving
  copied prefixes, arbitrary slot codes, noninheritance and record retirement.
  IdApi v17 has 21 fields / 168 bytes; native initialization gains an explicit
  tenth capability. Native Loader v10 and early value preparation carry the
  matching selection, including TCC's saved Unit layout capability on reload.
  Native consumers require rebuilding; image encoding is unchanged.

- Extract automatic and precise GC settings and the 16 MiB collection predicate
  into GC Settings (81BK). IdApi v16 has 23 fields / 184 bytes; nine-argument
  initialization, Native Loader v9 and System GC installation v2 carry explicit
  settings selections. Preserve per-object precise reads, allocation checkpoints,
  callback admission and Codegen/Image suppression timing. Native consumers
  require rebuilding; image encoding is unchanged.

- Extract the ordered native root-provider list into Root Registry (81BJ).
  IdApi v15 has 25 fields / 200 bytes; eight-argument initialization and Native
  Loader v8 carry explicit registration capabilities. Early String, Float, Word,
  Range, Vector and Map preparation receive their matching incoming selection.
  Preserve admission, duplicate entries, the 32-entry limit, callback timing and
  copied GC/diagnostic snapshots. Native consumers require rebuilding; image
  encoding is unchanged.

- Move the native pre-overwrite hook into Method Observation (81BI). IdApi v14
  has 26 fields / 208 bytes; seven-argument initialization and Native Loader v7
  carry the explicit capability. Preserve exact direct-binding notification,
  pre-mutation timing, Store incoming registration/reentry, and silent rollback.
  Dictionary mutation and executable lifetime accounting remain in Id. Native
  consumers require rebuilding; image encoding is unchanged.

- Move native selector enumeration into Object Model (81BH). IdApi v13 has
  27 fields / 216 bytes; enumeration v2 carries direct dictionary selection
  through the existing sixth initialization argument and Native Loader v6.
  Preserve ordered keys, invalid-vtable admission, live callback selection and
  pure Block-constructor admission through its v5 declaration. Native consumers
  require rebuilding; image encoding is unchanged.

- Move native vtable count and indexed observation into Object Model (81BG).
  IdApi v12 has 28 fields / 224 bytes; six-argument native initialization carries
  the matching live enumeration capability, and Native Loader installation is
  v5. Preserve live Store traversal, weak registry membership, GC ordering,
  Codegen clamping and existing ForeignAddress/Image/Tcc traversal bounds.
  Native consumers require rebuilding; image encoding is unchanged.

- Move canonical C-name export into Namespace (Design 81BF). IdApi v11 has
  30 fields / 240 bytes; native initialization carries matching live exports
  as its fifth argument. Namespace factory is v8, Float/Word preparation and
  collection publication contracts are v4, String preparation is v2, and Native
  Loader installation is v4. Preserve Integer's early export snapshot, other
  callers' live selection, incoming/saved API choices, roots and publication
  results. Native consumers require rebuilding; image encoding is unchanged.

- Move canonical C-name import into Namespace (Design 81BE).
  IdApi v10 has 31 fields / 248 bytes; native initializers carry an explicit live
  import capability as their fourth argument. Namespace factory becomes v7,
  construction/installers v3, and numeric/Block declarations v4. Preserve incoming
  and saved API selections, callback order, reentry and imported result identity.
  Native consumers require rebuilding; language behavior and image encoding are unchanged.

- Pass the selected caller's live import capability explicitly to Error Value
  (Design 81BD). Error Value ABI is v3; preserve late import selection, nested
  calls and imported result snapshots. IdApi v9 and image encoding are unchanged.

- Move interned publication into Namespace (Design 81BC), preserving selected
  raw binding, weak dictionary mirrors, Symbol identity and callback order.
  Namespace factory is v6; IdApi v9 and image encoding are unchanged.

- Move canonical export presence into Namespace (Design 81BB). IdApi v9 has
  32 fields / 256 bytes; native initializers receive an explicit live presence
  capability. Update Float/Word preparation, System/Block installers and numeric
  admission while preserving copied API selections, callback order and reload.
  Native consumers must be rebuilt. Image encoding and language behavior are unchanged.

- Pass each caller's live namespace presence capability explicitly to Error Value
  (Design 81BA). Native Error Value ABI is v2; preserve late callback selection,
  reentry and copied API contexts. IdApi fields and language behavior are unchanged.

- Move canonical interned lookup into Namespace (Design 81AZ), preserving absent
  name interning, selected storage, null bindings and independent runtime state.
  Namespace factory is v4; IdApi v8 and its live import/presence slots are unchanged.

- Move count/name-at into Namespace (Design 81AY), replacing private Id function
  exports with Namespace-owned descriptors. Reflect retains live selected-floor
  discovery, callback order and enumeration snapshots. Namespace factory is v3;
  IdApi v8 and image encoding remain unchanged.

- Extract reverse export-name lookup into Namespace (Design 81AX), including
  Image, Alien, Reflect and Portable Code consumers. Preserve live callback and
  context selection, first-alias lookup and borrowed names. IdApi v8 has 33 fields
  / 264 bytes; native consumers must be rebuilt. Run `mise run namespace-names`.

- Move raw export namespace storage out of Id into the loaded Namespace module
  (Design 81AW), preserving binding order, aliases, nulls, enumeration and roots.
  Native hosts now prepare its factory before Id initialization. Run `mise run namespace`.

- Extract System load: into Source Loader (Design 81AV), preserving source
  reading, staging, evaluation, diagnostics and callback order. Move its four
  semantic dependencies out of Prelude. Run `mise run source-loader`.
- Extract System loadModule: into Native Loader (Design 81AU), preserving
  direct loading, owned paths, live callbacks, original API/null state forwarding,
  receiver results and errors. Run `mise run native-loader`.
- Extract native Block whileTrue:/whileFalse: into Block Loop (Design 81AT),
  preserving fresh value sends, evaluation order, live Boolean/Error policy and
  later replacement by authored stackless control. Run `mise run block-loop`.
- Extract System readFile: and writeFile:contents: into File IO (Design 81AS),
  preserving binary data, the read limit, borrowed path callback order and lazy
  errors. Verify with `mise run file-io` and the focused file IO image smoke.
- Extract System getEnv: into Environment (Design 81AR), preserving live lookup,
  Unicode handling, missing values, lazy errors and callback order.

- Extract System readLine into System Input (Design 81AQ), preserving binary
  reads, buffering, EOF, lazy errors, callback locking and installation order.

- Use shared Error Value policy for Integer errors and iteration propagation
  (Design 81AP), preserving preparation, numeric admission and live callbacks.

- Use shared Error Value policy for Float recognition and construction
  (Design 81AO), preserving numeric behavior, preparation and live dispatch.

- Use shared Error Value policy for Word asInteger overflow (Design 81AN),
  preserving storage preparation, live callbacks and lazy Error dispatch.

- Move Prelude's shared Error construction/recognition helpers into
  `birnam-error-value` (Design 81AM). Preserve eager kind/message construction,
  lazy Error exports, factory/behavior snapshots and fresh dispatch.

- Move System print:/println: and their stdout writer into `birnam-system-output`
  (Design 81AL). Preserve registration snapshots, original execution callbacks,
  borrowed byte conversion, newline/flush order and ignored IO errors.

- Move Prelude's shared exact-String and printable-byte helpers into
  `birnam-text-view` (Design 81AK). Preserve borrowed pointers, callback timing,
  fresh printing lookup and path aliasing across later callbacks in all consumers.

- Move System gc/liveObjects/autoGc:/preciseGc: into `birnam-system-gc`
  (Design 81AJ). Preserve registration snapshots, original execution callbacks,
  live Boolean identities, count conversion and Integer construction after queries.

- Move Object printString/asString and Symbol printString into `birnam-printing`
  (Design 81AI). Preserve Prelude's registration order, original execution table,
  fresh lookup, borrowed Symbol-name copies and live String construction.

- Move String split: into the late `birnam-string-collection` adapter (Design
  81AH). Preserve byte snapshots, invalid/empty separator behavior, native Vector
  construction, callback timing and the original execution table across reentry.
  Collection retains the existing parent and installation order.

- Move Map's thirteen method bindings, traversal and printing into `birnam-map`
  (Design 81AG). Preserve registration order, live callbacks, null handling and
  native Vector iteration. Map v2 reduces the native boundary to construction
  and installation; Collection drops its unused Integer dependency.

- Extract Map representation, canonical objects and native HAMT storage into
  `birnam-map` (Design 81AF). Preserve construction/publication order, persistent
  sharing, collision handling, callback timing and reentrant update behavior.
  Existing Map methods and traversal helpers consume the native storage boundary.

- Move Vector's thirteen method bindings and scoped sorting roots into
  `birnam-vector` (Design 81AE). Preserve stable sorting, live dispatch, GC roots,
  registration order, Range's native comma callback and Map's native iteration.
  Vector v2 removes external count/at readers and exposes method installation
  plus the iteration bodies used directly by Map.
- Extract shared Vector/Map comparison and key hashing into
  `birnam-collection-value` (Design 81AD). Preserve identity shortcuts, numeric
  and byte semantics, hash bits and live callbacks; add no public selectors.

- Move Range reversal and concatenation into `birnam-range` (Design 81AC).
  Preserve bounds snapshots, allocation order and direct native Vector calls.
  Range construction v2 retains the existing Vector comma function and removes
  the unused external bounds reader; all eight Range methods now live together.

- Extract Vector storage, canonical objects and native trie helpers from
  Collection into `birnam-vector` (Design 81AB). Preserve allocation order,
  persistent sharing, layouts, adoption and reload behavior. Existing Vector
  methods and String/Map/Range consumers use the explicit native dependency.

- Move Range representation, canonical objects, layout, constructor and Integer
  `to:` into `birnam-range` (Design 81AA). Preserve creation/publication order,
  adoption, ordinary alloc callbacks and reload behavior through explicit
  construction stages. Collection's two Vector-backed methods use pure bounds
  access; IdApi and image encoding stay unchanged.

- Extract six Range methods from Collection into `birnam-range` (Design 81Z).
  An explicit construction dependency preserves registration order, publication
  reentry, selected Id callbacks and repeated initialization. Vector-producing
  methods and Range storage remain in Collection; IdApi/image formats stay unchanged.

- Extract StringBuilder's native buffer, cleanup and seven selectors from
  Collection into `birnam-string-builder` (Design 81Y). Preserve growth, byte
  truncation, empty clone/new, live callbacks and Vector-guarded publication.
  Load it after String and before Collection; IdApi and image encoding stay unchanged.

- Move String identity, representation, byte ownership, construction and cleanup
  into `birnam-string` (Design 81X). Migrate native consumers to an explicit
  dependency and remove three IdApi fields: v7 has 34 fields / 272 bytes.
  Preserve allocation/clone/finalizer schedules, byte borrowing, cleanup ownership,
  method reinitialization and image encoding. Native consumers require rebuilding.

- Extract fourteen String methods from Collection into `birnam-string`
  (Design 81W), with explicit numeric dependencies and its own printing-selector
  root. Preserve current byte/text/conversion behavior, live callbacks and
  method replacement. Splitting stays with Array storage; Id retains String
  representation and owned-byte cleanup. ABI/image formats remain unchanged.
- Extract Object/nil's five existing predicate and printing methods from
  Collection into `birnam-nil` (Design 81V), loaded immediately before Collection.
  Keep nil's identity and roots in Id, preserving inherited behavior, live
  callback/method replacement and existing ABI/image formats.
- Extract Boolean's existing methods and False's private-behavior setup from
  Prelude into `birnam-boolean` (Design 81U), loaded immediately before Prelude.
  Preserve branching, short-circuiting, inherited behavior and live replacement;
  keep singleton representation in Id and retain current ABI/image formats.
- Restore Float's pre-extraction repeated-initialization behavior: reinstall its
  seventeen methods while retaining canonical objects and namespace bindings.
  Later Prelude initialization continues to preserve live Float replacements.
- Move Word's canonical objects, payload and primitive operations into
  `birnam-word` (Design 81T), with explicit dependencies for Machine, TCC and
  Image. IdApi v6 removes four fields to 37 fields / 296 bytes; native modules
  require rebuilding. Keep Word conversion, unsigned values, image handling,
  live replacement and existing method reinitialization behavior.
- Extract Word's existing printing, equality and Integer conversion methods
  from Prelude into `birnam-word` (Design 81S). Load it after Float and preserve
  current numeric/error behavior and live method replacements. Word storage
  remains in Id; IdApi v5 and image formats stay unchanged.
- Move Float's canonical objects, payload and primitive operations into
  `birnam-float` (Design 81R), with explicit dependencies for all consumers.
  Prepare Float before Integer to preserve mixed numeric initialization. IdApi
  v5 drops four fields to 41 fields / 328 bytes; native modules require rebuilding.
  IEEE bits, live replacement, GC roots and image formats retain their contracts.
- Extract the seventeen existing Float methods from Prelude into `birnam-float`
  (Design 81Q), loaded after Integer. Arithmetic, conversions, errors and current
  method replacement retain their behavior. This first checkpoint leaves Float
  representation/constructors and their consumers in Id; IdApi/image bytes stay
  unchanged.

- Move production collection coordination into the loaded `birnam-collect`
  module (Design 81P): phase order, conditional deferred settlement, retirement
  before detachment, live finalizer invocation, reclamation and completion.
  Id retains storage/metadata owner operations; IdApi and image bytes are unchanged.

- Move actual GC traversal to the loaded tracer (Design 81O). Automatic, explicit
  and generation-retirement collections use its live worklist through ordered
  root, first-mark and outgoing-reference callbacks. Id's old `Marker` is removed;
  the host mounts the tracer explicitly before allocating native modules. Native
  root capture, current layout/hook semantics, weak retirement, deferred
  settlement and finalization are preserved. IdApi v4 and image bytes are unchanged.

- Promote the seed's graph tracer into the production `birnam-trace` module
  (Design 81N). It reports reached/not-reached allocation identities from scoped
  storage descriptions, rejects coverage gaps by default and preserves accepted
  omissions in diagnostic reports. Automatic GC marking and reclamation remain
  in Id. The host and installer inventories include the new native library.

- Extract the production storage-observation boundary (Design 81M): protected
  ordinary `withStorageView:` dispatch, runtime/allocation identities, requested
  and padded extents, shared core-root facts and explicit coverage gaps. A new
  `birnam-storage` native library validates owner-supplied descriptions without
  marking, settling deferred allocations or changing collection policy.

- Integer now owns its canonical objects, payload, construction, access and native
  methods in `libbirnam_integer.dylib`, loaded directly above Id. Native consumers
  acquire its explicit dependency; Prelude no longer installs Integer methods.
  IdApi v4 removes the four Integer fields (45 fields / 360 bytes); native
  consumers must be rebuilt. Method, Frame and image formats are unchanged.
  Recovery uses Integer's pure codec, and deferred Heap allocations hold opaque
  bytes. Arithmetic dispatch, fresh identity, Error propagation and the native
  constructor's collection opportunity are preserved.

### Added

- Structured tasks with automatic timeslicing, owned child tasks, joining,
  explicit success/failure/cancellation outcomes, and shielded cleanup.
- Process-backed multicore services: `Concurrency load:` prepares named source
  modules and their worker image; `program worker:` creates a serial service
  whose ordinary `handle:` method runs in a fresh task scope for each request.
  Worker requests return tasks, support cooperative remote cancellation, and
  provide graceful `shutdown` and `cancel` operations.
- [Concurrency guide](docs/guide/07-concurrency.md), searchable `birnam doc`
  entries for `concurrency`, `worker`, `taskControl`, and `taskExecutor`, and a
  multicore example runnable with `birnam examples/multicore/main.birnam`.

- Prelude can declare exact native integer semantics for guarded stackless calls.
  The floor checks current targets and native API context, preserves allocation
  and error behavior, and pins the declaring library. Design 74T also adds checked
  raw numeric-chain kernels and production multi-call integration.
- Design 74T adds checked identity-graph recovery through ordinary Id recipes,
  including exact numeric values and shared acyclic aggregates. Construction is
  rooted across GC and revalidates storage and recovery authority before atomic
  Frame publication. The four consumer families are integrated; Design 74AK
  records bounded qualification and the retained defaults. Unboxed arithmetic,
  loop reuse, closure/literal elision and counted indices remain opt-in.
- Design 74S adds bounded inherited-dispatch caching and guarded native Frame
  regions for local copies and call staging. Dependent native copies additionally
  use copy propagation, dead-store elimination and grouped stores. Ordinary target lookup, exact Id
  allocation order and observable Frame contents remain authoritative.

- Compiler-generated small-callee plans extend guarded inlining through local
  aliases (Design 74R). Entry recovery, exact index-box order, captured identity
  and GC/observation boundaries remain checked against canonical execution.
- Design 74P adds checked production recovery of a registered callee from an
  Id-described slot recipe, with atomic Frame/root and generation-pin transfer.
  Hot portable argument/capture projections inline their return under canonical
  Machine guards; observation or GC boundaries recover the original callee.
  Public ABI, descriptor encoding and image formats remain unchanged.
- V2 Frames support suspension and one-shot capture/restore through ordinary
  Machine operations (Design 60P). Resume commits the pending Call edge, including
  parallel copies, cleared slots and precise successor roots. Frame layout V1
  and raw Frame slots are removed; handwritten probes now use genuine V2 layouts.
  Public IdApi/Method ABIs and canonical descriptor bytes are unchanged.
- Design 74O's test-only feasibility experiments reconstruct an absent callee
  using production compiler layouts, consume an Id-authored recovery recipe,
  and test identity, native observation, GC and continuation boundaries.
  Debug/release and full verification pass. V2 continuation storage works in
  the bounded probe; the publication gap found there is resolved by 60P.
  No general reconstruction admission or performance benefit is claimed.
- Design 74N repeats isolated native-tier testing after dependency revalidation.
  Eighteen paired cases and increased native coverage pass; seven controlled
  timing groups find no independent speedup beyond variation and measure about
  0.30–0.35 ms extra warmup cost in both chain sizes.
- Design 74M's focused performance runner isolates native-tier execution,
  compilation and memory costs from dispatch changes. Seven controlled pairs
  find no independent speedup beyond variation; separate diagnostics identify
  unused cold preparation and invalidation caused by unrelated symbol interning.
- Eligible portable Blocks can use runtime-owned native PC entries and recover
  through their original active Frame after invalidation (Design 74K).
  Ordinary staging and registration remain unchanged. Native code stays alive
  through GC, nested calls, active replacement and cache eviction.
  `mise run native-tier-test` checks recovery and guarded lookup in debug/release.
- `mise run deopt-api-experiment` checks Design 74I's guard substitutes through
  the frozen IdApi against ordinary floor builds. Native mutation and custom
  lookup counterexamples identify the guard-authority boundary for promotion.
- `mise run read-reuse-experiment` runs Design 74H's development-only guarded
  Frame-read reuse experiment. Debug/release cases include GC callbacks that
  mutate cached slots without dictionary changes. Seven controlled timing
  pairs were inconclusive, so the experiment is not integrated into normal
  compilation. Runtime ABI and image contracts remain unchanged.
- `mise run generated-deopt-experiment` runs Design 74G: compiler-generated
  per-PC entries recover to their original baseline through the real driver.
  The test checks colored Frame state, precise GC, invalidation and actual
  native-library retirement in debug and release. This experiment retains its
  test-only selector; Design 74K separately admits eligible portable programs
  in ordinary execution, with the existing ABI and image contract preserved.
- `mise run deopt-experiment` runs Design 74F's test-only same-frame recovery
  experiment in debug and release. The real Runtime driver switches from a
  static optimized entry to its original baseline on invalidation, preserving
  the active Frame, values, aliases and effects. The experiment adds no runtime
  exports and changes no image ABI or Frame layout.
- Design 74 E1 (development-only): a `dev-guarded-slots` cargo feature on the
  floor, TCC rung and emitter adds guarded direct Machine slot access for
  generated StepMethods, selected per compiling process by
  `BIRNAM_DEV_GUARDED_SLOTS=1`, with a resumable loop fixture and driver.
  These development shims remain absent from ordinary builds; the runtime ABI
  hash and export surfaces are untouched. Every vtable now carries a
  dictionary-mutation epoch.
- Development-only slot-index box elision on the guarded path (Design 74D,
  `BIRNAM_DEV_ELIDE_SLOT_BOX=1` on top of the E1 selection): the floor
  answers slot accesses from an unboxed index on a guard hit and performs
  the ordinary send itself, with the store value rooted, on a miss. Measured
  at about half the baseline loop time on the Design 74 fixture.

### Changed

- IdApi v3 removes native owner/parent accessors; consumers use replaceable
  Vtable messages. The existing owner GC edge and parent mutation behavior are
  preserved. Native consumers must rebuild against the 49-field table. See
  [Design 81K](docs/design/81k-owner-and-parent-protocols.md).

- Validation now has explicit routine, exhaustive, release and performance plans,
  resumable per-step evidence and configuration-specific development build reuse.
  CI shares builds across disjoint matrix workers; cold release builds remain
  independent. Historical measurement payloads are archived losslessly, and
  [the validation guide](docs/validation.md) replaces temporary controller instructions.
- Ordinary sends guard canonical Machine slot access and default lookup inside
  the runtime (Designs 74J/74L). Current methods, inheritance, custom lookup and
  native/resumable overrides retain their effects. IdApi, exported functions,
  primitive semantics and Frame/image formats remain unchanged. Seven controlled
  groups qualify a 17.3% combined improvement on the smaller warm portable
  fixture, with no above-spread regression; other rows and separate PC/lookup
  gains are inconclusive. The native compiler adds about 603 KB to the floor.
- Release acceptance for v0.15 judges micro-layer comparator flags by their
  raw per-operation medians (Design 60HC): a flag is accepted when raw time is
  within the 10% micro allowance or moved by less than the case's own
  three-process spread, because v0.15 halved the subtracted empty-loop
  baseline and two clean candidates flagged disjoint micro cases with no raw
  regression. Every other layer keeps the ordinary comparator threshold;
  v0.16 returns to the ordinary comparison.

- CodeProgram objects retain checked immutable layouts, so Block construction
  and reflection reuse validated views. Global imports and method dictionaries
  use derived identity indexes while preserving ordered enumeration, current
  binding behavior and weak-key reclamation. Host-gated timing acceptance
  closed on 2026-09-09 (`docs/measurements/as7-acceptance-comparison.json`):
  Block construction and CodeObject reflection measurably cheaper, the global
  and method indexes timing-neutral at probe scale, docgen 39.6 to 29.4 s.
- Rust ordinary/portable C emission shares byte escaping, native providers and
  consumers share private ABI carriers, and data-only receipt validators share
  mechanical checks while retaining their schemas, diagnostics and policies.
- Rust host and Codegen share the existing SHA-256 implementation. Artifact
  formats share scalar byte helpers while retaining their checked/unchecked
  offset behavior, independent validation policies, and provider-free bootstrap.
- Store snapshots use transient ordered pair indexes instead of repeated
  whole-log scans. PEG indexes rule names and copies visited pattern data once
  per parse, preserving mutable grammars, memo identity and parse diagnostics.
- Query matching borrows candidate values and validates constraints before
  copying bindings. Rejected candidates allocate nothing; successful triples
  avoid temporary value copies while preserving query ordering and equality.
- Weak Symbol lookup now uses a collision-safe name index while preserving
  reclamation, enumeration order, and stable live-name pointers. Generated
  vtable caches are indexed by generation and slot, and heap allocation metadata
  no longer stores a redundant payload pointer. Design 66 records development
  throughput evidence and the remaining allocation investigation.
- Development benchmarks can opt into the existing host-load gates with
  `BENCH_CHECK_HOST=1`; release benchmarks continue to require those gates.
- Project builds publish self-describing application, configuration, and module
  roots instead of `artifacts.toml`. Ordinary `run` and `install` consume the
  last successful publication without reading authored sources or parsing the
  manifest; `--build` explicitly refreshes it. Tests load the published module
  root without executing or requiring a valid application root.
- Project build output reports reused modules. Missing or malformed cached
  artifacts rebuild safely, and failed builds preserve the previous publication.
- Ordinary project launch preflights its selected closure once. A nine-module
  Git/native fixture measured 1.228 s before and 0.826 s after this change
  (five warm launches, release runtime; diagnostic evidence in Design 65).

### Fixed

- Cell image recovery now relocates its store, entity, and attribute references
  instead of preserving stale native addresses. The payload ABI and image
  format are unchanged. Images previously saved with raw Cell addresses must
  be recreated from source.

- JIT generation keys no longer equal Unit object addresses, preventing pending
  retirement metadata on the native stack from retaining an otherwise dead Unit.
  Production recovery integration exposed this GC lifetime issue (Design 74P).
- Native-tier admission revalidates its actual Machine/lookup dependencies after
  dictionary mutation (Design 74N). Unrelated symbols, exports and ordinary method
  replacement retain valid native entries; real guard changes still resume the
  original Frame through its baseline. IdApi and language semantics are unchanged.
- Stdlib boot regained the ~10 ms that the PEG pattern-snapshot tables and
  per-vtable method indexes had added: parse-local PEG tables now hash
  integer keys with an address hasher, and method dictionaries of at most 16
  keys scan instead of maintaining a hash index. Host-gated against the
  pre-Design-68 baseline: dylib boot 107 to 86 ms, source boot 271 to 242 ms,
  `parse.json-20kb` 362 to 310 ms, docgen 39.6 to 29.4 s. The earlier
  dylib-startup RSS growth did not reproduce (65.5 MB on every run).

## [0.14.0] — 2026-08-27

### Added

- **Stackless native execution is now the production runtime path.** Birnam
  lowers resumable methods and blocks into explicit Machines, Frames, and
  StepMethods, preserving ordinary dispatch, inheritance, DNU, late binding,
  callbacks, and tail-call behavior without growing the native stack.
- **Portable CodeObjects preserve executable meaning across processes and
  images.** Programs, lexical bindings, statements, captures, and source maps
  are encoded canonically, restored through image v8, and rematerialized into
  disposable native code without making process-local addresses durable.
- **One-shot continuations and resumable lifecycle controls.** Host suspension,
  restoration, code-binding retention, unload/retirement fencing, bounded
  history compaction, and image recovery now share explicit ownership rules and
  production acceptance coverage.

### Changed

- **Release acceptance now proves the shipped archive itself.** The release
  gate installs the immutable candidate, exercises exact-current rejection,
  image and resumable lifecycles, long-running stack-flat control, bounded
  churn, performance comparison, and longevity, then publishes the receipt and
  complete evidence bundle beside the tarball.
- The image carrier advances to v8 to preserve boxed identity and foreign
  addresses while retaining portable executable meaning.

### Fixed

- Release promotion now replaces the consumed `## [Unreleased]` heading with a
  fresh one, so the next release does not fail merely because the previous
  release successfully promoted its notes.
- Restored image entry and baked-image boot behavior on the Rust host, including
  exact-current checks and stackless registration after recovery.
- The Rust TCC dylib audit covers the stackless export seams used by installed
  and relocated release bundles.

### Performance

- This first complete stackless release explicitly accepts substantial
  throughput regressions in exchange for bounded native-stack use, resumable
  execution, portable CodeObjects, image-v8 recovery, and reclaimable live-code
  lifecycles. Against the newest retained production Rust snapshot,
  `sends.unary` is 19.6% slower, `loops.to-do` is about 237x slower,
  `query.single-clause` is about 456x slower, and full documentation generation
  is about 30x slower. The complete decision, case boundary, and release-only
  ceilings are recorded in Design 60HB; later releases return to the ordinary
  performance policy.

## [0.13.0] — 2026-08-04

### Added

- **`mise run test` runs the birnam-level module suites** (`scripts/birnam-test.sh`
  → `modules/run-tests.birnam`) against a freshly built prefix. `mise run verify`
  gates the Rust workspace and the build driver but never loads a `.birnam` test
  file, so the birnam suites had no task of their own and had to be invoked by
  hand — easy to skip when checking a change that only touches birnam source.

### Changed

- **The statement separator `;` is now optional.** Whitespace alone separates
  statements. Every expression form is self-delimiting — a bracketed group
  (`[..]`, `#[..]`, `{..}`) or a single token — and nothing juxtaposes at
  statement level, so `;` carried no information the grammar did not already
  have. `[a foo]; [b bar]` and `[a foo] [b bar]` are one program, and the two
  spellings may be mixed. Existing source is unaffected; the new suite
  `modules/lang-test.birnam` pins the equivalence.

### Fixed

- **A binary operator can be defined at runtime.** The trampoline that backs
  `Reflect defineMethod:on:as:` outside an AOT artifact derived its arity by
  counting colons in the selector. A binary operator has none, so `#|>` and
  friends were compiled as unary and handed their two-parameter block a
  `value:`, answering `#doesNotUnderstand` instead of running. The module
  already had the correct rule in `selector_arity`; the trampoline just
  open-coded its own. Only the from-source path was affected, which is why
  stdlib's own `|>` worked while an override of it could not.
- **Validation compiles through libtcc are serialized.** libtcc carries
  process-global state across `tcc_new`/`tcc_relocate`, so the two
  `birnam-alien` tests that compile shims raced when cargo ran them in parallel
  threads and killed the test process with SIGSEGV — about one run in four,
  under `cargo test --workspace` and therefore under `mise run verify`. The
  lock lives in `compile_source_for_validation` so no caller has to remember.
- **An unbound selector now errors at every arity.** Dispatch forwards an
  unbound send to `doesNotUnderstand:` as `[selector, arg0, …]`, so the handler
  receives `1 + argc` arguments; the base handler guarded on exactly 1, which is
  true only of a unary send. Every keyword and binary send to an unbound
  selector therefore answered `nil` instead of a kinded `#doesNotUnderstand`
  error, so a renamed or mistyped selector read as a silent `nil` at the call
  site. `modules/stdlib-test.birnam` now pins the behaviour at unary, one- and
  four-keyword, and binary arity.
- **`modules/cli-test.birnam` calls the current CLI entry point.** It still sent
  `run:source:rustWorkspace:guides:`; the `source:` keyword named the Zig source
  directory and went away with the Zig tree. Four cases had been failing since
  then — masked as `got=nil` by the `doesNotUnderstand:` bug above.
- **Emacs `birnam-mode` project commands** (0.4.0): `birnam-run-project` and
  `birnam-test-project` now pass `--build`, so they no longer fail on a stale
  or absent artifact manifest; `birnam-run-project` also passes `--no-repl`,
  because a compilation buffer has no terminal to host a project whose
  manifest sets `run.repl = "tty"`.
- **Emacs lsp-mode startup is deferred.** `birnam-lsp-start` now goes through
  `lsp-deferred` when lsp-mode provides it, so a `.birnam` file opened
  invisibly by a grep or project search no longer spawns a language server
  from the major-mode hook. Project rooting moved from a `let'-bound
  `default-directory` — which could not survive deferral — to a buffer-local
  `lsp-auto-guess-root` resolved through the `project.el` integration
  `birnam-mode` already installs.
- `birnam-mode.el` byte-compiles without warnings: the lazily loaded lsp-mode
  functions are declared, and `lsp-language-id-configuration` is declared
  special without a value so this file never shadows lsp-mode's default.

### Added

- `mise run emacs-test` runs the `birnam-mode` ERT suite
  (`scripts/emacs-test.sh`), which previously had no task wiring.

## [0.12.0] — 2026-07-29

### Changed

- **Rust is now Birnam's only implementation and build authority.** The Zig
  runtime, compiler integration, build graph, probes, and compatibility
  selection paths were removed.
- The installer, release bundle, CI, project tools, wrapped-library docs, and
  stage1 modules now use the Rust host exclusively.
- The native source documentation module is now named `nativedoc`.

## [0.11.0] — 2026-07-15

### Added
- **Project-aware language tools.** The LSP now indexes authored source across
  resolver-selected dependencies for navigation, references, hover, scoped
  completion, and signature help, with UTF-16 and document lifecycle coverage.
- **Emacs lsp-mode integration.** `birnam-mode` now discovers `birnam.toml`,
  starts `birnam --lsp`, roots the REPL at the project, and exposes project
  check/build/run commands with integration tests.

## [0.10.0] — 2026-07-14

### Added
- **Blocking process isolates and live-worker rollout.** Trusted workers now
  support copied-value request boundaries, deadlines, structured failures,
  health-checked single-worker replacement, and explicit cleanup ownership.
  Mailbox-safe live evolution carries checksum-bound activation and rollback
  controls with independent evolution receipts.
- **File-backed live module baselines.** Live module revisions can be exported,
  inspected, staged inertly, activated at the host boundary, rolled back while
  locally valid, and saved as guarded baselines that behave consistently under
  mapped boot and source rebuild.
- **Installed documentation and developer tools.** Installs and release bundles
  now carry the birnam module sources, Zig-floor documentation sources, and the
  authored guide under `share/birnam`; `doc`, `docgen`, `--lsp`, and `--listen`
  work from an installed tree without a source checkout. The complete
  verification gate includes an isolated-install smoke test.
- **User guide + self-hosted Markdown.** The generated HTML reference now opens
  with chapters covering the language, idioms, fact store, and images, rendered
  by a deliberately small Markdown module written in birnam.
- **Project-root configuration.** `birnam.toml` is discovered from cwd upward;
  `main` resolves relative to the discovered root, and `load = ["module", …]`
  loads compiled modules dependency-first before the entry point and REPL.

## [0.9.0] — 2026-07-12

The measurement release: the benchmark infrastructure landed, and what it
measured got fixed. The relational read path went from seconds-per-query to
milliseconds (the index arc), value equality became one story everywhere the
system compares values, and the first stdlib migrations gated by measurement
shipped.

### Added
- **Benchmark infrastructure (docs/46).** `[Time monotonicMicros]` and a
  calibrated, GC-disciplined harness (`modules/bench.birnam`); the micro
  suite (28 migration-gate cases plus a process-isolated parse suite); the
  relational suite over a fixed 10k-entity base that **gates on expected
  cardinalities** — it refuses to pass on wrong answers; macro whole-process
  cases (boot paths, image save/boot, stage1 compile, scripts, docgen, GC)
  via `scripts/bench.sh`; `bench-compare` with regression thresholds; mise
  tasks and `docs/misc/BENCHMARKS.md`. Tracked snapshots live in
  `bench/results/v<VERSION>.json`.

### Changed
- **Collection tier-B boundary, settled by measurement.** `includes:` and
  `asVector` moved to stdlib birnam (`includes:` is a one-send delegation to
  the retained native `indexOf:` — the fully-derived form measured 116×);
  `reversed` and `copyFrom:to:` stay native (2.58× / 1.60× — per-row loops
  fail the gate, as docs/44 §7 predicted). The whole boundary surface is
  pinned in stdlib-test either way.

### Fixed
- **Floats compare by value everywhere the system compares values.** Map
  keys, `includes:`/`indexOf:`, store facts (set semantics + retract by an
  equal-valued box), and datalog bindings — `[#[1.5 2.5] includes: 1.5]` is
  true now. Per-type like `=` (`1.0 = 1` stays false); `-0.0` and `0.0` are
  one Map key.
- **Datalog pattern constants match by value.** `#[?e #age 33]` matches an
  asserted 33 — previously number/float/string constants in triple patterns
  compared by pointer identity and silently matched nothing. Join agreement,
  rule args, and derived-tuple dedup ride the same value equality, so the
  engine now compares values one way everywhere.

### Performance
- **The index arc — the relational read path is ~quadratic no more.** The
  store gained an (e,a) postings index (an access path only: the append-only
  log stays canonical and is what serializes; the index rebuilds from it),
  the engine partitions gathered triples by attribute lazily per query,
  derived-tuple membership is hashed, and the join copies bindings only on
  match. Measured over the ~23k-fact bench base: single-pattern query
  **3.6 s → 3 ms (~1200×)**, nested pull **10.9 s → 8 ms (~1400×)**,
  recursive-rule query **4.3 s → 39 ms (~110×)**, 3-way join
  **4.7 s → 80 ms (~59×)**, asserts **102 µs → 2.7 µs/op (~44×)**. Point
  reads speed up system-wide — compiled global references ride the same
  store.

## [0.8.0] — 2026-07-11

The refounding. The legacy fact-store kernel is retired and birnam is rebuilt
from a pure Piumarta object floor (`src/id/`) up: the whole grammar compiles
to native code through the in-process TinyCC backend, the object model is
prototype-based, the relational reading is a datalog engine over the store,
and a growing share of the system — stdlib, config, CLI, REPL, module
manager — is written in birnam itself. Effectively a new system; the notes
below describe the new world rather than diff the old one.

### Added
- **The id floor and the native pipeline.** siphon → libid → store → peg →
  lang → tcc → codegen: every birnam form compiles to native code via the
  vendored TinyCC; blocks compile to native closures. FFI runs both
  directions — struct-by-value out, C-calls-birnam callbacks back in.
- **Prototype object model.** Every well-known name is a prototype owning its
  family vtable; creation is one verb `clone` (`new` aliases it) with
  copy-on-write vtables; no immediates — Integer is a boxed full i64, joined
  by Float and a raw 64-bit `Word` (with peek/poke/alloc/free for C memory).
  Native `whileTrue:`/`whileFalse:` on a real Block family, short-circuit
  `and:`/`or:`, structural equality across collections, `timesRepeat:`.
- **Result-style errors.** Errors are kinded propagating values (`kind`,
  `isError`, `message`) with `on:do:` (multi-kind) and `ensure:`; parse
  failures, FFI failures, and grammar left recursion are kinded errors
  instead of crashes or silent nil.
- **Collections + literals.** HAMT-backed Map, persistent Vector, Range, and
  String share one sequence protocol over a single native fold; `#[…]` vector
  and `{…}` map literals; `StringBuilder`; the `|>` pipeline operator.
- **The relational reading.** A datalog engine over the store:
  `[db findAll: ?v where: …]` with multi-variable joins, predicate filters,
  negation, disjunction, recursive rules (semi-naive, mutual recursion;
  durable as relational data), aggregates + group-by, time travel (`asOf:`,
  `since:`/`between:`), and pull. Writes: `[db entity]`,
  `[db assert: e attr: #a value: v]`, retraction.
- **Garbage collection.** A self-contained mark-sweep collector with
  automatic threshold collection, per-vtable trace hooks, and an exact-heap
  precise mode.
- **Images.** Save and boot the live system: relocatable AOT compiled units,
  full heap-graph serialization, incremental append + compact, and a
  self-booting executable bake. Compiled-unit-first integrity (format v3):
  every embedded dylib is checksummed and verified before load changes live
  state; full saves publish atomically; unsupported durable values reject the
  save with a structured `[System imageSaveReport]`. Doc'd stdlib methods,
  module singletons, Alien-wrapped C libraries, and runtime-loaded modules
  all survive save/boot.
- **Module system.** birnam-source modules compile to installable dylibs
  (`birnam --compile-module`, `zig build stage1`), with eval-from-source as
  the permanent floor; the module manager is itself a birnam module —
  dependencies are store facts, load order is a topological sort, cycles
  answer `#circularDependency`. Runtime loading via `[Module load: #name]`
  (deps first, idempotent). `birnam.toml` is parsed by the birnam-native
  `Toml`; `birnam init` scaffolds a project.
- **The self-hosted layer.** stdlib, toml, config, cli, and repl are birnam
  modules: the string protocol, the error protocol, the CLI subcommands, the
  console REPL session loop (`[Repl console]`), and the module manager are
  birnam code. The Zig floor keeps the per-byte scanners — the measured send
  cost (~1.5–2 µs) gates what migrates.
- **Alien — C libraries without new Zig.** `[Alien dlopen:]` +
  `[Alien wrap:signature:as:on:]` wrap any C library at runtime;
  `birnam wrap <name>` scaffolds a wrapper module; termbox2 is wrapped (the
  `logue` roguelike renders through it). Opt-in release paths for callbacks
  and dlopen handles.
- **OS + app modules, written in birnam over libc.** Time, File, Process,
  Log, Random, Socket (TCP), Http (a static HTTP/1.1 server), Json, Html,
  and Test (a live, Smalltalk-style test system).
- **Docs + tooling.** Docstrings ride definitions (`defineMethod:…doc:`) and
  land in the store; `birnam doc <module> [selector]` and `birnam docgen`
  (HTML docs covering birnam modules and the Zig floor); an LSP written in
  birnam (`birnam --lsp` — diagnostics, hover with docstrings); an
  out-of-process socket REPL (`birnam --listen`); Emacs `birnam-mode`
  refreshed for the current language.
- **One verification gate:** `zig build verify` runs the native test suite,
  the self-hosted stage-one rebuild, the birnam-level suites, and the REPL
  goldens; it is enforced by GitHub Actions and before release tagging.
- **Explicit platform boundary:** arm64 macOS is the sole supported build and
  release target. Unsupported targets now fail before compilation instead of
  producing an incomplete or non-bootable artifact. See
  `docs/misc/PLATFORM-SUPPORT.md` for the porting requirements.

### Changed
- **README rewritten for the active system:** current installation, REPL,
  script, image, tool, verification, and support guidance now replaces the
  retired kernel and command documentation.

### Removed
- **The legacy fact-store kernel** (retired to `src/attic/`) and, with it, its
  unreleased post-0.7.4 improvements (`--boot-image`, code-fact storage
  tiers, file-in pruning) — the id system supersedes them.
- The legacy configuration surface: the `$BIRNAM_PATH`/`$BIRNAM_DYLIB_PATH`
  search tiers, `[[wrapped]]` cc-built wrappers, and `[module_overrides]`.
  The live environment variables are `BIRNAM_LIB_DIR`, `BIRNAM_ID_DYLIB`,
  `BIRNAM_NO_STDLIB`, and `BIRNAM_GC_PRECISE`.
- The `--demo` flag.

## [0.7.4] — 2026-06-22

Release-process only: cutting a release now requires a CHANGELOG entry for
the version, and that section becomes the GitHub release notes.

## [0.7.3] — 2026-06-19

### Fixed
- **`:prune`/`:gc` no longer kill the linenoise REPL.** The linenoise REPL was
  birnam-owned (a `whileTrue` loop in `repl.birnam`), so its loop frame never
  unwound — there was no quiescent point to sweep, and a `:prune` mid-loop
  reclaimed the running loop's own blocks and hung the REPL.

### Changed
- **Shell owns the REPL loop; linenoise is just the line reader.** The shell
  now drives the loop and calls `[linenoise prompt:]` for input. Both the
  built-in and linenoise loops run one shared line-processor, so `:prune`/`:gc`
  **and** auto-prune/auto-gc run at the quiescent boundary in the linenoise
  REPL exactly as in the built-in one — and a long session stays memory-bounded.
  The old `repl.birnam` driver is removed. (Belt-and-suspenders: `:prune`/`:gc`
  invoked via `[language command:]` mid-loop refuse rather than crash.)

## [0.7.2] — 2026-06-18

### Changed
- **`Module load:` resolves bare names through the module search path** — the
  "known place" (`<exe>/../lib`, `$BIRNAM_DYLIB_PATH`, config `dylib_paths`, …),
  like first-party modules. A path with `/` is still used verbatim. So wrapped
  C libs load by bare name regardless of cwd; `install`/`dist` stage wrapped
  dylibs into `lib/`. Supersedes the 0.7.1 chdir workaround.
- Our own `birnam.toml` uses the `[module_overrides]` table for `repl`.

## [0.7.1] — 2026-06-18

### Fixed
- linenoise REPL failed to start from a subdirectory (wrapper's relative dylib
  path didn't resolve). (Superseded by 0.7.2's search-path resolution.)

## [0.7.0] — 2026-06-18

### Added
- **Networking — `Net`** (TCP over zig's `std.net`/`std.posix`, no FFI):
  `Net connect:port:`, `Net listen:`, socket `send:`/`recv:`/`close`, server
  `accept`. Default-loaded.
- **stdlib modules, birnam-authored** (default-loaded via one `stdlib` dylib):
  **Test** (`describe:do:`/`it:do:`/`summary` — xUnit-lite), **Stats**
  (`mean:`/`median:`/`stddev:`/…), **Csv** (`parse:`/`rows:`/`readFile:`/
  `write:to:`), **Log** (`info:`/`warn:`/`error:`/`level:`). Each carries its
  own Test-based tests (`stdlib-test.birnam`, 18/18).
- `IdApi.bind_global` so a zig module can expose a global without filing in
  birnam.

## [0.6.0] — 2026-06-18

### Added
- **stdlib surface (native):** Math `abs`/`min:`/`max:`/`pow:`; a `Random`
  prototype (`seed:`/`next`/`nextInt:`/`between:and:`); a `Time` prototype
  (`now`/`millis`/`sleep:`); OS on `System` (`getEnv:`/`setEnv:to:`/`exit:`/
  `run:`); String-utils (`repeat:`/`padLeft:`/`padRight:`/`center:`/
  `capitalize`/`titleCase`); string `asFloat`.
- **JSON + YAML parsers (PEG-backed).** `Json parse:`/`stringify:` and
  `Yaml parse:` (flat subset). JSON is **authored in birnam** (`json.birnam`
  embedded in a thin dylib) and default-loaded.
- **Birnam-authored parsers over PEG:** `peg` bound as a global with
  combinators (`seqOf:`/`choiceOf:`/`anyOf:`/`range:to:`/…); parse trees are
  navigable from birnam (`ruleName`/`children`/`start`/`end`). Plus
  `Object new` (prototype creation), a general `vtable` accessor, and type
  predicates (`isString`/`isNumber`/`isVector`/`isMap`).
- **Symbol value protocol** — symbols answer `=`/`printString`/`asString`/
  `asSymbol`.
- **Generalized module overrides** — `[module_overrides]` toml table +
  `BIRNAM_MODULE_OVERRIDES` env + `--overrides` flag (precedence argv > env >
  toml > default). `repl = "…"` is sugar for the `repl` slot.
- `birnam --help` / `--version`.

## [0.5.0] — 2026-06-18

### Added
- **Datalog write path:** `[db entity]` mints an entity; `[db assert: e attr:
  #a value: v]` asserts a fact — the birnam-level writes the query/pull surface
  was missing.
- Examples: `address-book` (datalog facts/query/pull), `fizzbuzz`, `primes`,
  `bank-ledger`.

### Changed
- linenoise REPL driver rewritten in a flat, named-block style.

## [0.4.0] — 2026-06-18

### Added
- **LSP P2:** `hover` (token/selector/arity) + `signatureHelp` (keyword-send
  parameters), lexical, no re-parse.

## [0.3.0] — 2026-06-18

### Added
- **Emacs `birnam-mode`** (`tools/emacs/`): font-lock, indentation, comint
  REPL, and lsp-mode + eglot client registration.
- **`birnam --lsp`** — a Language Server over stdio. **P0:** parse diagnostics.
  **P1:** `documentSymbol` (top-level `let`s) + prefix-filtered `completion`
  (keyword floor + interned selectors).
- **Wrapped first-party C libs** — `[[wrapped]]` in `birnam.toml` + `birnam
  wrap` (builds via `cc`, no zig); linenoise wrapped. An opt-in
  birnam-authored linenoise REPL (`repl = "linenoise"`).
- Width-complete FFI peek/poke, integer bitwise/shift, negative literals.
- Example: word-frequency counter.

## [0.2.0] — 2026-06-17

Onboarding ergonomics: get from "I have the binary" to exploring without a
manual.

### Added
- **Startup banner** — `birnam <version> — type :help for commands. Evaluate
  with brackets: [3 + 4]`. The empty prompt now teaches itself. Version is
  embedded from `VERSION` at build time.
- **`:help`** (`:h`, `?`) — REPL command reference covering both the
  shell-handled commands (`:in`/`:save`/`:load`/`:prune`/`:gc`/…) and the
  lang-handled ones (`:reset`/`:eval-mode`).
- **Script + eval run path** — any `*.birnam` argument is filed in, `-e EXPR`
  evaluates one expression; both run after the modules load and then exit.
  `-i` keeps the REPL open afterward, so a script's definitions carry into the
  live session ("load defs, then explore").
- **`birnam init`** — scaffold a project: writes `birnam.toml` and a starter
  `playground.birnam` in the current directory, never clobbering existing
  files.

### Notes
- Files separate statements with `;` (the REPL is line-based; files are not).
  The scaffolded `playground.birnam` follows this.

## [0.1.0] — 2026-06-17

First release. Made the shell pleasant to invoke and the install zero-config,
and added the tooling to cut binaries.

### Added
- **Bare-name module loading** — `birnam peg lang` instead of full dylib
  paths; names resolve along a search path (`lang` → `liblang.dylib` →
  `libbirnam_lang.dylib`). With no arguments, the default set (`peg lang`)
  loads.
- **`birnam.toml`** project config — discovered cwd-upward (git-style). Pins
  the module set (`modules`), extra search dirs (`dylib_paths`), and a snapshot
  to auto-load (`image`). A tiny TOML subset.
- **`$BIRNAM_PATH`** — Go-style `:`-separated install roots, each probed at
  `<root>/lib` then `<root>` (complements `$BIRNAM_DYLIB_PATH`, which lists
  exact dylib dirs).
- **Zero-config install** — the shell probes `<exe>/../lib`, so a
  `<prefix>/bin/birnam` + `<prefix>/lib/` layout resolves its modules with no
  environment variable set.
- **Release tooling** — `mise.toml` (pins Zig 0.15.1; `build`/`install`/`dist`/
  `release` tasks), `scripts/dist.sh` (build → stage → ad-hoc sign → tar a
  portable bundle), `scripts/release.sh patch|minor|major` (bump → tag → build
  → publish a GitHub release).
- **`--bare`** flag — load no modules (bare kernel).
- README "Get started in 60 seconds".

### Changed
- Install footprint is now exactly `bin/birnam` + `lib/libbirnam_*.dylib`;
  `libffi.a` (a build-time static archive) is no longer installed.

### Fixed
- The shell exits cleanly instead of dumping thousands of bogus
  GeneralPurposeAllocator "leaks" — its kernel/interner/module state is
  process-lifetime by design.

[0.2.0]: https://github.com/chazu/birnam/releases/tag/v0.2.0
[0.1.0]: https://github.com/chazu/birnam/releases/tag/v0.1.0
