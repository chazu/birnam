# Design 83 implementation history

This preserves the migration records from
[Design 83](83-production-id-construction-and-lifetimes.md). Records describe the
state at the time they were written; later entries supersede earlier statements
about production wiring and remaining work. For the current status and acceptance
gaps, use [the implementation status](83-production-id-construction-and-lifetimes.md#implementation-status).
The design's normative requirements remain unchanged.

## Implementation history

The following records describe intermediate migration states. Statements about
remaining construction work in older records are superseded by the status at the
top of this document and the final application-construction record below. The
normative requirements in sections 1–6 still apply in full.

### Implementation evidence and remaining boundary

**Production migration is in progress. Id is now independently packaged.**
The production Id crate depends only on `birnam-abi`. Its dylib exports the native
module descriptor and identity; Runtime and compiler/artifact code live in the
separate `birnam-runtime` crate. Both are installed and the host initializes
Runtime through Id's loader after its predecessors. Compiler admission is an
explicit later extension; base Execution has its value dependencies at construction.
Language reflection now returns owned Methods through the checked language-send
entry used by TCC and Portable Code. Unsafe native consumers still use the frozen
raw Method convention. Registered generated implementations retain their
generation; native functions retain their image. Anonymous synchronous and
resumable TCC functions retain their owning Unit. Bindings retain that same Unit
through Lifetime v2 native references, without permanently rooting unreachable
cycles. Native consumer migration and the complete construction graph remain
unfinished. See [the language boundary](../../abi/language-send-v1.md).

The Method production check is `scripts/method-values-smoke.sh PREFIX`, also
included as `method-values` in routine/exhaustive validation. It passes source,
source-free compiled modules, mapped/rebuilt images and baked execution, with
replacement, removal, aliases, custom lookup, malformed lookup results and GC.
Both ordinary and privately customized Method instances refuse image saving.
The native construction-prefix fixture also retains distinct native contexts,
invokes after replacement/removal, and collects during invocation before the
compiler extension exists. The focused Runtime/TCC/Portable Code tests pass
324 tests (17 ignored); the installed Portable Code suite passes 114 assertions.

The synchronous portable-code eviction assertion formerly required unmapping
inside the enclosing turn; it also failed on the preceding committed build.
It now asserts deferral like the resumable cases. The existing
`scripts/portable-resumable-lifecycle-smoke.sh` checks both synchronous and
resumable materializations across separate real REPL turns, proving eviction
survival and actual subsequent native-unit release. Debug installation and the
workspace diagnostic build pass; release validation remains waived.

The host's [`Siphon`](../../rust/birnam-host/src/siphon.rs) supplies the small ad
hoc loading protocol and invokes `birnam_native_module_v1`. The real Id descriptor
constructs `InitialObjects` using only Id allocation, dictionaries and Symbols;
its own bind/invoke path then installs native loading, construction and storage
operations. Core Id dispatch has no Runtime, GC, Execution or future-provider
slot. Native descriptor admission closes rejected images before entry and pins
entered images, including failed initializers with partial publication.

Upper startup uses the same objects and carrier. Id keeps its registries; each
registry has one owner and a stable address. No graph is copied and no carrier
slot is retargeted. Six complete supporting factories now load through Id from Method Observation,
Storage and Dispatch Cache. Runtime receives copies explicitly; the host no longer
prepares dependency globals or maps Runtime ahead of native initialization. That
ordering improvement does not implement the construction graph in section 4.

The first upper native entry is now
`libbirnam_bootstrap.dylib:birnam_native_module_v1`. Its only predecessor is Id.
Its [native owner](../../abi/fixtures/bootstrap-v2/README.md) calls Id's current
loading message for every subsequent native construction. It retains Object and
Protocol initialization results, including nested loads, before releasing
argument roots. Before Lifetime exists those references form a working early
root set. Explicit adoption installs actual records in the completed root registry
before discarding that set. Later loads use the registry directly; explicit
result release permits reclamation. Transfer rejects reentry and retargeting;
ordinary native load failures propagate unchanged. Bootstrap's native owner is
retained by the host through construction/application lifetime and released at
teardown, without requiring a GC object or a forward Runtime callback.

`libbirnam_root_registry.dylib:birnam_native_module_v1` follows through Bootstrap.
The registry itself requires only Id. Initial Object arguments are retained before it publishes a Word pointing to
its complete [root owner descriptor](../../abi/fixtures/root-registry-owner-v1/README.md).
Native registration, retain/release and root enumeration work without Runtime or
GC, including their failure paths. Admission checks the native owner's thread;
there is no future Runtime callback. After Runtime is complete, it replaces that
working mutation policy with its storage-observation admission. Canonical root
queries still work during collection/observation. Existing root-provider ABI v1
remains intact.

The host's `begin_id` initializes Id and Bootstrap; `initialize_roots` constructs
the root registry and adopts Bootstrap's early set. `begin_floor` combines those
steps. `InitialFloor::finish` constructs the remaining Runtime with the same owners. Ordinary `load_floor` and
`start` use this path, while `start_with_floor` continues an explicitly constructed
prefix. Runtime's native entry in `libbirnam_runtime.dylib` receives nine
completed predecessor pointers as Word operands and invokes the temporary
[v15 constructor](../../abi/runtime-construction-v15.md). Earlier Runtime bridges
and normal `id_init` exports are removed. Explicit Runtime diagnostic builds
retain the old local initialization path for fixtures that have not yet migrated;
that path is absent from normal Id and Runtime libraries. The host's obsolete
fallback preparations for completed native predecessors are removed.

Retained root records transfer with the registry into Runtime and are included
in actual GC and diagnostic roots. Releasing a record removes its reference;
tokens are local to the registry and never reused. Released records are removed,
so native storage tracks live roots rather than cumulative registrations. The returned root view is
borrowed until registry mutation and must be copied before invoking callbacks.
The remaining Lifetime records now initialize in `libbirnam_lifetime.dylib`
through Id, before Runtime or any value/collector library is available. Its
predecessors are Id and the complete native factories returned by
`libbirnam_payload_layout`, `libbirnam_trace_hooks` and `libbirnam_finalizers`.
Those factories construct their public capabilities with Lifetime's own working
admission, membership and record operations. They do not point forward to Runtime.

[`LifetimeOwnedV2`](../../abi/fixtures/lifetime-v2/README.md) owns the exact payload
byte descriptions, trace-hook attachments and finalizer records. Runtime adopts
that owner through `birnam_runtime_init_v11`; its behavior/heap tables retain no
copies of these records. Collection, diagnostics, Image and recovery query the
same records through the native boundary. Public APIs retain their existing ABI
and code ownership. Runtime replaces Lifetime's initial Id-based admission/facts
policy with its completed observation/permanence policy. Canonical queries do not
invoke that policy or arbitrary lookup during collection.

`InitialFloor::initialize_lifetime` exposes the completed prefix; ordinary
`finish` invokes the same step. Weak layout/hook retirement is separate from
finalizer detachment, so a finalizer attached to a behavior still runs after its
behavior metadata retires. Native Lifetime destruction releases all description
bytes before destroying its capabilities and never invokes object finalizers.
Legacy `id_init` construction and unit harnesses retain local service records
until their consumers migrate; current production uses only the native owner.
Bootstrap now owns common-protocol initialization results. Routing the remaining
legacy constructors through it, completing ownership handoff as module exports
are published remain required. Collector activation now follows the completed
value/Trace/collection prefix as described below. This prefix does
not move Runtime/compiler code out of Id.

`InitialFloor::initialize_base_values` then loads the existing Boolean and Nil
libraries through Bootstrap. Boolean's native entry takes completed roots and
Lifetime, creates the actual Nil/Boolean identities through Id messages, and
installs identity/basic Boolean operations. Nil's native entry takes those
completed identities and installs the Object/Nil predicates. Their
[base-value descriptor](../../abi/fixtures/base-values-v1/README.md) contains only
already constructed objects. Basic methods use that completed native context;
they do not read the later String/Block/Runtime API. Printing and Block control
are absent until the later extension entries install them.

Runtime adopts these same objects and layouts, retaining the existing permanence
policy for the five canonical objects/behaviors without promoting unrelated early
allocations. Later Boolean/Nil entries preserve the basic method bindings. The
legacy `id_init` path still constructs local constants until its consumers move.
The remaining upper construction graph is still unimplemented.

`InitialFloor::initialize_namespace` then loads Namespace's native entry with
completed roots, Lifetime and base values. Its
[native owner](../../abi/fixtures/namespace-owner-v1/README.md) owns the actual
ordered binding table and a receiver exposing Symbol-based publication/import,
Word-valued count/index access and reverse aliases. Initial names refer only to
completed objects. String, Integer, Error and Block are neither loaded nor
required by this entry. Missing names, malformed operands and bounds have native
outcomes. Its registered root provider visits current binding names/values and
the receiver, copying references before marking. Rebinding does not retain old
values in a second root table.

Runtime adopts the same Namespace storage and adds its existing C-name adapters.
It preserves early published names and initializes the legacy weak dictionary
view from them. The native publication method gains completed Runtime observation
and mirror policy; subsequent native and upper publication still use one table.
The former Namespace preparation is skipped in production. Native destruction
clears the root provider's selection and releases its owned storage; the provider
then reports no roots. The remaining value/construction stages still
need migration, and the legacy weak dictionary mirror remains to be retired
with its consumers.

`InitialFloor::initialize_object_model` loads Object Model with completed roots,
Namespace and base values. Its
[native owner](../../abi/fixtures/object-model-owner-v1/README.md) contains the
actual private-behavior associations and complete Enumeration v2 interface.
Private behavior construction sends existing Id protocols, roots the new behavior
until attachment, and works before String, numbers, GC or Runtime. Native
malformed operands, non-behavior ownership targets and enumeration bounds have
complete early outcomes. Enumeration calls Id facts acquired through lookup;
it retains no Runtime view callback or copied dictionary registry.

Runtime shares that owner between behavior tracing and enumeration. Its old
per-behavior owner field and empty metadata records are removed. A live behavior
contributes its actual owner edge; dead-behavior retirement removes the native
association before Id detaches the allocation. The native private-construction
method survives adoption and later Object Model mounting. Canonical associations
are established early and are not overwritten during adoption. Object Model's
production enumeration-factory preparation is removed.

Runtime installs completed preparation, reassignment and owner/parent adapters
with explicit contexts. This avoids the legacy active-runtime requirement when
an early native method invokes them through Id after adoption. The owner/parent
query adapters temporarily preserve the old all-Oop null sentinel; converting
those remaining native consumers to the typed Unit contract is still required.
Legacy local owner records and enumeration factories remain only for unmigrated
`id_init` consumers and local harnesses. These changes complete the current
Namespace/Object Model construction prefix, not the remaining value and GC
stages or the Runtime/compiler package separation.

`InitialFloor::initialize_string` loads the existing String library through
Bootstrap with completed roots, Lifetime and Namespace. Object Model is already
installed. Its [native entry](../../abi/fixtures/string-v1/README.md) constructs
the real String behavior/prototype, installs its raw layout and finalizer-backed
byte ownership, and publishes the names. `fromBytes:` accepts native Bytes;
`bytes` returns borrowed Bytes; `length` returns a Word; `clone`/`new` copy storage.
No Integer, Error, Float, Block or Runtime dependency is present in these methods.

String sends current Id allocation messages. It validates the returned allocation
against selected inert Id facts before writing, rejects existing ownership or
finalizers, roots clone inputs across dispatch, and attaches cleanup through the
already initialized Lifetime records. No storage-map borrow spans a send.
The completed String table is adopted by the upper consumers; production skips
String's legacy preparation. Its separate text extension installs methods once
the number dependencies exist. Late String mounting preserves early names,
storage and native bindings. This completes step 5's byte-storage construction.
The aggregate Runtime remains in Id.

`InitialFloor::initialize_error` now loads the actual installed Error library
(`libbirnam_id_error.dylib`) with completed roots, Lifetime, base values,
Namespace and String. Its [native entry](../../abi/error-value-v3.md#production-error-construction)
returns the Error factory as an Object, publishes the existing three Error names
and registers its own roots. Bootstrap releases its temporary result root once
that publication is complete. No descriptor or new library is added.

The existing factory selectors, kind/message access, recognition, Error
propagation and Object missing-message construction are all usable before
numbers, Block or Runtime. Kind/message references are rooted across allocation;
payload writes validate the returned allocation and readers validate stored
references. Lifetime owns the existing two-Oop layout. Runtime preserves an
already installed Object DNU, and late Error mounting preserves the native
factory, layout and method/context identities, including early replacements.
The old stateless Error Value v3 adapter remains for legacy IdApi consumers; it
owns no representation and now sees a completed factory before numbers mount.
This completes step 6's native Error construction.

`InitialFloor::initialize_integer` constructs the actual Integer base through
Bootstrap with completed roots, Lifetime, base values, Namespace, String and the
Error factory. The [native entry](../../abi/integer-v1.md#production-construction)
returns the existing Integer operations table. It owns the same signed-64-bit
payload and codec, constructs canonical identities, installs the raw layout and
publishes the names. Integer construction, arithmetic, comparison, printing and
Error propagation work before Float, Word, Block or Runtime. Allocation sends
the current Id method and checks actual size/behavior before writing. Equality
and ordered comparison retain their existing semantics, including double-precision
rounding in ordered comparisons.

This completes step 7's Integer base. Later entries reuse its actual operations
table and identities; mixed-number methods replace only original base entries,
preserving early replacements. Float conversion and Block iteration are absent
until those dependencies exist.
The old numeric compiler declaration assumes direct allocation and still returns
zero for this message-based constructor. The [v5 native declaration](../../abi/native-numeric-semantics-v5.md)
now supplies the successful exact-Integer contract for actual native entries.
Compiler construction admission checks current lookup and allocation; arithmetic
also checks Namespace and Symbol lookup. Numeric predictions preserve complete
function/context pairs, and closure admission checks the current native
owner/parent entries. Replacements withdraw admission without running their
callbacks. These upper compiler checks must move with Execution/compiler during
the remaining package split.

String, Error and Integer root their cached selectors and predecessor references.
Removing a method binding must not collect a selector still held by a native
context; the same selector can be used to restore the method after collection.

`InitialFloor::initialize_float` constructs Float after Integer through Bootstrap.
Its [native entry](../../abi/float-v1.md#production-construction) takes the completed
predecessors and Integer table, owns the existing IEEE bits representation, and
publishes the canonical identities and seventeen existing methods. `fromBits:`
and `bits` use native Words and preserve the full payload, including signed zero
and NaN payloads. Construction sends the current Id allocator and checks actual
allocation size/behavior before writing. Conversion errors, arithmetic, printing
and live Error/ErrorVt lookup work without Word, Block or Runtime.

Once Float is complete, it sends Integer `installFloat:` with its table as a
native Word. This is an explicit extension entry on the existing Integer object.
It creates a separate complete context, installs mixed arithmetic/comparisons and
`asFloat`, and preserves original-base replacements. Copied Integer methods keep
their existing mixed receiver behavior. Integer's base has no future Float slot.
Late mounting preserves these entries and adds only the remaining Block iteration.
Production skips Float's legacy preparation and adopts the identical operations
table into upper consumers; legacy preparation now returns that table if called
on the same native Id. The legacy-only constructor remains for native clients.

`InitialFloor::initialize_word` constructs Word through Bootstrap with completed
roots, Lifetime, base values, Namespace, String, Error and Integer. It takes no
Float input. Its [native entry](../../abi/word-v1.md#production-construction)
constructs the existing unsigned representation, raw layout and canonical names.
`fromBits:`/`bits` use native Words; equality, hexadecimal printing and checked
Integer conversion work before Block/Runtime, including overflow errors through
current Error factory messages. Cached selectors and canonical identities are
rooted. Production skips legacy Word preparation; upper consumers adopt the same
table and late mounting preserves native methods, layouts and replacements.
Legacy preparation returns that table if called on the same native Id.

`InitialFloor::initialize_text` now sends String `installText:` with completed
base-value, Integer and Float descriptors, then sends nil and true
`installPrinting:` with completed roots and String. These explicit native
extensions install all fourteen existing String text/numeric methods and
Nil/Boolean printing before Block/Runtime. Each extension owns a separate
complete context; no future provider is stored in the base owners. Existing
bindings are preserved and late legacy mounting keeps the native methods.

String roots arguments and temporary printable results across callbacks and
uses the live numeric/String operation tables. Indexing remains byte-oriented;
invalid indices and numeric parses keep their existing nil results. Symbol
conversion preserves its first-NUL boundary. Native and legacy text paths share
trimming, replacement and bracket-scanning algorithms. Nil/Boolean printing sends
the current String construction message; cached selectors and predecessor
references remain rooted.

This completes the value/text entries in step 8, with the real sixteen-library
prefix exercising both ordinary results and advertised failure paths. No new
library, representation, descriptor version or Runtime bridge is added.
Execution/Block and the remaining upper/package separation
are still open. Native Integer/compiler admission is described above.

The live graph portion of step 9 now constructs before Runtime as well.
`InitialFloor::initialize_trace` loads Native Mark through Id, then constructs
Trace with completed root-registry, Lifetime, Object Model and Native Mark
inputs. Its [64-byte descriptor](../../abi/trace-graph-v2.md) owns root discovery,
Id/owner edges, payload scanning and inherited-hook selection without a Runtime,
Machine or compiler input. Production Runtime borrows that same completed graph;
its mark admission, Execution roots and retirement policy remain upper inputs.
Native roots and hooks now receive Trace's context-free mark callback, backed by
Native Mark's owned scope; the callback no longer points into Id. Normal startup
skips the old Native Mark factory preparation in Id; legacy initialization
retains that path.
The eighteen-library prefix exercises this construction with the remaining
libraries unavailable. Validation examines the actual early allocation graph and
rejects missing payload descriptions without marking or reclaiming anything.
Native Stack now supplies register capture through its complete v4 factory.
Graph construction alone does not activate collection. The subsequent explicit
activation validates layout or native-hook coverage, establishes the lasting
native boundary and installs collecting constructors. Native modules remain
responsible for declaring their owned references; the graph validator cannot
infer references hidden in arbitrary native storage.

`ObjectMemory` owns allocations and identities. Collect now constructs the
[collection-state owner](../../abi/collection-state-v2.md) before Runtime, from
complete Native Stack, GC Settings and Allocation Budget factories, Trace graph,
Lifetime and Object Model plus its own phase implementation. Its native entry enumerates the existing Id allocations
and creates the actual weak mark/permanence records and initial budget. Runtime
adopts the same records, phase, anchor and settings; it preserves early state and
adds only allocations made since their initial snapshot. Public mutations start
with working thread admission, then receive Runtime's complete observation policy.
The four former production preparations inside Id are skipped. The completed
coordinator is adopted with this owner instead of awaiting a later mount. Marking and dead
record removal now run in Collect, including deferred allocations when realized.
The legacy local mark map remains only for legacy initialization/unit consumers.
The [collector capability](../../abi/collector-v3.md) also owns live tracing,
Lifetime/Object Model retirement, Id index retirement, finalizer detachment and
allocation release. Once Execution exists, Runtime installs a completed extension
that supplies scoped upper roots and deferred/code retirement on every call,
including calls through previously saved collector entries. Without Execution
there are no such records.
The state owner advertises no operation needing a future callback. Explicit
activation and GC-aware constructor installation now run through Collector v3
before Runtime construction. Execution and the remaining package split are open.

`Heap` retains the deferred-allocation journals and storage-consumer adapter.
Allocation records use three native words. The collection owner's weak records
hold the exact payload pointer and mark/permanence flags; they do not root the
allocation or contain finalizer policy. Lifetime owns the separate finalizer
entries. This metadata overhead is outside payload-byte accounting.
`Vtables` and `Symbols` require only Id allocation and their own
indexes. `Behaviors` accesses Object Model owner records and Lifetime descriptions. The obsolete allocation-identity, dictionary-index and
mutation-counter factories and their host preparations are removed.

The current behavior payload is five words (40 bytes): capacity, tally, keys,
values and parent. Each dictionary value is a typed 24-byte Operand. Image
runtime ABI is **7 (`f57d502f7c1c0437`)**, image format is 8, and upper IdApi
remains version 20. [Dispatch Cache ABI v2](../../abi/dispatch-cache-v2.md) stores
complete 16-byte native entries in 40-byte slots; its descriptor is 56 bytes.
These versions reject older incompatible native representations.

Runtime installs completed GC-accounted construction/interning, managed native
loading and observed mutation as ordinary method replacements. The same native
carrier works before and after adoption. Id provides allocation/behavior
construction, its prototype, allocation extent/identity, behavior and Symbol
bytes before language value modules exist. Native caller roots and upper
payload descriptions remain explicit obligations.

Collect selects its exact allocation enumeration, index retirement and detachment
entries through Id lookup during construction. The completed native Trace graph
selects its own Id fact entries and supplies primordial references, marked-object
edges, behavior owners, payload layouts/hooks and native roots. Runtime no longer
selects redundant Id storage callables on this production path. The mark and
collection-source implementations now live in Collect. Runtime supplies its
remaining upper roots, compatibility roots, deferred materialization and
executable-owner retirement callbacks. No Id registry borrow crosses a visitor.
The diagnostic storage view,
legacy Symbol compatibility mode, adoption and Execution's dead-binding accounting
still access internal Rust storage. Production index retirement, allocation
detachment and release now use the native storage callables. Upper behavior
metadata and per-allocation finalizer records retire separately; the coordinator
keeps detached payloads readable until their finalizers return. Lifetime's root and representation owners now initialize before Runtime;
the full Id allocation/registry adoption cutover
remain required.

Ordinary binding, inherited caching and Machine synchronous targets carry the
complete native function/context pair. Context-only replacement is observable.
Effectful lookup can replace a binding while the current send invokes its exact
selected entry once; the next lookup sees the replacement. DNU performs a fresh
handler lookup without repeating the original lookup. Generated adapters enter
their exact registered implementation rather than resending the selector.

Execution owns managed outer turns across sends, Machine work and nested native
callbacks. The pinned outer return roots its result before draining retirements;
a bounded Machine return under a native caller does not drain. Finalizers of TCC
Units and mapped owners transfer native resources into detached queue records
before their Oop payloads are freed. Candidate and CodeBinding disposal use the
same checkpoint mechanism. Alien's callback registry retains its Block and Unit
before publishing the callback address. These ownership implementations still
reside in Runtime inside `birnam-id` and must move together to upper Execution.

At the upper language boundary, native Object results remain objects, Unit
becomes Nil, invalid arguments become `nativeInvalidRequest`, and other failures
become `nativeFailure`. Successful native scalars/bytes/entries/protocols require
a language wrapper and currently produce `nativeResultRequiresWrapper`. Error
construction is an ordinary upper send; bare Id has no Error dependency.
`legacy_method.rs` still adapts existing all-Oop consumers, preserving their raw
argument-buffer convention. Legacy reflective lookup/removal cannot represent
arbitrary native function/context pairs and remain a required migration.

Current debug evidence:

- The Id suite passes **308 tests**, with 15 existing ignored tests. It covers
  early object/behavior creation, adoption, identity/field preservation through
  first GC, post-adoption accounting and reclamation after dropping roots. The
  collector acquisition test installs an Id root callable before construction,
  replaces the method afterward, and proves the existing collector retains the
  selected callable while a new consumer observes the replacement.
- A fresh debug install publishes **55 native libraries and 28 authored modules**.
  `scripts/id-native-loading-smoke.py PREFIX` loads that actual installed Id with
  upper libraries absent, constructs objects and installs methods from independent
  C, and then starts the normal host using the same carrier and graph. The C
  visitor reenters Id, allocates during allocation enumeration, verifies snapshot
  membership, and checks complete failure propagation. It also retires independent
  behavior/Symbol indexes and detaches/releases allocations before Runtime exists,
  verifies immediate membership removal and live detached bytes, and rejects
  ordinary Object arguments to the native destruction operations. After upper adoption it
  supplies completed roots/collector explicitly and verifies actual collection
  preserves the rooted object's identity and payload. Ordinary typed sends and
  native-to-language Error outcomes also pass. With only Id, Bootstrap and
  root-registry libraries available, a second C module retains a new object
  before Runtime
  exists. The same retained record survives adoption and precise collection;
  release then permits reclamation. Repeated retain/release removes records
  without reusing tokens. Invalid initializer operands fail before publication.
  With only Id, Bootstrap, roots, the three primitive description libraries and
  Lifetime available, another independent C initializer declares payload layouts, a native
  trace edge and three finalizers, including one on a behavior. The same records
  survive normal adoption and precise collection; releasing the root permits
  reclamation, runs every finalizer once and removes the weak descriptions.
- The Bootstrap prefix runs with only Id and Bootstrap available. Independent C
  makes a nested native load return an otherwise unrooted Object, exercises
  Protocol retention using the real Id, and propagates a missing-library failure.
  The object survives root transfer, Runtime adoption and precise GC; explicit
  result release permits reclamation. The same check passes after Runtime exists.
  Bootstrap's installed exports match its manifest and its only native link is
  the platform library. The ABI/host suites pass 24/86 debug tests. Normal host
  startup, compiled modules, images and baked execution pass the root-registry
  workload with this loader in the production path.
- With only the nine Id/Bootstrap/Lifetime/base libraries available, the
  independent C base-value fixture exercises Nil/Boolean operations and native
  failures while printing and Block control are absent. It replaces a Nil
  predicate, reloads both completed base entries, then verifies the same values,
  layouts and exact basic function/context pairs after normal startup and
  precise GC. The Boolean and Nil image workloads pass source, compiled, native
  image and baked execution, including live method replacement. The debug
  ABI/host/Id suites pass 24/86/308 tests (plus four host binary tests).
  Installed Boolean/Nil exports match their manifests and link only the platform
  library; Id exports only the current v5 Runtime bridge.
- With only ten predecessor libraries available, Namespace's independent C
  consumer verifies publication/import, ordered native enumeration, first reverse
  alias, malformed operands and bounds with String/Integer/Block/Error absent.
  The actual table and aliases survive Runtime adoption and precise GC. Removing
  one alias preserves the object; removing the last permits reclamation, and
  later C-name imports observe the cleared binding. Both Namespace and Namespace
  Enumeration workloads pass source, compiled, native image and baked execution.
  Debug units pass for ABI (24), host (86 plus four binary tests), Id (308) and
  Namespace (20). The installed Namespace exports match its manifest and it
  links only the platform library. The v4 Runtime bridge replaces v3; production
  no longer prepares Namespace's factory inside Id.
- The eleven-library prefix includes Object Model's actual installed native
  entry. Independent C constructs a private behavior before successor modules,
  exercises native enumeration and argument/target failures, and roots only that
  behavior. The same owner edge retains the object after Runtime adoption and
  precise collection. Releasing the root permits both allocations to be reclaimed
  and removes the weak record. The same construction/release check passes after
  Runtime exists, with the original native ensure entry still selected. The
  fixture uses an inherited probe method so Store backfill does not add a second
  root to the private behavior under test.
- The twelve-library prefix constructs String bytes with Error/Integer/Float/
  Block absent. The independent C fixture verifies binary and empty bytes, copy
  ownership despite raw payload mutation, malformed operands, live allocation
  replacement (including nested sends), rejection of small/already owned results,
  and unchanged clone bindings. Early Strings survive upper adoption and GC;
  releasing their roots removes both their allocations and byte/finalizer records.
  The fixture repeats construction and reclamation after Runtime adoption and
  confirms the upper consumer has adopted the identical String table/identities.
  String's source/compiled/saved-image/baked-image workload passes. The legacy C
  method suite passes 410 byte/index/slice cases and 18 numeric boundaries;
  String storage units (2), Id units (308) and host units (86) pass. The installed
  String library exports exactly its seven manifest symbols and links only the
  platform library. The existing String ABI and private Runtime v5 bridge remain
  unchanged; no new factory preparation or production library was introduced.
- The thirteen-library prefix constructs Error before Integer/Float/Block.
  Independent C verifies the three existing factory forms, predicates, kind and
  message access, propagation, missing-message construction, malformed operands,
  and reader/allocation safety. An Error retained by one root keeps its kind and
  String message alive after Runtime adoption. Releasing that root reclaims all
  three and releases the String buffer. Changed payload-layout records, native
  factory replacements and Object DNU entries survive adoption and late Error
  mounting. The same fixture repeats construction/reclamation with Runtime
  active. Error's source, compiled, saved-image and baked-image workloads pass,
  including fresh factory replacement and exact recognition. Id's 308 and the
  host's 86 debug units pass; the installed Error exports match its five-symbol manifest and
  link only the platform library. The private Runtime v5 bridge is unchanged.
- The fourteen-library prefix constructs Integer with Float/Word/Block absent.
  Independent C checks 605 signed arithmetic cases and 605 comparisons, boundary
  printing, Error propagation, malformed operands and replacement allocation
  bounds. The fixture repeats after Runtime adoption with precise GC, preserving
  canonical storage, early method replacements and unchanged base entries.
  Retained Integers survive collection and released ones are reclaimed; cached
  selectors survive method removal and collection. Integer's source, compiled,
  saved-image and baked-image workload passes. Integer (1), Id (308) and host (86)
  debug units pass. The installed Integer exports match its nine-symbol manifest
  and link only the platform library. The private Runtime v5 bridge is unchanged.
- The fifteen-library prefix constructs Float and installs Integer's mixed-number
  extension with Word/Block/Runtime absent. Independent C checks 1,164 arithmetic
  cases, IEEE comparisons, signed zero/NaN payloads, conversion limits, printing,
  live ErrorVt replacement, copied Integer methods and allocation bounds. The
  fixture repeats after adoption with precise GC; native method/context
  replacements and retained payload bits survive, and released objects are
  reclaimed. Float (1), Integer (1), host (86) and Id (308) debug units pass.
  The legacy Float client passes 676 arithmetic cases plus conversion, publication,
  replacement and GC checks. Installed Float has its eight manifest exports and
  links only the platform library. Float's source, source-free compiled, saved-image
  and baked-image workloads pass, including live arithmetic replacement and Error
  factory rebinding. Integer's error-value image workload also passes with the
  native mixed extension installed. The Runtime v5 bridge is unchanged.
- The sixteen-library prefix constructs Word before Block/Runtime. Independent C
  checks 262 unsigned payloads against hexadecimal formatting and conversion
  bounds, exact equality, fresh identity, malformed operands and allocation size.
  The fixture repeats with precise GC after adoption: method/context replacements
  and retained bits survive, and released Words are reclaimed. Word (1), host (86)
  and ABI (24) debug units pass. The legacy Word client passes 81 equality cases
  plus representation, conversion, replacement and collection checks. Both Word
  image workloads pass source, source-free compiled, saved-image and baked runs,
  including overflow and live Error factory replacement. Installed Word matches
  its eight-symbol manifest and links only the platform library.
- The completed sixteen-library value/text prefix runs String's fourteen methods
  and Nil/Boolean printing before Block/Runtime. Independent C covers all 256
  byte indices, binary slices/equality, trimming, replacement, bracket scanning,
  first-NUL symbol conversion, numeric parsing boundaries and malformed native
  operands. It repeats after adoption with precise GC, including collection
  during printing and subsequent byte callbacks. Early method/context replacements
  survive; releasing the retained String reclaims its identity and byte buffer.
  The legacy String client passes 410 binary/index/slice cases and 18 numeric
  boundaries; host debug units pass (86). String, Nil and Boolean source,
  source-free compiled, saved-image and baked workloads pass with native text
  and printing. Their existing 7/4/4-symbol manifests match the installed dylibs,
  which link only the platform library. No Runtime bridge version changes.
- The native Trace graph is constructed in the production eighteen-library
  prefix before Runtime. An independent C client verifies exact Id fact selection
  survives method replacement, conservative/precise scanning, core-only queries,
  inherited and replaced hooks, Object Model owner edges, sink failures and live
  layout replacement. A separate clean-prefix process validates all initial
  representations, rejects a newly missing description, then succeeds after its
  layout is installed. The same graph works after Runtime adoption and actual
  precise GC. Debug ABI/host/Id/Native Mark/Trace units pass (429 total, 16 ignored).
  Trace and Native Mark expose exactly 6 and 4 symbols and depend only on the
  platform library; installed Id matches its bare manifest plus the two existing
  module wrappers, with only the v6 Runtime bridge. Payload Layout and Trace Hooks
  pass source, source-free compiled, saved-image and baked execution. Native Mark
  also passes mapped/rebuilt images with the legacy preparation skipped; its C
  fixture verifies the callback now belongs to Trace and retains the same identity
  across providers/hooks, null/duplicate marks and nested-collection rejection.
- The twenty-two-library prefix constructs Collect's mark records, budget, phase,
  stack anchor and settings before Runtime. Independent C checks exact initial
  inventory/accounting, mark and permanence rules, control operations, rejected
  coordinator input, preservation through adoption, and accounting for subsequent
  allocations. Actual GC removes records before finalizers, preserves an early
  retained root and reclaims it after release. ABI, host, Id, Collect, Native Stack,
  GC Settings and Allocation Budget units pass (437 total, 16 ignored). The four
  control image workloads pass source, source-free compiled, saved and baked
  execution; Native Stack, Budget and Phase also exercise mapped/rebuilt images.
  The final native harness and Phase image run verify immediate coordinator
  adoption. Installed Collect exports exactly six symbols; each of the three
  primitive owners exports four, matching its manifest. All link only the platform
  library. Installed Id matches its manifest plus the two module wrappers and
  replaces the intermediate bridge with `birnam_runtime_init_v7`. This verifies
  state adoption; the subsequent activation implementation is described below.
- Native Stack v4 retains Darwin arm64 register capture and interval validation,
  and adds the thread-lifetime boundary used by activation.
  The independent C scanner fixture passes at default optimization and `-O2`,
  observing an x19 value in the real save area, callback frame ordering, rejected
  intervals and nested capture. The native loading harness invokes that operation
  before Runtime; later collection uses the same factory. ABI/host/Id/Native Stack
  units pass (425 total, 16 ignored), including source-probe root coverage. The
  legacy native client preserves automatic collection, native/provider roots,
  callback/finalizer anchor resets, nested GC and owner/observation admission.
  Native Stack's source, source-free compiled, mapped/rebuilt image and baked
  workloads pass. The installed-host anchor-selection test also passes; its
  harness uses consistent unwind flags to avoid Cargo's abort/unwind artifact
  collision. Installed Native Stack and Id match their 4/68 export manifests;
  the GC-specific Darwin `_setjmp` dependency now belongs to Native Stack. Id's
  separate compiler-linked `setjmp` dependency remains with the pending compiler
  package extraction. The subsequent explicit activation transition is described below.
- The same twenty-two-library prefix now constructs the collector itself from
  completed native owners. Independent C verifies root-only survival, later
  release, retirement of unreferenced Symbols/behaviors, removal of mark/finalizer
  metadata before finalization, nested collection rejection, and accounting reset.
  A separate process verifies a rejected root callback leaves all allocations and
  finalizers intact and the result count untouched. The full native loading and
  adoption harness passes with the same collector. ABI/Collect/host/Id units pass
  (423 total, 16 ignored). Collection and Trace Hooks pass source, source-free
  compiled, saved-image and baked execution. Finalizers also pass mapped/rebuilt
  images, replacement, detachment and native cleanup. The portable CodeBinding lifecycle
  workload preserves publication, replacement, self-removal and retirement.
  Collect depends only on `birnam-abi`, exports its existing six declared symbols,
  and links only the platform library. Installed Id matches its 68 exports with
  the new temporary v8 bridge. These checks establish native collection ownership;
  they predate the explicit activation/construction transition described below.
- Serial debug unit suites pass for Object Model (4), Id (308), host (86) and ABI
  (24). The combined multi-package command encountered Cargo's existing
  abort/unwind artifact filename collision; separate package runs passed.
  The final installed Enumeration and Allocation Identity workloads pass source,
  compiled, native image and baked execution, including independent host proof
  of mapped/rebuilt images, parent/behavior replacement, method lookup, active
  Frame access, identity and finalization. Object Model's installed exports match
  its manifest and its only dynamic dependency is the platform library.
- The root-registry image workload passes for source, source-free compiled,
  native Code images and baked execution, including duplicate provider order,
  callback-time append, nested collection, roots, aliases and reload. Debug unit
  checks pass for ABI (24), host (86), Id (308) and root registry (5). The installed
  root-registry exports match its manifest; Id has the declared exports plus the
  installer's existing `module_init` and `birnam_symbol_borrow_v1` wrappers.
  The Lifetime change additionally passes 431 debug unit tests across ABI, host,
  Id, payload layout, trace hooks and finalizers. Installed Lifetime and all three
  primitive description libraries match their export manifests. Lifetime's only
  Cargo dependency is `birnam-abi`; its dylib links only the platform library.
- The installed payload-layout, trace-hooks and finalizers image workloads pass
  with native Lifetime records across source, source-free compiled, native Code
  image and baked execution. They preserve borrowed-prefix replacement, arbitrary
  layout kinds and image rejection/restoration, hook-marked children, finalizer
  replacement/detachment, native cleanup, roots, aliases and reload.
- `scripts/allocation-identity-image-smoke.sh PREFIX` exercises source, source-free
  compiled, mapped/rebuilt native images and baked execution, with identity,
  dictionary replacement, finalization, active Frames, aliases, GC and reload.
  Existing dispatch and lifecycle coverage additionally exercises exact adapter
  entry, context-only replacement, nested callbacks, outward-result rooting,
  detached-owner release, portable resumable eviction and CodeBinding self-removal.
  The installed `portable-resumable-lifecycle-smoke.sh` and
  `portable-code-binding-lifecycle-smoke.sh` checks also pass with native Id
  detachment/release, including eviction survival, replacement and self-removal.

The deferred-identity fixture exposed a legacy custom-lookup failure that also
reproduced on `e8045b44`: converting a selected native allocator to a legacy
Method address and back discarded its context and typed Word argument contract.
The upper legacy adapter now retains the original native entry for the duration
of that lookup. Nested lookups have separate temporary records; later binding
mutation does not change the entry already selected. Different native contexts
represented by the same legacy address in one lookup are rejected as ambiguous.
This supports existing callable legacy adapters only; arbitrary native entries
still require migration of reflective consumers, not invented Method addresses.

The unchanged `scripts/deferred-identity-image-smoke.sh PREFIX` now passes in
full: source, compiled module, saved mapped/rebuilt image and baked execution,
including the actual host's image-mode checks and forced recovery. Its independent
Storage observer records the same closure/numeric deferred writes in all three
image forms: `(386, 150)` normally and `(386, 75)` with forced recovery. The
existing assertions for identity, finalization, Frame reads/writes, custom lookup,
dictionary mutation, aliases, GC and reload remain intact.

Native compiler admission uses the current allocator and lookup, full native
function/context pairs, and the declared Namespace read path. Closure admission
also recognizes the native owner/parent protocol entries. The independent
`scripts/native-integer-admission-smoke.sh PREFIX` compares counted effects with
optimization disabled/enabled for eight replacements: allocation, Namespace
binding lookup, interning, Symbol validation, lookup, owner/parent queries and
arithmetic. All counts match; restoring every method restores closure admission.
The production native-loading harness also passes with C checks for the new
declarations before and after Runtime adoption. Debug export inventories match
the Integer and Namespace manifests (11 and 5 symbols respectively).

The legacy adapter tests exercise the actual native allocator with a Word size
after nested lookup and binding replacement, and distinct live Runtime contexts
sharing one legacy address. The current ABI/Id/Integer/Namespace unit suites pass
(356 passed, 16 ignored), including native relation-context rejection. Legacy
Integer error/value and numeric-context fixtures pass their 800 oracle cases and
live callback/restoration checks. These tests do not close arbitrary native
reflection migration or the package split.

Remaining architectural acceptance requires all of the following:

1. Move Runtime, compiler/artifact dependencies and upper ownership out of Id;
   remove temporary forwarding exports and migrate native/reflective consumers.
2. Route the remaining legacy constructors through Bootstrap and complete their
   result-ownership handoff when module exports become available. Common native
   initialization results are now retained and transferred to the initialized
   root owner. Migrate remaining legacy local-service paths with their consumers.
3. Finish storage-consumer migration in adoption, diagnostics, compatibility and
   Execution accounting. Collection now uses Id's native retirement/release path;
   the remaining Rust allocation/registry handoff must not become a permanent
   coupling between separately packaged modules.
4. Replace the aggregate preparations/initialization with real base and extension
   entries. Publish the entry-level predecessor manifest, map every SCRIPT_MODULES
   entry or removal, reject cycles/missing predecessors, and update the diagram.
5. Prove real construction prefixes, source/compiled/image/worker behavior and
   the source/Cargo/private-export independence audits specified above. Passing
   the current tests does not close these unimplemented requirements.

### Mandatory completed collector extension

The public collector now has an explicit completed-owner extension
([Collector v2](../../abi/collector-v2.md)). The base remains independently usable
before Runtime. Once Runtime and its Execution objects exist, Runtime installs
its full roots and retirement operations. Saved public descriptors select that
extension on every call; they cannot accidentally keep collecting the earlier
heap without Execution's obligations. The scoped base passed into the extension
performs the original native collection sequence without redispatch.

Normal Runtime collection now passes only the captured native interval, while
trace probes pass only their additional roots. Runtime's extension owns the full
upper retirement sequence and rejects a competing per-call retirement table.
The outer collection owner keeps its 128-byte layout and requires collector v2;
the temporary upper constructor becomes `birnam_runtime_init_v11`.

This extension closed the saved-entry obligation needed for section 5's
construction wrappers. Activation was separate work; its production
implementation follows below. The module/package split remains open.

Validation passes for the ABI/Id/Collect unit suites (341 passed, 15 ignored),
the installed native-loading harness and the collection, finalizer and
deferred-identity image scripts. The saved-entry test traces an otherwise
unbound Symbol with no caller roots and proves Runtime's compatibility root
survives; the same early descriptor later collects with Runtime retirement.
The independent C extension test checks added roots, release, all retirement
callbacks, repeat installation and rejection of replacement/nested collection.
The image tests cover source, compiled, mapped, rebuilt and baked execution.
Actual deferred writes agree across image forms: `(386, 150)` normally and
`(386, 75)` under forced recovery. Installed Id/Collect exports match their
manifests, including Id's then-linked artifact exports. No release validation was run.

### Production collector activation and construction

[`Collector v3`](../../abi/collector-v3.md) now activates the production heap
before Runtime/Execution. `InitialFloor::finish` first calls
`activate_collection`: Bootstrap v2 pauses publication, the live Trace graph
validates Id edges and payload layout/native-hook coverage, Collect reconciles
records and installs Native Stack v4's lasting thread boundary, then replaces
construction methods through Id messages. It publishes Active only after all
steps succeed. A failure retains the existing owners and leaves collection
disabled without marking, finalization or reclamation. Bootstrap's early roots
have already been transferred into ordinary Root Registry records; activation
neither copies the heap nor permanently marks every bootstrap allocation.

`allocate`, `delegated`, `clone` and `new` collect before construction, with their
input explicitly rooted. This prevents a native hook from seeing an uninitialized
new instance. `intern:` accounts a fully initialized Symbol and roots it if a
collection is due; re-interning an existing Symbol never causes collection.
The Id primitives remain non-collecting. New allocations update Collect's own
weak records and budget. Saved early wrappers invoke the public collector, so
they gain the complete later Runtime roots and retirement extension.

Aggregate Runtime construction temporarily retains its actual canonical objects
and compatibility Symbols while it creates Execution objects, then releases those
roots after installing its complete collector extension. This transitional
constructor still installs Runtime's existing upper methods and deferred-accounting
adapter. Those duplicates and the remaining Rust Symbol/dictionary handoff must
disappear with the native Execution/package migration; activation does not claim that cut
is complete. The matching temporary upper entry is `birnam_runtime_init_v11`.

The C activation fixture forces automatic collection through all four construction
paths with only twenty-two predecessor libraries present. It checks result/input
liveness, existing-Symbol behavior, initialized payloads at trace hooks, publication
pause and failure without sweep for an undescribed payload. It also forces GC
during native Integer construction and overflow-error creation. Separate tests reuse
saved constructors after Runtime adoption and force collection while Runtime
itself is being built. The older Id-only storage probes now explicitly destroy
their temporary raw payloads before entering validated startup. Native-hook-only
payloads remain valid coverage; the missing-description negative test still fails.

Validation: the final ABI, Bootstrap, Collect, Id, Native Stack and Trace unit
run passes 356 tests with 15 ignored. The installed native-loading harness and
collection, finalizer and deferred-identity image workloads pass. Deferred writes
remain `(386, 150)` normally and `(386, 75)` under forced recovery in mapped,
rebuilt and baked images. The independent ARM64 stack capture/scanner checks
pass, and installed Id, Collect, Bootstrap and Native Stack exports match their
manifests (including Id's then-linked artifact exports).

The worker-service smoke passes its Cell image round trips but fails the
unchanged ten-second ready deadline, including an isolated retry. A child sample
shows normal authored-module startup. The existing independent worker wire test
then passes source-free image boot, twelve exact replies and orderly shutdown;
its observed ready time was 53.87 seconds while the unit suite also ran, not a
performance baseline. An earlier debug implementation records the same deadline
failure in [Design 81BE](81be-production-namespace-import.md). This establishes
working worker image recovery and transfer, not acceptance of the nineteen-case
worker-service suite. That suite and the full architectural acceptance above
remain open. No production timeout was changed or release validation run.

### Diagnostic storage inventory through completed Trace

[Trace graph v2](../../abi/trace-graph-v2.md) now produces the allocation,
representation and core-edge portion of the production diagnostic storage view.
It uses the same selected Id operations as live tracing, with completed Lifetime
and Object Model records. It reports Id roots and Symbols without exposing Id's
registries. Inspection reads no ordinary payload words and invokes no root
provider, trace hook or finalizer. Its callback receives copied scratch arrays;
no Id or representation-record borrow survives into the callback.

Runtime obtains that snapshot outside its state borrow, then adds Collect's
permanence flags, its current upper roots, optional compatibility Symbol roots,
provider declarations and deferred-journal counts/bytes. Permanent-object roots
are derived from the reported core edges. It no longer re-enumerates Id storage
just to subtract physical bytes from total accounting. Observation admission,
ordinary lookup of `withStorageView:`, epochs, coverage gaps and failure cleanup
retain their existing contracts. The local/legacy constructor still has a
fallback inventory; it is not a second production storage authority.

This removes direct allocation/header/classification/dictionary reads from the
production diagnostic inventory. Runtime's allocation construction, Symbol
compatibility management, behavior mutation and aggregate ownership handoff are
still coupled to Id's Rust types and remain part of the package separation.

Validation passes 350 ABI/Trace/Collect/Id unit tests (15 ignored). The shared
root-building helper preserves the existing collector root order. The
installed native-loading harness passes the eighteen-library graph prefix,
activated construction and post-adoption cases. Independent inspection checks
cover sorted identities, extents, core flags, owner edges, inherited hook and
finalizer declarations, no provider/hook/finalizer invocation, visitor failure
propagation and replacement of Id fact methods after graph construction.

The protected storage-observation script passes source, source-free compiled,
mapped, rebuilt and baked execution. Final-prefix host checks also pass compiled,
mapped, rebuilt and baked observations with epochs, nested rejection, failure
cleanup, aliases and post-GC allocation identity. Id, Trace and Collect exports
match their manifests. No release validation was run. This closes the production
diagnostic inventory migration, not the remaining Runtime/package separation.


### Production allocation ownership stays in Id

Id now retains its allocation registry from the moment the initial graph is
created. The aggregate Runtime constructor no longer takes that registry away.
Its Heap calls completed native Id entries for allocation, interning, membership,
size/identity, enumeration, behavior association and detached release. Temporary
views copy coordinates and facts; they neither own an allocation nor borrow its
private record. Id is destroyed after upper Runtime state at harness teardown.
The local allocation backend remains only for legacy isolated harnesses.

Bootstrap selects the working, non-collecting `allocate` and `intern:` entries
before activation replaces their bindings. The temporary upper constructor is
`birnam_runtime_init_v11`, taking the completed
[primitive allocation descriptor](../../abi/primitive-allocation-v1.md) alongside
its existing predecessors. This is a concrete bootstrap seam: deferred identity
materialization may occur during collection and must not recursively collect or
charge its already-accounted journal entries again. Ordinary upper construction
still performs its existing admission, rooting, collection and accounting before
using these primitives. No future callback is installed in Id.

`allocationAt:` supplies the original pointer for an exact live payload address;
`allocationCount` supplies the current registry length without a copied index or
full enumeration. New objects are allocated with their final behavior, so their
construction does not spuriously advance the method mutation epoch. Explicit
reassociation advances that epoch once, in Id.

This removes production Heap's allocation-registry ownership and direct access.
It does not finish Symbol/dictionary ownership, their private upper interfaces,
native Execution construction or the final Id package separation.


Validation: 337 ABI/Id unit tests pass, with 15 intentionally ignored. A fresh
debug installation passes the independent native-loading harness, including exact
allocation address/count queries, detached storage, non-collecting primitive
retention across activation/adoption and collecting upper constructors. Installed
Id exports match all 68 expected symbols, including its two statically linked artifact exports.
Collection-value, finalizer and protected storage-observation workloads pass
source, source-free compiled, image and baked execution. The deferred-identity
suite also passes mapped/rebuilt/baked recovery with actual write counts
`(386, 150)` normally and `(386, 75)` under forced recovery in every mode.
No release validation was run. These checks establish the allocation migration;
the broader construction and package-separation acceptance remains open.


### Removing upper access to Id registries

Production now keeps Symbol names and ordered method dictionaries in Id
alongside allocations. Runtime retains native operations, not Rust registry
owners or pointers. Existing `symbolBytesOf:`, `behaviorCount`, `behaviorAt:`,
`parentOf:`, `parentOf:put:`, `referencesOf:do:` and retirement operations remain
the sources of those facts. The following native-only operations cover actual
remaining Runtime consumers; they use the existing typed operand carrier.

| Selector | Arguments to Id | Result and purpose |
|---|---|---|
| `symbolNamed:` | Bytes name | Existing Symbol or Unit, without interning; optimizer admission and compatibility checks |
| `symbolCount` | None | Word live Symbol count |
| `symbolAt:` | Word ordered index | Symbol or Unit |
| `symbolNameBytes` | None | Word bytes owned by names, including NUL terminators; compatibility limits |
| `behaviorEpoch` | None | Saturating Word dictionary mutation epoch; existing dispatch/optimizer guards |
| `isBehavior:` | Object candidate | Word zero/one exact membership |
| `behaviorView` | None | Borrowed Bytes encoding the ordered Object array |
| `selectorsIn:` | Object behavior | Borrowed Bytes encoding its ordered key array |
| `bindingOf:in:` | Object key, Object behavior | Direct typed value or Unit if absent; an opaque Object value still hides the parent binding |
| `lookup:in:` | Object selector, Object behavior | Canonical inherited NativeEntry or Unit; no application lookup |
| `bindingsIn:do:` | Object behavior, NativeEntry visitor | Visits copied key/value/weak-flag triples in dictionary order; used for executable retirement |
| `addMethod:to:as:` | Object key, Word behavior address, NativeEntry | Previous entry or Unit; primitive mutation below Store and Execution policy |
| `removeMethod:from:` | Object key, Word behavior address | Removed entry or Unit |
| `addWeakBinding:to:as:` | Object key, Word behavior address, Object opaque value | Unit; existing compatibility dictionaries |
| `newBehaviorWithParent:` | Word parent address, zero for none | New registered behavior, without collection; upper constructors account it |
| `bind:to:` | Object selector, Object receiver | Exact selected NativeEntry or Unit, or unchanged native failure; performs effectful metacircular lookup once |

Policy-free registry mutations require native Word addresses. Ordinary all-Object
language sends are rejected before mutation or allocation; they continue to use
the existing upper methods with collection and executable-lifetime bookkeeping.
Addresses resolve through Id membership to their original pointers.

Borrowed Object arrays are public native views, not Rust dictionary records.
They remain valid until the corresponding registry changes; callers copy them
before callbacks. Binding visitors receive copied entries with no registry
borrow held across invocation. Id neither owns executable generations nor changes
turn policy. Runtime's existing notification, rooting and lifetime bookkeeping
still surrounds primitive dictionary mutation. It snapshots dead selected entries
before Id retires their dictionaries.

`bind:to:` establishes Id's own dispatch context. Runtime no longer establishes
that context with a raw dictionary pointer. Legacy lookup/add/remove adapters
use the completed primitive registry operations instead of relying on an ambient
Id scope. The isolated local unit backend can retain its local scope until its
legacy harness is removed. Application sends still use fresh effectful lookup;
canonical dictionary queries are used only where the existing guards or trusted
bootstrap/collector contracts already permit them.


The initial Id graph contains no upper registry clients. Runtime constructs those
clients from the completed protocol, and retains Id until its own state is gone.
Only isolated local harnesses own local registry backends. Production executable
retirement enumerates copied bindings before Id removes their dictionaries;
Store notification and managed-turn ownership remain upper responsibilities.

This removes production Runtime's Symbol/dictionary ownership, direct registry
access and raw-pointer entry into Id dispatch. The legacy all-Oop Method adapter
still recognizes core primitive function identities in the shared crate. Native
Method reflection, removing that adapter and the local fallback, native Execution
construction and the final package split remain required; this change does not
establish independent Id packaging.


Validation: all 311 Id unit tests pass (15 ignored). A fresh debug installation
passes the independent native-loading harness, including registry queries and
mutations before and after Runtime, rejection of all-Object raw mutations,
ordered views, weak opaque shadowing, exact selected contexts, and visitor
mutation/failure propagation. Its 68 installed Id exports match the existing
manifest plus statically linked artifact exports; the native carrier and Runtime v11 bridge are
unchanged.

Dispatch-cache and protected storage-observation scripts pass source, source-free
compiled, mapped/rebuilt image and baked execution, including host observations,
custom lookup effects, mutation, shadow removal, GC, aliases and failure cleanup.
The deferred-identity image suite passes with actual writes `(386, 150)` normally
and `(386, 75)` under forced recovery in all three image modes. No release
validation was run. The full construction/package acceptance remains open.


### Saved Id entries and explicit dispatch context

Id's `lookup`, `addMethod` and `removeMethod:` primitives now carry their own
Id dictionary reference in the existing `NativeEntry.context` field. A native
consumer can select one through a lookup message and invoke it after that message
returns, while retaining the Id owner. Previously these entries had null contexts
and consulted a hidden thread-local dispatch scope, so an otherwise valid saved
primitive could abort outside `message` or use the wrong active Id.

The fixed bridge now passes its four existing Id facts explicitly through binding,
nested lookup and DNU dispatch. There is no Id thread-local context, enter/leave
guard, or Runtime-owned dispatch scope. The first two bootstrap bindings carry
the already-created dictionary reference; subsequent bindings still use Id's
message protocol. No public carrier, selector inventory or native export changes.

The upper all-Oop bridge distinguishes an actual legacy function adapter from a
native function merely recognized by legacy reflection. It invokes native entries
with their selected context, adapting the old allocation/mutation operands and
representable method results at that boundary. Recognizing a native equivalent
no longer authorizes invoking a different function against the active Runtime.
Canonical lookup guards compare the complete primitive retained from completed
Id. Restoring a legacy primitive address recovers that original native entry,
including its context. The guard also rejects lookup replaced before Runtime
construction; capturing a replacement does not make it canonical. Installing a
legacy lookup wrapper in the primitive's place would
prevent Id queries from returning native entries without legacy equivalents.

This removes a hidden dispatch prerequisite and fixes invocation context. It does
not finish language Method ownership: the legacy representable method-result
convention still exists, arbitrary native entries still require a checked wrapper,
and the upper adapter still recognizes primitive function identities. Owned
callables, native Execution construction and the package split remain open.


Validation: 316 Id unit tests pass (15 ignored), including direct saved primitive
calls across distinct Id registries, invocation against the selected native
context while another Runtime is active, allocation without an ambient Runtime,
restoration of the complete primitive, and effectful lookup installed before
Runtime construction. The fresh debug installation passes the independent native
loading harness and the dispatch-cache source/compiled/mapped/rebuilt/baked suite,
including the host checks. All 68 installed Id exports match the existing manifest
and statically linked artifact exports. Runtime v11 and the native carrier ABI are unchanged.

The independent native Integer admission test also passes all eight replacement
paths with matching optimization-disabled/enabled effects. This covers allocator,
lookup, Namespace, interning, Symbol validation, owner/parent and arithmetic
replacements after the lookup guard changes.

The deferred-identity suite passes source, compiled, mapped/rebuilt image and baked
execution, including host observations and forced recovery. Actual writes remain
`(386, 150)` normally and `(386, 75)` under forced recovery in all image modes.
No release validation was run. The remaining Rust `InitialObjects` handoff,
legacy Method adaptation, native Execution construction and independent Id
packaging are still architectural work, not acceptance established by these tests.


### Runtime receives Id through its native carrier

Production Id now keeps its state for process life, like its pinned primitive
native image. There is no `PENDING` owner slot or `take_initial_objects` transfer.
The [Runtime v12 constructor](../../abi/runtime-construction-v12.md) takes the
completed native Id carrier explicitly, followed by the existing completed lower
owners. Runtime stores a copied `Protocol`; it neither owns nor inspects a boxed
Id `State`. The host passes the same carrier it received from Siphon.

The upper constructor obtains Object/Symbol prototypes, behaviors and lookup/DNU
selectors through Id messages. It roots each query result before another native
call, preserves the current collecting interner and keeps those temporary roots
through construction. This also removes a stale private Symbol-prototype pointer:
if the early unbound Symbol has been retired, Runtime uses the current interned
identity. Compatibility storage inspection obtains Id roots through the native
root operation without retaining a RuntimeState borrow across the call.

Tests use the same constructor through an opaque forwarding carrier whose context
has a different Rust layout. Their separately created Id instances are kept alive
by a test-only erased lifetime guard, dropped after upper state. Production has
no such guard and no private Id owner handoff. The legacy `id_init` entry still
obtains a carrier from the process slot before calling this constructor; it is a
compatibility path, not the production constructor's implicit dependency.

The actual Cargo/package split, native Execution/Block construction, owned Method
reflection and remaining temporary interfaces are still outstanding. In
particular, native primitive identities in the legacy adapter/compiler admission
and local test registry layouts still need to be separated; this change does not
claim that moving a Rust pointer alone completes Id independence.

Validation: 345 ABI/Id unit tests pass (15 ignored), including the opaque-carrier
constructor and early Symbol retirement cases. The fresh debug installation
builds 55 native libraries and 28 stage-1 modules. The independent native-loading
harness passes, including collection during Runtime construction, invocation of
saved primitives afterward, and C rejection of malformed v12 inputs before and
after initialization without consuming owners. All 68 installed Id exports match
the manifest and statically linked artifact exports. Release validation remains waived.

The same v12 installation also passes the deferred-identity image suite through
source, compiled, mapped/rebuilt and baked execution. Its independently compiled
host uses the matching v12 constructor. Actual writes remain `(386, 150)` normally
and `(386, 75)` under forced recovery in all image modes.

### Original Id entries cross the boundary as a completed dependency

Bootstrap retains the original allocation, interning, lookup, add/remove,
allocation-size, behavior and Symbol-byte entries immediately after Siphon loads
Id, before loading Bootstrap or any other upper module. The temporary
[Runtime v13 constructor](../../abi/runtime-construction-v13.md) receives this
[complete record](../../abi/id-primitives-v1.md). It replaces the allocation-only
record without changing the Id carrier, native selectors or Id initialization.

Runtime's lookup and Integer admission compare exact retained entries, including
contexts. The legacy bridge recognizes operand conventions from those entries
and invokes the selected function/context. It no longer refers to private Id
primitive functions. Capturing current lookup after upper preparation is not a
substitute: an earlier effectful replacement must not become the default merely
because Runtime sees it first. Test construction retains the initial entries
before allowing early mutations, matching the production order.

This removes private primitive-symbol dependencies from those consumers. It does
not remove the remaining local registry/layout variants, complete owned Method
reflection or separate the Runtime package. Those remain acceptance work.

Validation: 347 ABI/Id unit tests pass (15 ignored). A production-constructor test
uses a provider whose lookup/add/remove implementations share one forwarding
function with distinct opaque contexts; another confirms an earlier effectful
lookup replacement is not admitted as the original primitive. The fresh debug
installation builds 55 native libraries and 28 stage-1 modules. The independent
native-loading suite passes, including automatic collection during construction,
saved-context invocation and malformed-constructor rejection. Integer admission
preserves optimization-disabled/enabled effects for all eight replacement paths.
All 68 installed exports match the v13 manifest and statically linked artifact exports.

The v13 dispatch-cache image suite also passes source-free compiled, mapped,
rebuilt and baked execution, including the separately compiled host observations.
It preserves lookup effects, mutation, shadow removal, collection, aliases and
reload. No release validation was run.

### Private Id implementation and production-only native clients

The Id graph, allocation registry, Symbol registry, dictionaries and native
dispatch now live behind the private modules in
[`id/mod.rs`](../../rust/birnam-id/src/id/mod.rs). Ordinary upper code cannot name their
Rust implementation types. The sole Rust facade is the existing legacy `id_init`
carrier getter; production construction continues to receive its carrier and
original entries explicitly. Tests and explicit `ffi-test-hooks` diagnostics
retain only the internal access required by their local harnesses.

Runtime's local Heap/Symbol/dictionary variants and constructors no longer compile
into the ordinary library. Native mutation results and Symbol-validation errors
have upper-owned Rust representations. The scanner's borrowed payload coordinates
belong to Heap. The existing preceding behavior word, allocation alignment and
rounded extent calculations live in
[`object_layout`](../../rust/birnam-abi/src/object_layout.rs); no object layout,
native descriptor or exported symbol changes. These ABI facts do not expose Id's
allocation records or dictionary layout. Source links and executable measurement
inventories follow the moved production files.

This is an enforced source boundary, not independent Cargo packaging. Runtime,
compiler/artifact dependencies and the legacy exports are still built by the
same crate. Separating those libraries, finishing native Execution construction
and removing the compatibility initialization path remain required.

Validation: 347 ABI/Id unit tests pass (15 ignored), and the normal and explicit
diagnostic/guarded-slot configurations compile. The fresh debug installation
builds all 55 native libraries and 28 stage-1 modules. The independent native
loading suite passes before and after Runtime construction, including allocation,
collection and saved contexts. Storage observation passes source, compiled,
mapped/rebuilt and baked execution, including nested rejection, failure cleanup,
roots, aliases and reload. All 68 installed exports match the unchanged manifest
and wrappers. No release validation was run.


### Separate production Id and Runtime libraries

The production package boundary now follows the existing native carrier boundary.
`birnam-id` contains Id allocation, Symbols, dictionaries, fixed native dispatch and
native loading, with only `birnam-abi` as a Cargo dependency. Its complete normal
export inventory is `birnam_native_module_v1` and `birnam_id_floor_identity`.
`birnam-runtime` contains the upper Runtime, Machine, collector integration and
compiler/artifact dependencies. Ordinary Runtime has no Rust dependency on Id;
only explicit diagnostic builds and unit tests compile the local Id test backend.

The installer publishes both dylibs. The host no longer looks for `id_init` or an
upper Runtime constructor in Id. After completed native predecessors exist, it
maps Runtime for its six remaining legacy factory preparations, then requests
Runtime's actual native initialization through Bootstrap and Id. Mapping that
upper image is still temporary construction machinery: it does not establish the
Execution base/compiler extension required by section 4. Eleven obsolete factory
preparations and their retained-handle fields are removed from the host.

The installed native-loading fixture audits both complete symbol inventories and
Id's system-only dynamic dependencies. Its isolated directory contains only the
actual Id dylib before upper construction. Independent C checks that Id has no
Runtime/legacy constructor exports; a separate C consumer checks Runtime's direct
and native-entry malformed constructor rejection before adoption. Existing early
allocation, dispatch, rooting, collector and saved-entry checks continue against
the same installed production libraries.

Validation: 352 ABI/Id/Runtime unit tests pass (15 ignored), 90 host/installer
unit tests pass (one ignored), and normal plus explicit diagnostic/guarded-slot
configurations compile. A debug installation builds 56 native libraries and 28
stage-1 modules; the complete independent native-loading suite passes. The final
installation also passes storage observation in source, source-free compiled,
mapped/rebuilt image and baked execution, including the real host's nested
rejection, failure cleanup, rooting and alias checks. Release validation remains
waived.

Remaining work includes the Execution base/compiler extension, owned Method
consumers, the six upper preparations, predecessor-manifest/diagram reconciliation
and worker acceptance. Older C/diagnostic drivers that still initialize the former
aggregate dylib through `id_init` or use `BIRNAM_ID_DYLIB` to replace Runtime need
migration to the new boundary; their previous results do not validate this split.
The older `compatibility-authority-test.py` currently fails its reviewed-image
comparison. Running the committed HEAD checker against committed HEAD image source
reproduces that failure; the split does not change image encoding or broaden the
checker to accept it. That comparison remains an open validation issue.

Earlier evidence sections above record intermediate states and their then-current
export counts. Their two additional artifact exports came from statically linked
`birnam-artifact`, not installer-generated wrappers.


### Runtime receives completed supporting factories

The final six host preparations have been removed. Method Observation, Storage
and Dispatch Cache now publish their existing complete factories through Id's
native loading entry, using the explicit [factory requests](../../abi/runtime-support-v1.md).
`FactoriesV1` carries the copied operations into Runtime v14; Heap, observation,
cache and guard consumers receive their actual operations during construction.
Their ordinary constructors no longer consult global factory slots. Normal Runtime
no longer exports the six dependency setters or the v13 constructor. Explicit
legacy diagnostic construction remains separate from the production path.

The host opens Runtime only through the real Id loader after all construction
arguments exist, then acquires its lookup handle after initialization. The three
empty legacy module entries and their `SCRIPT_MODULES` rows are removed. Their
replacement entries are the six named native requests in the factory document;
no language methods or objects were published by those removed rows.

This completes the supporting-factory migration, not the Execution/compiler
separation. Runtime still includes artifact/optimization policy, and the full
entry/predecessor manifest, owned Method migration, older diagnostic drivers and
worker acceptance remain required.


Validation: 363 ABI/Runtime/support-library unit tests pass (15 ignored), and
normal and explicit diagnostic/guarded-slot builds compile. The fresh debug
installation builds 56 native libraries and 28 authored stage-1 modules. The
independent native suite validates the exact installed export inventories,
exercises all six factories with Runtime unavailable, rejects malformed native
requests and eight malformed factory tables with otherwise valid predecessors,
then constructs Runtime from those same predecessors. Existing early-object,
collector, saved-entry and post-adoption checks also pass. Release validation
remains waived.

The final host/installer suite passes 90 tests (one ignored). The installed
storage-observation suite passes dynamic source, source-free compiled,
mapped/rebuilt native Code images and baked execution, including independent
host observations, nested rejection, failure cleanup, roots, aliases and reload.


### Native values and Block precede parser/compiler modules

Runtime now constructs `Block` and `BlockVt`, with Oop capture layout and the
ordinary behavior owner, alongside its execution objects. A native module can
create a concrete delegated Block behavior, install its native `value` method
and allocate captures without Prelude or a source compiler. Block Loop's own
`module_init` installs its complete native while methods using the existing
Block and Error Value; Prelude no longer constructs Block or attaches that
installer. Printing also installs its own Object/Symbol defaults before
collections; StringBuilder conversion and collection printing do not depend on
Prelude. Boolean's Block-taking controls, Nil/String/StringBuilder, collection
value operations, Vector/Range/Map/String collection composition, Error and Store
now load before Artifact, PEG, Lang, TCC and Codegen. These later modules receive
actual published values instead of relying on future initialization.

The installed native-loading suite now has a prefix containing the real shipped
Runtime and value modules, with Artifact, PEG, Lang, TCC, Codegen, Prelude,
Portable Code and Image libraries absent. The actual host startup must reach the
missing Artifact entry, after which an independent C consumer exercises native
Block capture/calling and precise collection, Boolean control, Vector callbacks,
Map, String splitting, Object/Symbol/collection/StringBuilder printing, loop error
propagation, Globals and basic Machine behavior.
The same check verifies Block identity survives Block Loop reload. Separate
copies of the shipped Block Loop image reenter first initialization during
selector interning and registration with another API table; execution retains
the original table selected before either callback.

This establishes the native value/control prefix. Runtime still physically links
compiler/artifact implementation and exposes its old admission machinery;
separating that advertised compiler extension remains required. The legacy value
entry/provider construction and full checked predecessor manifest also remain
open. The new prefix fixture does not establish completion of those boundaries.


Validation: 319 Runtime and 90 host/installer unit tests pass (16 ignored), plus
Prelude's identity test. The final debug installation builds all 56 native
libraries and 28 stage-1 modules. Its full native-loading suite passes, including
the prefix's printing/conversion cases, precise GC and both first-initialization
reentry cases. The printing source/compiled/native-Code-image/baked suite passes
on that installation. Block Loop source/compiled/native-Code-image/baked tests
pass after its installation and reentry changes; the broader storage-observation
source/compiled/mapped/rebuilt/baked suite also passes after the Block and value
ordering changes. Release validation remains waived.


### Execution base and explicit compiler extension

Runtime v15 receives the already constructed Integer, Float, Word and String
APIs before it publishes Machine/Block or installs collector extensions. These
are mandatory constructor inputs; legacy value entries reuse them. Machine's
Integer-valued queries work immediately after construction, before those legacy
entries run.

Artifact admission/catalogs and speculative native/numeric registries now belong
to an optional compiler extension. Production constructs it through Id with
`Bytes("compiler"), Word(Runtime API)` after Store, Artifact, PEG, Lang and TCC,
immediately before Codegen. Its provider returns null beforehand. Codegen
requires the completed extension and resolves compiler operations through its
[descriptor](../../abi/runtime-compiler-v1.md), without raw-export fallback.
Native Execution, Block and collecting value/control operations work while the
extension is absent. Both entries share a library, as section 4 permits.

This change does not finish owned Method reflection, the entry-level manifest,
legacy driver migration or the outstanding full image/worker acceptance. Those
remain part of Designs 82/83, not follow-up scope outside this implementation.


Validation for this change uses `/tmp/birnam-83-compiler-extension`, a fresh debug
installation with 56 native libraries and 28 compiled stage-1 modules. Runtime's
319 unit tests, host/installer's 90 tests and Codegen's 22 tests pass (20 tests
remain explicitly ignored across those suites). The installed native-loading
suite passes, including direct constructor rejection of incomplete value inputs,
immediate Machine queries, compiler-provider absence, foreign-carrier rejection,
complete extension resolution and repeated construction. The printing source,
source-free compiled, native-Code image and baked-executable checks also pass.
The unit suites use a separate unwind build directory from native harness builds;
release validation remains waived.

Block Loop's source/compiled/native-Code-image/baked checks pass on the same
installation, preserving captured blocks/cells, aliases, errors, GC and live
method/factory replacement. The storage-observation suite also passes across
source, compiled, mapped, rebuilt and baked execution, including host-side BUSY
checks, nested rejection, visitor failure cleanup, roots and allocation identity.
The diagnostic `ffi-test-hooks` Runtime configuration still type-checks.

### Owned Methods and anonymous compiled Units

Language reflection now returns an owned Method, preserving the exact native
function/context pair. `Object methodFor:`, `Vtable methodFor:`,
`Method invokeOn:`, `invokeOn:arguments:` and `Vtable addMethod:as:` use this
representation. TCC and Portable Code select the completed checked language-send
entry during construction. Invalid language lookup results become errors rather
than executable addresses. The unsafe legacy `IdApi.send` convention remains a
migration boundary. See [the language contract](../../abi/language-send-v1.md).

A Method retains its TCC Unit in an ordinary traced slot when its function comes
from anonymous compilation. Registered resumable functions additionally retain
their generation lease; other native functions retain their mapped image.
Native authors still own arbitrary native context memory and its object roots.
Method invocation roots the Method, receiver and arguments across callbacks.
Image saving rejects process-local Methods, including those with private behavior,
as `methodNotImageable`.

TCC and Codegen select completed register/forget operations from Runtime before
publishing their interfaces. TCC registers each generated address and owning Unit
before publishing bindings, and forgets weak address records before finalization.
Portable synchronous nested functions and resumable thunks use the same path.
Retaining the Unit is necessary because generated nested Block constructors read
that object as well as execute its native code.

Ordinary source-written Blocks use this path too. TCC fills a generated Unit
reference before publishing relocated code, and generated Block installation
registers the function with that Unit. Source evaluation no longer permanently
retains every Unit containing a Block. It roots active evaluation and outward
results, uses a Unit-owned cache, and registers normal finalization after
evaluation. Surviving Blocks, Methods and bindings determine the Unit's lifetime.
The separate-turn lifetime check covers this source path as well.

[Lifetime v2](../../abi/fixtures/lifetime-v2/README.md) adds native references
indexed by source object and a native key. Runtime uses behavior/selector keys
to retain a bound function's Unit. Trace visits these references only from live
source objects, allowing unreachable Unit/behavior cycles to collect. Removing
or replacing a binding takes effect immediately and retains the previous Unit
until the outer turn returns. This covers lookup that sends more messages or
triggers collection before the original dispatch finishes. Id has no new fields,
dependencies or executable-lifetime policy.

Synchronous portable Block entries validate arity, payload extent and traced
capture slots before reading captures. A String with enough bytes is not a valid
capture vector. No native pointer validation or sandboxing is introduced.

Acceptance uses the normal production installer and independent native-loading
fixtures. `method-values-smoke.sh` covers source, source-free compiled modules,
mapped/rebuilt images and baked executables, including live replacement/removal,
aliases, callback GC, malformed lookup results, capture checks and image rejection.
`method-unit-lifetime-smoke.sh` checks synchronous/resumable reflection and alias
survival after eviction/GC, followed by eventual Unit release across outer turns.
The native collector fixture additionally checks retention and collection of a
native-reference cycle before Runtime is present. These are production paths,
not a separate bootstrap experiment.

Remaining Designs 82/83 work includes unsafe native consumers and legacy
constructors, the complete entry/predecessor manifest, migration of diagnostic
drivers, worker/image acceptance and the final requirement-by-requirement audit.
The ownership work does not establish completion of those requirements.

The expanded ownership checks use the debug installation
`/tmp/birnam-83-method-ownership`. Separate REPL turns prove eventual native Unit
release for source, synchronous and resumable Blocks. Method source/compiled/
mapped/rebuilt/baked checks pass, including source-written Block reflection and
wrong-arity invocation. ABI, Runtime, Codegen, Codegen Runtime, TCC and Trace
unit checks pass; the normal installed Portable Code runner reports 114 assertions.
The native prefix suite covers Lifetime v2 before Runtime and its later adoption.
Storage observation also passes source, compiled, mapped, rebuilt and baked
execution, including host observation, nested BUSY rejection, visitor cleanup,
root preservation and allocation identity after GC. Release validation is waived.

The forced-source Portable Code runner exposed a generated-reference lifetime
bug: compiled `Test` answered nil after the nested framework load, while a fresh
`[Globals at: #Test]` found the object and executed all 114 assertions. The emitter
had embedded an unrooted Symbol address in each global reference, then the lookup
shim converted that Symbol back to a name. Collection during nested loading could
retire those Symbols before their generated references were used.

Generated global reads now call the existing name-based lookup directly. JIT
Symbol binding, per-artifact Symbol storage/initialization and the emitter's
baked-global lists are removed. The artifact initializer symbol remains because
candidate admission requires it. Existing `birnam_global_b` exports remain for
old artifact ABI consumers; newly emitted code does not use them. This removes
an ownership obligation rather than adding another native root table.

`scripts/source-global-lookup-smoke.sh` runs the Portable Code suite through
installed, forced-source and compiled entry paths and checks both the reported
assertion total and individual pass lines. Its forced-source check rejects the
previous `d9f61666` installation's silent zero exit. Broader worker and
image-authority acceptance remain outstanding.

The debug installation `/tmp/birnam-83-live-globals` passes all three modes
(114 assertions each), Method source/compiled/mapped/rebuilt/baked checks and
the separate source/synchronous/resumable Unit-reclamation checks. Codegen passes
97 unit tests; Codegen Runtime passes 22 with four existing ignored tests;
validation tooling passes eight. Its Id library is byte-identical to the
previous ownership installation used for the independent native-prefix checks.

The worker service check still fails its ten-second readiness deadline. Profiling
the real child shows repeated selection-table decoding during authored-module
admission; a later sample shows the host recompiling image code after loading
the image. Generation authority now indexes admitted named-method rows once per
calculation, preserving the existing independent digest fixture and validation.
This removes quadratic decoding, but does not by itself establish worker
acceptance or remove the host's additional image-recovery compilation.

On `/tmp/birnam-83-generation-index`, Codegen Runtime's 22 unit tests pass
(four remain ignored), including the frozen dispatch-base digest. The independent
image-worker reader verifies twelve replies and orderly shutdown on both the
previous and updated installations, with byte-identical transcripts. One serial
comparison observes readiness at 43.63 and 42.21 seconds respectively; this is
diagnostic timing, not a performance acceptance claim. The production service
suite remains blocked by its unchanged ten-second readiness limit.

### Bootstrap owns construction through Runtime

The production sequence from roots through Runtime now lives in
[`birnam-bootstrap/src/construction.rs`](../../rust/birnam-bootstrap/src/construction.rs).
Bootstrap captures Id's original entries, loads the completed predecessor modules
through the actual Id carrier, adopts roots, activates collection and supplies
Runtime's constructor inputs. The host's `InitialFloor` methods now delegate to
this constructor rather than reimplementing its steps.

Construction is selected through Bootstrap's existing common native descriptor
and returns a native entry retained by the Bootstrap owner. Its requests and
result kinds are specified in the [Bootstrap contract](../../abi/fixtures/bootstrap-v2/README.md).
The 64-byte owner ABI is unchanged. A constructor request removes its mutable
construction state from the shared owner until it returns; nested construction
fails, while ordinary nested module loads retain their existing behavior. Prefix
diagnostics stop this same production constructor before successors are available.

The host still loads later legacy modules and discovers Runtime's aggregate
capabilities. Their migration, the full predecessor manifest, worker/image
acceptance and the final architecture audit remain required. Moving this sequence
does not establish that those later modules satisfy the target construction rule.

Validation on `/tmp/birnam-83-bootstrap-construction` passes the independent native
prefix suite, all three global-lookup modes, and Method source/compiled/mapped/
rebuilt/baked checks. Host unit tests pass 86 cases with one existing ignored
case. The installed constructor and host sources match the tested worktree.
The native fixture directly obtains Bootstrap's constructor, checks missing
successors and rejects graph advancement from inside an initializer.

### Remove repeated lower initialization

The upper host sequence no longer invokes `module_init` on completed Namespace,
Float, Word, Object Model, Root Registry, GC Settings, Payload Layout, Trace Hooks,
Finalizers, Native Stack, Native Mark, Allocation Budget, Nil or String modules.
Their entries and values are already available from Bootstrap's construction.
`LoadedFloor` carries the completed value descriptors; `LoadedStack` uses them
immediately. The host no longer has a second Float/Word/String preparation path
or prepared-library handles.

Integer and Boolean control extensions remain in the upper sequence because they
install behavior requiring Runtime's Block representation. Trace/Collect legacy
provider attachment and other upper module entries also remain pending migration.
The compiler-free prefix fixture removes the fourteen completed library paths
after Runtime construction, then exercises the real remaining startup sequence.
It also checks the number of legacy initializations, since the platform loader
can reuse a mapped library even after its file is moved. The previous host
executes thirty initializations before Artifact; the updated host executes
sixteen. The same fixture rejects the previous host at that check and passes
with the updated host, including its execution, collection and Store assertions.

Validation on `/tmp/birnam-83-no-repeat` passes installed/forced-source/compiled
global lookup and Method source/compiled/mapped/rebuilt/baked execution with image
rejection. Host unit tests pass 86 cases with one existing ignored case. The
native prefix suite passes with the rebuilt host and the preceding installation's
native libraries, all 56 of which are byte-identical to the new installation.
The installed host source matches the worktree used for these checks.

### Native control construction above Runtime

Bootstrap now constructs the complete Error helper, Integer iteration,
Object/Symbol printing, Block loops and Boolean control group. Their common
native constructors take explicit completed inputs; the exact operands are in
[Bootstrap v2](../../abi/fixtures/bootstrap-v2/README.md). Integer's control entry
adds only `to:do:`; Boolean's adds only Block-taking branches and short-circuit
operations. Base arithmetic, predicates and printing remain the lower modules'
existing behavior. No compiler service is installed by these entries.

The host requests Bootstrap's `control` stage before collections and receives
its completed Error/printing/loop descriptors for later legacy consumers. The
five corresponding `SCRIPT_MODULES` entries are removed. Runtime supplies its
existing namespace presence/import views through named common-descriptor
requests; Bootstrap does not probe private exported symbols to find them.

The compiler-free native fixture now withholds collection and Store libraries,
constructs this group with zero legacy module initializations, and exercises
iteration, branches, printing, loops, errors, captures and collection. It then
restores those later libraries and continues the real production sequence.
Upper collection/Store/compiler/system construction and checked-dispatch
consumer migration remain required.

Validation on `/tmp/birnam-83-control` passes the native prefix suite, including
absence of control selectors before construction, missing collection/Store
libraries, and nested native Block Loop initialization from interning and method
publication. Repeated completed construction returns the same descriptor.
Installed, forced-source and compiled global lookup pass; Method source,
compiled, mapped, rebuilt, baked and image-rejection checks pass. Host unit tests
pass 86 cases with one existing ignored case; Integer's storage test also passes.
All eleven changed Rust sources match the installed copies. Id's library is
byte-identical to `/tmp/birnam-83-no-repeat`.

### Complete native collection and Store construction

Bootstrap now builds the Collection base, StringBuilder, value policy, Vector,
Range, Map, String collection extension and Store through common native entries.
Each family is callable when its constructor returns; the native path no longer
returns an unprepared family factory for a later Collection-wide installation.
Vector precedes its consumers. Its exact concatenation NativeEntry is passed to
Range with its context intact, without another Vector method installation.

StringBuilder publishes under its own namespace guard instead of depending on
Vector's absence. Production now constructs Vector and its consumers before
StringBuilder, exercising that independence in ordinary startup. Native
construction keeps temporary roots for new objects and selectors until permanent roots are registered, including Store's allocation and
interning sequence. Object initialization results transfer to the module's
registered roots before Bootstrap releases its temporary result root.

The eight corresponding legacy host entries are removed. The host receives the
completed collection descriptors and Store compensation service from Bootstrap.
Runtime supplies its existing namespace-export, vtable-enumeration and
method-observation views through named native requests. The exact predecessor
operands and advertised results are recorded in [Bootstrap v2](../../abi/fixtures/bootstrap-v2/README.md).

The prefix fixture constructs control without collections, collections without
Store, and Store without any parser/compiler library. Each construction phase
uses zero legacy module initializations. Later compiler/system construction,
checked-dispatch consumers, the full manifest and final architecture acceptance
remain required.

Validation on `/tmp/birnam-83-collections-final` passes the native prefix suite,
including collections with Store absent and Store with compiler libraries absent.
Installed, forced-source and compiled global lookup pass. Method source, compiled,
mapped, rebuilt, baked and image-rejection checks pass. Unit tests pass 86 host,
11 Store and four Vector cases; one host and two Store cases remain ignored.
All twelve changed Rust sources match the final installed copies. Id's dependency
and export checks pass. Release validation remains waived; worker readiness and
the remaining architecture acceptance are still open.

### Complete native parser construction

Bootstrap now constructs Artifact, PEG and Lang through their common native
entries, with complete Integer/String/Vector and Lifetime inputs. Lang receives
the completed PEG object and builds the real Birnam grammar with message sends
before returning its facade. Artifact and PEG do not require Store themselves;
the full host sequence completes Store before requesting this parser group.
The three legacy host rows are removed. The precise entry operands and results
are recorded in [Bootstrap v2](../../abi/fixtures/bootstrap-v2/README.md).

New facade/behavior objects retain temporary constructor roots until permanent
roots are installed. Repeated native construction returns the completed result
and does not rebuild Lang's grammar. The parser prefix test adds the three
libraries only after proving the lower prefix works without them, then exercises
parsing, CST collection results, syntax errors and Artifact failure reporting
with TCC and Codegen absent and Runtime compiler admission still inactive.

TCC's `bindRuntime` currently publishes generated-code registration shims that
probe Runtime symbols. Codegen similarly selects compiler admission privately,
and the host supplies Store compensation only after Codegen's initializer has
returned. These are the next concrete construction dependencies to remove.
Runtime's `initialize_compiler` currently requires completed values, so it can
precede TCC; TCC and Codegen can then receive its completed admission descriptor.
Codegen must receive Store compensation before publishing its operations.
This ordering still needs implementation and prefix validation. System
construction, checked-dispatch consumers, the full manifest and worker/image
acceptance also remain required.

Validation on `/tmp/birnam-83-parsers-final` passes the native prefix suite,
including real parsing before TCC/Codegen and compiler admission. Installed,
forced-source and compiled global lookup pass; Method source, compiled, mapped,
rebuilt, baked and image-rejection checks pass. Unit tests pass 59 Artifact,
86 host, one Lang and four PEG cases, with two existing ignored cases. All five
changed Rust sources match the installed copies. The fifteen checked native
export sets match their manifests, including nine corrected control/collection
manifests. Release validation remains waived; worker readiness remains open.

### Compiler dependencies completed before publication

Bootstrap now constructs Runtime admission, TCC, Portable Code and Codegen
through common native entries. Admission's already complete values are its
predecessors; no TCC/Codegen provider slot is installed into Runtime. TCC and
Codegen receive the admission descriptor directly, and their native compiler
symbol resolution retains its context instead of probing the Runtime provider.

Portable Code previously followed Codegen even though Codegen's portable
operations imported its native APIs. Portable Code itself needs only completed
values/execution, so its representation and methods now precede Codegen. This
removes that forward dependency without making a partially usable Codegen.
Codegen also receives Store compensation before it publishes any methods, plus
completed Tcc, Store, Globals, Lang and CodeObject predecessors. The host's late
Store configuration and the three legacy module rows are removed. Exact
operands/results are recorded in [Bootstrap v2](../../abi/fixtures/bootstrap-v2/README.md).

The native prefix fixture is extended to compile C, evaluate Birnam and invoke
a generated closure after GC with System/Image libraries absent. Ordinary source
and image validation must still cover the full production stack. The native
constructors preserve the existing language protocols and TCC override path.
Legacy direct entry points remain for existing native consumers; non-compiler
private entry lookup and unchecked-dispatch consumers are still unfinished.
System construction, the complete manifest, worker readiness and final
requirement-by-requirement acceptance remain open.

The compiler prefix exposed an additional implicit dependency: JIT Blocks used
Codegen's exported `birnam_is_error` without binding it into their TCC Unit.
Legacy loading made that symbol globally visible; Id's native loader correctly
keeps modules local. Codegen now explicitly binds its complete helper set,
including canonical and Rust spellings, into each generated Unit. Unit-owned
vtable caches bind both spellings to the same Unit implementation. TCC alone
supplies the resumable-entry bridge, removing its duplicate binding. Compiler
errors now distinguish compilation, global binding, Runtime binding, relocation
and generation-identity failures. The regression check compiles a captured Block
and invokes it after GC with no System/Image initialization.

Compiled Mach-O artifacts have a separate linking requirement: the existing
format imports the public `birnam_*` C ABI. TCC supplies an exact native action
after construction; Codegen receives that action with its context as a final
constructor operand. Before mapping an artifact, Codegen invokes the action
and exposes its own C ABI. Both operations retain `NOW | GLOBAL | NOLOAD`
references to the already completed images identified by their own function
addresses. They neither initialize modules again nor discover future providers.
This upper artifact-linking step preserves the existing format while ordinary
native loading and JIT Units remain local. Id has no artifact-specific behavior.

Validation on `/tmp/birnam-83-compiler-linked` passes the native loading suite,
including C compilation, captured-Block execution after GC and the TCC override
with System/Image libraries absent. Installed, forced-source and compiled global
lookup pass. Owned Methods pass source, compiled, mapped, rebuilt, baked and
image-rejection checks; portable binding publication, replacement, self-removal
and retirement also pass. ABI/TCC/Codegen unit tests pass 52 cases, with six
existing ignored cases. All ten changed Rust sources match the installed copies;
the sixteen checked native export sets and Codegen install name match their
manifests. Release validation remains waived. System/application construction,
legacy consumer removal, the complete construction manifest, worker readiness
and final architecture acceptance remain open.

### Complete System operation construction

Prelude's common native entry now constructs only the rooted System object.
The System GC, IO, environment, file and loader modules install their own
operations during native construction. Text View receives completed String
storage; Source Loader receives completed Codegen. Bootstrap owns this sequence,
and nine more legacy host rows are removed. The IO prefix requires neither
collections/Store nor parser/compiler libraries; `load:` is absent until Codegen
exists. The exact inputs and stage results are in
[Bootstrap v2](../../abi/fixtures/bootstrap-v2/README.md).

This construction change preserves the existing System language protocols.
`loadModule:` itself still invokes the old native-module ABI, and legacy direct
consumer entry points remain. Their migration, application constructors, the
complete manifest and final acceptance are still required.

Validation on `/tmp/birnam-83-system` passes the complete native suite, including
System IO with collections, Store and parser/compiler libraries physically
unavailable, absent source loading, real file/environment operations, GC,
native-load failure and repeated construction. Installed, forced-source and
compiled global lookup pass. System output and source-loader startup, compiled,
mapped-image and baked execution pass with precise GC, aliases, retained values,
live method/factory replacement and error cases. Selected unit tests pass 92
cases with two existing ignored cases. All eleven changed Rust sources match
the installation; all twenty-four checked native export sets match their
manifests. Application constructors, legacy consumers, the complete manifest,
worker readiness and final architecture acceptance remain open.


### Application construction and manifest (2026-09-21)

Isolate, Relational, Alien and Image now use the common native module descriptor
with completed predecessors. Bootstrap owns their ordering and constructor roots;
Image receives compiler admission explicitly. Normal startup no longer contains
`SCRIPT_MODULES` or calls any legacy native initializer. The final Trace, Collect
and Error compatibility rows were redundant and are removed with the four
application rows. Direct consumers of legacy native entries remain unfinished.

The full entry inventory is in `abi/production-construction-v1.json`, with exact
application operands documented in `abi/fixtures/bootstrap-v2/README.md`. The
installer rejects missing predecessors, cycles and library inventory mismatch.
Those checks establish manifest structure, not the safety of remaining legacy
consumers or worker readiness. Validation uses the fresh debug prefix `/tmp/birnam-83-applications-final`:

- `scripts/id-native-loading-smoke.py`: passed, including application operations
  before Image/authored modules, repeated Image identity and zero legacy native
  initializations through full startup. All earlier prefix/GC checks also pass.
- `scripts/source-global-lookup-smoke.sh`: installed, forced-source and compiled
  modes execute the full Portable Code assertions.
- `scripts/method-values-smoke.sh`: source, source-free compiled modules,
  mapped/rebuilt images and baked execution pass, including exact entries,
  replacement/removal, aliases, collection and Method image rejection.
- `scripts/isolate-image-wire-test.py`: twelve exact worker wire round trips and
  orderly stop pass. This external reader allows a longer startup wait; it does
  **not** establish the production parent's ten-second readiness requirement.
- Affected package tests: 163 passed, four ignored. Runtime tests: 322 passed,
  fifteen ignored. The separately invoked relational output matrix passes its
  120 cases after removing its private-provider/`id_init` setup and using the
  production constructors instead.
- All nine changed production Rust sources match the installed copies byte for
  byte; the construction manifest hash matches the install receipt. The later
  relational test-only migration was compiled against that same installation.

Removing the last legacy load exposed a hidden policy transition: Runtime kept
all Symbols alive until `configure_symbol_borrow` ran for a legacy module. Native
Runtime now enables weak Symbols when constructed with its completed collector.
The native fixture checks this before upper startup and verifies early Error,
Symbol and String reclamation afterward. Diagnostic legacy construction retains
its existing compatibility default; explicit legacy admission can still request
bounded compatibility retention.

Evidence is recorded in `.cargo-target/83-applications-evidence.json`. Release
validation remains waived. Remaining work is legacy native consumers (including
`System loadModule:`), unchecked native sends/reflection, compatibility exports
and aggregate capability discovery, worker readiness, documentation/diagram
consolidation and the complete requirement-by-requirement acceptance audit.


### One checked send for native and generated consumers

`IdApi.send` now uses the same checked entry as `birnam_runtime_send_v1`.
This moves the host and all native module message consumers onto managed turns
with owned reflection results, instead of retaining the raw Method convention
for their calls. `lookup` and `removeMethod:` answer Method or nil, and a raw
function address passed to `addMethod` is rejected. Trusted native C registration
continues through `IdApi.method`; Id's typed native carrier remains independent.

TCC and Portable Code use the send supplied by the completed Runtime API. The
private `dladdr`/`dlopen`/`dlsym` selection of a separate language-send entry is
removed. Runtime's own constructor uses typed Id operands for metacircular method
installation. `send_legacy_abi` and its argument/result-policy branches are gone;
the primitive constructor test now exercises the native protocol directly.

This does not finish C lookup adapter removal or legacy `module_init` migration.
Those remaining native boundaries must be audited separately; changing the send
entry does not justify claiming all unsafe native interfaces have disappeared.

Validation uses `/tmp/birnam-83-checked-native-final`: 324 affected Rust tests
pass (17 ignored), the native suite passes checked reflection and raw-address
rejection, and global lookup passes installed/source/compiled modes. Method
values pass source, source-free compiled, mapped/rebuilt and baked execution,
including image rejection. The external worker reader reports twelve exact
wire round trips and orderly stop; readiness is 44.92 seconds, so the production
ten-second readiness requirement remains open. The Methods and wire process
handles are now closed; their logs contain their final success markers.

All eight changed production Rust sources match that installation byte for byte.
The later test-only guard-cache assertion now expects typed Id construction to
leave the upper lookup cache cold; the full affected unit suite passed afterward.
Evidence is in `.cargo-target/83-checked-send-evidence.json`. Remaining C fixtures
that cast lookup answers to function pointers require migration; these checks
do not establish whole-project acceptance. Release validation remains waived.


### System native loading through Id

The production System wrapper now calls Id's loading message with a byte path
and explicit Object operands. `loadModule:` supplies none;
`loadModule:arguments:` reads a sequence via `size` and zero-based `at:`. This
requires Integer but no collection implementation. The wrapper copies the path
and roots the sequence and each selected argument across effectful sends and
initialization. Object results are returned; Unit returns System; native-only
results and load failures become Errors. See [the contract](../../abi/native-loading.md).

Native Loader no longer exports `module_init`, a provider, an installer, or
private dependency attachment entries. Its constructor returns completed System.
Prelude no longer reinstalls its operations through the retired installer. The
host legacy loader, its aggregate dependency discovery, private Symbol
compatibility configuration and obsolete error variants are removed. Existing
C test consumers still need migration; this change does not claim their acceptance.

The old host module counter is removed with its only caller path; zero on a
deleted path is not a useful test. The native suite now audits the absence of
aggregate initializer calls in the production host and System sources. Existing
deferred-index/identity host fixtures now request their five actual dependencies
through the common descriptor. Their raw lookup bodies and older language
loading scripts still need migration before those full suites can pass.

Validation on `/tmp/birnam-83-id-module-loader-final`:

- The complete native suite passes, including loading before collections and
  compilers, explicit argument Objects retained across GC, Unit/Object results,
  malformed sequence size, native-only result rejection, failed initialization,
  missing/incompatible descriptors and no fallback to `module_init`. The twenty-four
  native export inventories match; Native Loader exports only its descriptor.
- The migrated native-loader image suite passes startup, source-free compiled
  execution, mapped image and baked execution. It retains repeated initialization,
  receiver/alias identity, GC, errors and live method/Error-factory replacement.
- Affected Rust tests: 91 passed, two ignored. The removed test covered only the
  deleted host Symbol compatibility-limit parser. Both deferred-index/identity C
  constructors and Rust image drivers compile against the explicit native API;
  this is not execution acceptance for their remaining old reflection bodies.
- All five changed production Rust sources and the relational test source match
  the final installation byte for byte. The deleted Native Loader installer
  module is absent from that installation too.

The evidence receipt is `.cargo-target/83-native-loader-evidence.json`. Remaining
work includes the old C consumers and internal raw lookup adapters, obsolete
module exports/factories, worker readiness, and the complete Design 82/83
acceptance audit and documentation consolidation. Release validation is waived.


### Remove raw C lookup results

Object-valued C lookup callbacks now return owned Method or nil, just like
language lookup. The upper bridge extracts the exact typed entry from that
Method. The address-recovery stack, nested result scopes, context-ambiguity
heuristic and raw function-address fallback are removed. Native methods using
the Object ABI accept only Object operands; callers requiring typed native
operands use Id's protocol. The bridge between the two calling conventions
remains necessary, but does not invent or recover missing callable ownership.

Runtime construction uses Id's original typed mutation entries until Runtime
can install completed lifetime-aware operations at its stable address. The
temporary pointer-encoded add/remove wrappers and their bootstrap replacement
pass are removed. Execution constructs Method representation before accepting
effectful language lookup. Minimal dispatch fixtures instead invoke Id's actual
typed lookup and require no Method/Execution construction. Diagnostic lookup
callbacks are migrated to the same owned-result rule as production.

Validation uses `/tmp/birnam-83-owned-lookup`: 323 Runtime tests pass (15
ignored), including nested lookup with replacement/GC, exact contexts for the
same native function and raw-address rejection. The cache lifetime witness now
roots a Method before removing its binding, proves that it survives release of
a separate generation lease, then drops it and checks retirement after GC.

The complete native suite passes, including a foreign C lookup that performs a
nested send and collection before returning a Method, and returns an Error
without invoking a raw-address answer. Source globals pass installed/source/compiled
modes. Method values pass source, source-free compiled modules, mapped/rebuilt
images and baked execution, with replacement, aliases, GC and image rejection.
All six changed production Rust sources and both changed Rust test files match
the installation byte for byte. Evidence: `.cargo-target/83-owned-lookup-evidence.json`.

Internal C-address aliases for typed allocation/delegation, old native exports
and direct C fixture consumers still require cleanup. Worker readiness and the
complete acceptance/documentation audit remain open. Release validation is waived.


### Host service selection through completed native construction

`InitialFloor::finish` now requests its eleven completed Runtime services through
Bootstrap. Bootstrap selects each service by loading Runtime through Id with
`Bytes(name), Word(runtime_api)`, using the same native entry as later module
construction. The host no longer discovers these services with `dlsym`.
Runtime returns its existing live descriptors, preserving their contexts,
admission callbacks and ownership. No new owner or second service instance is
introduced, and Id has no knowledge of the service names.

The twelve private provider exports for namespace presence/import/export/names,
vtable enumeration, method observation, roots, GC settings, payload layouts,
trace hooks, finalizers and native stack are removed. The export inventory and
construction manifest describe the resulting interface; the latter now includes
six additional completed-service queries. Runtime's handle remains in the host
for separate execution/image entry points, which still require cleanup.

The ABI overview and Runtime construction contract document the current service
requests. The older capability pages explicitly mark their pre-Id construction
and provider accounts as historical. Unmigrated direct C fixture harnesses still
refer to retired APIs; they are not a supported production loading path and
remain part of the outstanding fixture migration/retirement audit.

Validation against `/tmp/birnam-83-completed-services` passes: the debug install,
85 host tests (one ignored), the complete native loading suite and native-loader
image checks across startup, source-free compiled loading, mapped images and
baked execution. Installed export inventories confirm the providers are absent.
All three changed production Rust files match installed sources byte for byte;
the construction manifest hash matches the installation receipt. Evidence is
recorded in `.cargo-target/83-completed-services-evidence.json`. Release validation
remains waived. Worker readiness and the full Design 82/83 acceptance audit
remain open.


### Worker recovery: remove repeated parsing and catalog scans

The real image-backed worker wire check reproduced a 44.71-second readiness time
against `/tmp/birnam-83-completed-services`. Sampling showed image recovery
rebuilding source code, repeatedly parsing the retained generation, then
collecting the resulting temporary parser heap. The mapped-image recovery step
still restores required source-defined behavior and is retained.

Generation canonicality now validates selection against the module projection
it just parsed. Image-code compilation uses one parsed projection for the
selection, authority, recorded digest and generated artifact. Each new operation
still reads live sources and invokes the current parser; there is no persistent
metadata cache. The stored generation digest and selection remain mandatory.
The portable-image suite now checks that forged digests and malformed selections
return nil, report the expected script failure and publish no native artifact.

With this change alone, the same worker became ready in 26.83 seconds and passed
all twelve exact wire round trips and orderly shutdown. This is an improvement,
not satisfaction of the production ten-second handshake requirement. A follow-up
profile found that upper GC retirement scanned every dead allocation for each
Block vtable. It now builds one address index and retains catalog entries with
constant-time membership queries. Collection order and lifetime decisions are
unchanged.

The final `/tmp/birnam-83-image-recovery` installation becomes ready in 24.91
seconds, with all twelve exact wire round trips and orderly stop passing.
Runtime tests pass (323, with 15 ignored), as do Codegen tests (22, with four
ignored) and the complete native loading suite. The portable-image suite passed
for `/tmp/birnam-83-image-projection`, whose Codegen source is identical to the
final installation: forged-authority rejection, mapped/rebuilt recovery, boxed
identity, nested source-less Code, resave, application entry and baked execution.
That image-suite installation predates only the separate GC catalog-index
change. Both changed production Rust files match the final installed sources.

Evidence is `.cargo-target/83-image-recovery-evidence.json`. The ten-second
production worker requirement remains unsatisfied; the nineteen-case service
acceptance is not claimed. The broader obsolete-interface/fixture cleanup and
complete Design 82/83 acceptance audit also remain open. Release validation
remains waived.


### Shared address indexes in storage owners

The worker profile also showed substantial general-purpose hashing in Collect
and Lifetime. Both now use the address hasher already used by Id and Runtime.
The implementation is a shared Rust helper in the ABI crate; no container or new
service crosses the native ABI. Keys remain exact allocation/behavior addresses.
Language names and string-keyed maps retain their existing hashing. The duplicate
Id and Runtime hasher implementations are removed.

Against `/tmp/birnam-83-address-indexes`, the same real worker is ready in 21.22
seconds and passes all twelve exact wire round trips and orderly stop. This
still does not meet the production ten-second deadline. Id/Collect/Runtime tests
pass (333 total, 15 ignored); Lifetime has no Rust unit tests in this run. The
complete native suite passes, including early Lifetime/collection and independent
Id startup. All six changed Rust files match installed sources. Evidence is
`.cargo-target/83-address-indexes-evidence.json`. Release validation remains waived.


### Retire the aggregate native initialization interface

The production sources no longer define `module_init`. All 48 aggregate
initializers are deleted, along with 22 Runtime dependency setters and 21 ABI
macros that generated setters at 93 module call sites. The 37 affected export
inventories remove 106 obsolete symbol entries. The native integration gate now
checks every installed library, including those outside its exact-export
inventory, for absence of aggregate initializers and dependency setters.

Removing that path exposed unused duplicate Boolean, Nil, Error, Integer, Float,
Word and String operations, the old Object Model adapter, and the old Collection
and Prelude initialization sequences. Those implementations are deleted;
production continues to use the completed native constructors and typed methods.
Compiler diagnostics identified the unused bodies, and the independent native
oracles remain the behavioral check. This removes roughly 2,800 lines rather
than hiding the old interface behind a feature flag.

The Codegen finalizer test now invokes its internal setup directly instead of
the deleted aggregate wrapper. Diagnostic closure/numeric probes attach after
completed Runtime construction, preserving their explicit opt-in environment
flags without an Integer dependency-setter callback. Test-only imports and a
numeric-declaration negative case are adjusted to avoid relying on removed code.

Design 82's current-state paragraph and the ABI overview now distinguish the
historical failure from the production implementation. Old direct C fixture
harnesses and remaining factory/provider exports still require their own
migration/retirement audit; this change does not claim those consumers pass.

Validation of this removal uses `/tmp/birnam-83-native-entries-final`. The whole
workspace library suite passes: 809 tests passed, 28 ignored. Diagnostic Runtime
also compiles with `ffi-test-hooks` and its required abort panic strategy. The
source-global lookup suite passes in installed, forced-source and compiled modes.
The owned Method suite passes source, compiled, mapped/rebuilt image and baked
execution, including aliases, replacement/removal, GC and image rejection.

The installed checks exposed a Block Loop fixture still calling the deleted
initializer. It now exercises both nested-initialization triggers (interning and
method publication) through the common entry, retaining rejection, completed
reload and original execution-table assertions. The Artifact authority check
and TCC export audit now describe current production entries and checked host
activation rather than requiring the retired initializer.

The complete native suite passes, including independent Id startup, construction
prefixes with successors absent, collector adoption and installed export checks.
All changed Rust production sources match this installed prefix. The local
receipt is `.cargo-target/83-native-entries-evidence.json`. This does not close
legacy fixture migration or the outstanding worker-service acceptance.

On this same installation, `isolate-image-wire-test.py` reaches ready in 20.99
seconds and passes all twelve exact replies and orderly stop. The unchanged
`worker-service-smoke.sh` passes its mapped/rebuilt Cell image checks but fails
starting the AOT worker with `handshakeTimeout`, before its nineteen service
cases execute. A five-second sample of that failing worker includes substantial
authored-module bootstrap work as well as image loading; the startup delay is
not confined to post-load recovery. The ten-second production deadline is
unchanged. Full worker acceptance remains open.

### Artifact emission shares its parsed source

A complete worker profile on `e864ea09` confirmed repeated whole-source parsing
inside artifact writing after image code had already been emitted. Codegen now
computes module/native requirements and source/revision digests from the same
owned tree used for emission, before embedded-source rewriting. These Rust values
last for that compilation only. Every later compilation still sends the current
`Lang parse:`; no parser or language-object cache is introduced. Artifact writing
checks the source digest again before using those descriptor facts.

The debug installation `/tmp/birnam-83-emission-metadata` reaches worker ready in
17.04 seconds, down from the preceding 20.99 seconds, and passes all twelve exact
wire round trips and orderly stop. It still misses the production ten-second
readiness deadline; this is not worker-service acceptance. All 28 installed
authored modules have byte-identical artifact descriptors compared with
`/tmp/birnam-83-native-entries-final`, including identities, dependencies,
native requirements, digests and capabilities. Codegen unit tests pass (22,
with four ignored).

The existing TCC artifact test now counts a replacement parser: one invocation
per compilation, a second compilation invokes it again, and replacing it with
a rejecting parser prevents artifact creation. The previous installed binary
fails the new count assertion; the current binary passes, including separate-
process compiled activation and exact failure output. This locks down both the
removed duplicate work and live parser replacement.

The complete `portable-image-v8-smoke.sh` passes on the same prefix: forged
authority/selection rejection, source-less nested portable meaning, aliases,
boxed identity, CodeBinding recovery, mapped/rebuilt/baked application entry,
resave and boundary-object rejection. The changed Codegen source matches the
installed copy. Evidence is `.cargo-target/83-emission-metadata-evidence.json`.
Release validation remains waived.


### Retire duplicate native providers and preparers

Forty-two old C exports are removed: three value preparers and thirty-nine
factory/provider aliases. Native constructors already return these completed
tables through Id. Rust helper implementations needed by local unit harnesses
remain ordinary Rust functions; they are no longer alternate exported startup
paths. Unused wrappers, Float/Word preparation phases and Runtime's unused Native
Allocation publication are deleted. Thirty-five export inventories are updated.

The active Error Value fixture now loads the module through Id with its completed
String input. The real host stack-anchor test obtains the Native Stack factory
through the same loading protocol while retaining its live callback replacement
and precise-policy ordering assertions. Every installed native library is checked
for retired initializer, dependency-setter, preparer and factory/provider exports.
Runtime's compiler provider is the sole explicit exception: Codegen still has a
discovery fallback and a local finalizer fixture using it. Production Codegen
receives the completed compiler descriptor explicitly. Removing that fallback
and migrating historical C initialization fixtures remain open work.

Validation uses `/tmp/birnam-83-native-providers-retired`: 809 workspace library
tests pass, 28 are ignored; the complete installed native suite and the explicitly
run real-host stack-anchor test pass. All 60 changed production Rust sources
match the installed source copies. Two test-only fixes followed installation and
were included in those successful test runs. All 84 installed Birnam libraries (56 native and 28 authored)
lack the 42 retired exports. Evidence is
`.cargo-target/83-provider-cleanup-evidence.json`. No release validation was run.
The worker deadline and complete Designs 82/83 acceptance audit remain open.


### Compiler construction has no provider fallback

Codegen now resolves compiler operations only through the descriptor supplied by
its native constructor. An absent descriptor or a refused operation stays
unavailable; neither falls back to provider discovery or raw compiler exports.
Runtime's last provider alias and the ABI's unused provider function type are
deleted. The installed export gate has no factory/provider exception.

The finalizer unit fixture supplies its compiler predecessor explicitly and
checks absent/refused operations with a symbol callback that rejects discovery.
The installed C fixture checks the retired provider is absent, rejects malformed
and foreign-carrier requests, resolves every operation on the returned descriptor
and verifies repeated native construction returns that same descriptor.

The existing selected-library discovery matrix still exercises the non-compiler
Runtime symbol adapter: a competing global symbol cannot win, failed acquisition
is cached, and the selected image stays mapped after the caller drops its handle.
Its three adapters and three modes now use production `load_floor` through Siphon,
Id and Bootstrap. The unused 169-line pre-Id preparation helper is deleted.
Host is a test-only dependency of TCC and Codegen; production dependencies are
unchanged. Adding that dependency exposed an ambiguous empty-vector assertion in
a Codegen state test; the assertion now directly checks that the result is empty.

Validation uses `/tmp/birnam-83-explicit-compiler`: Runtime and Codegen pass 345
unit tests (19 ignored); TCC and Codegen pass again with the migrated test setup.
All nine selected-library discovery cases pass. The complete native suite passes
without an export exception. Compiled-artifact tests pass, including one parse
per compilation and live parser replacement. Method values pass source,
source-free compiled, mapped/rebuilt image and baked execution, replacement,
removal, aliases, GC and image rejection. Production sources match the installed
copies; the later Cargo/test-only changes are recorded separately in
`.cargo-target/83-explicit-compiler-evidence.json`.

This closes obsolete production preparer/provider export removal. It does not
close historical C fixture migration, worker startup or the complete acceptance
audit. The last measured worker readiness remains 17.04 seconds against the
unchanged ten-second deadline. Release validation remains waived.


### Remove repeated work in image recovery

A fresh profile of the actual image-backed worker at `6742d400` reached ready in
19.39 seconds with sampling enabled. It exposed duplicate source parsing in
composed-generation adoption and a further full parse in the host used only to
decide whether to seal the recovered generation. The host's nil-digest branch
previously accepted the image anyway; it was not an additional rejection gate.
Successful source recovery already requires a canonical durable generation,
and composed adoption validates the successor before sealing.

Mapped and rebuilt images now share one host recovery sequence. It restores
source-defined methods, Alien wrappers and source-backed globals, reconciles the
composition, then seals it directly. Composed adoption validates its selection
and digest against the same parsed module snapshot used to rebuild the binding
ledger. Later operations still invoke the current parser; there is no persistent
cache or relaxation of artifact authority checks. Embedded-file rewriting now
consumes each child tree instead of repeatedly cloning its descendants.

Runtime also accounts for a new allocation using the requested extent supplied
to its retained Id primitive. It no longer queries membership, size and identity
again merely to compute that charge. Tracking an existing allocation still
queries its actual extent. The same header/alignment formula, permanence and
byte-budget overflow checks apply; no allocation, collection or identity is
elided.

The first debug installation, `/tmp/birnam-83-recovery-work`, reaches worker ready
in 13.02 seconds. Including allocation accounting,
`/tmp/birnam-83-recovery-accounting` reaches ready in 12.58 seconds. Both pass all
twelve exact wire replies and orderly shutdown. The unchanged ten-second
production deadline remains unmet; this is not full worker-service acceptance.

Validation on the latter installation includes the complete native construction
suite and full portable image-v8 suite: nested source-less code, boxed identity,
ordinary/rebuilt/baked entry, resave and boundary rejection. The image suite now
also forges a composed generation's digest and selection, verifies both are
rejected, restores them and verifies successful adoption in both recovery modes.
The compiled-artifact check retains the current-parser assertions and now checks
nested binary embedding after deleting the asset, including NUL and `0xff` bytes.
All 527 affected library tests pass (20 ignored). Production sources match the
installed copies. The local receipt is
`.cargo-target/83-recovery-work-evidence.json`. Release validation remains waived.

The unchanged full worker-service suite passes its mapped/rebuilt Cell checks
(41 before collection, 42 afterward), then fails AOT worker startup with
`#handshakeTimeout`. Its nineteen service cases are not reached. The receipt
includes the redirected failure output; the passing wire test does not substitute
for this outstanding acceptance requirement.


### Runtime construction has one native entry

The temporary `birnam_runtime_init_v15` export is removed. Its validation and
ownership transfer now live privately beside Runtime's common native entry;
there is no second external construction ABI. Bootstrap continues to supply the
same ten completed predecessors through Id. The ABI document retains its v15
filename to identify that operand layout and now describes only native entry.

The independent C fixture sends malformed Id carriers with completed predecessor
operands to the actual native descriptor. It retains wrong-count, wrong-kind,
null-descriptor and incomplete support/value table rejection checks before
ownership transfer. The installed export gate now rejects every standalone
`birnam_runtime_init_v*` export in every library, in addition to the exact
Runtime export inventory.

`cargo check -p birnam-runtime` and the complete native suite pass against
`/tmp/birnam-83-native-constructor`. Both changed production Rust files match
that installation's source copies. The repository test files are hashed in
`.cargo-target/83-native-constructor-evidence.json` alongside the validation logs.
This removes the temporary constructor facade; it does not close the older
fixture migration, worker readiness failure or full acceptance audit.
No release validation was run.


### Consolidate support admission on production Bootstrap

The five host admission matrices for Storage Observation, Deferred Identity,
Deferred Index, Dispatch Cache and Guard Verdict still tested removed provider
exports and fake Id dependency setters. They are replaced by one 76-case matrix
using actual Id and Bootstrap, with only the three support libraries available.
It also covers Method Observation. No Runtime or value module is present.

Each library is tested absent, without an entry, with incompatible entry headers,
and without an initializer. For every support request, a C proxy calls the real
native module entry and changes just that result: null/wrong-kind descriptor,
initializer failure, bad version/extent, or each missing operation. Other requests
retain the real implementation. Per-case receipts prove the intended request was
reached. The unmodified prefix then exercises all operations, independent state,
context-bearing cache results and repeated selection through the existing C
oracle. Removed setter rejection cases have no current equivalent; the installed
export gate prevents reintroduction of those obsolete interfaces.

Ten duplicate fixture files and five unreachable host preparation errors are
removed. The existing shell runners select the shared matrix. Their separate
legacy behavioral C fixtures remain unmigrated, so this is not a claim that those
complete runners pass. The installed native suite directly runs the new matrix.

Validation uses the unchanged native libraries from
`/tmp/birnam-83-native-constructor` and the current rebuilt host test driver.
All 76 admission cases and the full native suite pass; host units pass 85 tests
with one ignored. Host checking and shell syntax checks pass. Evidence is
`.cargo-target/83-support-admission-evidence.json`. Worker readiness, the remaining
fixture migration and full acceptance audit remain open. Release validation
remains waived.


### Runtime service oracles use production startup

Root Registry, GC Settings, Native Stack and native stack scanning no longer
construct a diagnostic floor through `id_init`, preparers, provider exports or
`module_init`. A shared host driver uses production `load_floor`, supplies each
oracle its explicit completed services, and loads the C fixture through Id.
Independent factory probes also use Id's native entry; the root probe destroys
the separate root owner it creates. Historical baseline compile branches and
obsolete empty module-initialization assertions are removed.

The fixture initializer returns a pinned native entry. The host calls it after
the loading message returns, because storage observation requires an idle host
and correctly rejects the managed turn around loading. No production admission
check is bypassed or relaxed. Production collector activation already installs
the thread anchor, so the suppression oracle explicitly clears it before testing
zero-anchor behavior rather than assuming a pre-activation default.

The existing C assertions retain ordered duplicate providers, callback-time
append, nested collection suppression, factory independence, precise-mode
conversion, automatic thresholds and accumulated budgets, native/provider roots,
callback/finalizer anchor reassignment, every scan byte boundary, register capture,
and exact owner/observation/overflow/tracer rejection diagnostics.
`scripts/runtime-services-test.sh PREFIX` runs all four; the three original
service scripts select their corresponding cases. Native Stack also retains its
Runtime unit and actual installed-host check. ABI pages now describe the actual
ownership and loading path instead of pre-Id preparation.

Against `/tmp/birnam-83-native-constructor`, the combined runner passes all 16
native cases and all 16 service unit tests. The complete Native Stack runner
also passes the Runtime anchor unit and the real installed-host test. Production
libraries are unchanged; the test driver is rebuilt from the current checkout.
Evidence is `.cargo-target/83-runtime-services-evidence.json`. These tests close
four legacy fixture migrations, not their separate image fixtures or the wider
acceptance audit. Worker readiness and the remaining migrations stay open.
Release validation remains waived.


### Service image fixtures use explicit native dependencies

The Root Registry, GC Settings and Native Stack image fixtures now expose only
the common native entry. They no longer receive the removed aggregate
`module_init` argument list or discover factory/provider exports. Each declares
its actual Runtime, namespace-import, roots and optional settings/stack/factory
inputs. Later no-argument System loads reinstall its test method using the same
completed inputs and do not register duplicate providers.

These CLI/image tests need native inputs that `System loadModule:` intentionally
does not manufacture. A shared test-only Native Loader proxy forwards the real
constructor, unchanged, then selects completed Runtime services through Id and
initializes the fixture explicitly. A temporary library overlay retains all
original installed libraries and substitutes only that proxy. There is no new
production export, service bundle or test hook. The installed CLI and actual
baked executable still execute the scenarios; they are not replaced by a custom
image runner.

Root Registry and GC Settings now explicitly publish their image main and add
an explicit rebuild pass. Across all three services, source startup, source-free
compiled callers, ordinary image entry, explicit rebuilding and baked execution
pass with exact output, aliases, collection and repeated fixture loading.
The independent C assertions retain provider order and callback-time append,
settings thresholds and mutation, native roots, anchor assignment and scanning.
This evidence does not by itself distinguish ordinary native mapping from an
allowed source-recovery fallback; the full image acceptance audit must retain
its separate exact-outcome check.

All three complete scripts pass against `/tmp/birnam-83-native-constructor`.
The prefix is unchanged; fixture sources, proxy/overlay and output hashes are
recorded in `.cargo-target/83-service-images-evidence.json`. ABI pages now document
the test entry paths. The wider legacy fixture migration, worker readiness and
complete acceptance audit remain open. Release validation remains waived.


### Image emission reuses block references and validated module trees

Worker profiling identified whole-program block resolution once per emitted
block, including capture analysis of unrelated subtrees. The inventory now
retains the block references it visits, so emission directly associates each
block with its stable key. Resolution for selected resumables also skips roots
and lexical paths that cannot contain the requested block. Canonical operation
positions, nested block ordinals and capture order remain unchanged.

Image compilation now retains the Rust syntax trees produced while validating
its generation sources. Selected resumable emission consumes those same trees
instead of sending the parser and traversing its CST a second time. Other
metadata operations discard each tree immediately. No tree or language object
is cached across operations; every later compilation observes the current
parser. Authority, digest, source identity and embedded-file checks remain.

The installed regression selects a real resumable module and replaces Lang's
parser with a counter. The previous binary reports 2 then 4 calls across two
image compilations; the new binary reports 1 then 2. Replacing the parser again
with a rejecting implementation prevents artifact creation. Ordinary compilation
retains its existing fresh-parser and binary-embedding checks. Inventory tests
retain literal canonical keys and check block identity; nested capture tests
include an unrelated sibling subtree.

All 119 compiler library tests pass (four ignored). The full portable image-v8
suite passes boxed identity, nested source-less code, ordinary/rebuilt/baked
entry, resave, boundary admission and forged-authority rejection. Production
sources match the debug installation at `/tmp/birnam-83-image-snapshots`.

A sequential wire comparison measured 12.36 seconds for the previous installation
and 11.54 seconds with block traversal fixed, with identical replies. Reusing
validated module trees then measured 11.07 seconds and passed all twelve exact
wire exchanges and orderly shutdown. These timings still exceed the unchanged
ten-second worker readiness deadline. The full worker-service runner passes its
Cell checks in ordinary and rebuilt images, then fails AOT startup with
`#handshakeTimeout`; its nineteen service cases are not reached.
Evidence is `.cargo-target/83-image-emission-evidence.json`. The remaining fixture
migration and complete Designs 82/83 acceptance audit stay open. Release
validation remains waived.


### GC retirement uses one address index and fewer record copies

A profile of the installed image-backed worker confirmed substantial time in
retirement of its temporary parser heap. Id now decodes the dead addresses into
one set and uses it for behavior, weak-key and Symbol retirement. Previously it
constructed three equivalent sets. The collector copies only dead addresses
from its scoped allocation snapshot, avoiding intermediate full-record vectors.
Removing a dead collection record now checks its flags and takes its object in
one lookup. It still rejects marked/permanent records; the detached payload
remains live for its finalizer and is released afterward. Public native layouts,
callback order and collection timing are unchanged.

The new collection-record unit checks missing/repeated removals, marked and
permanent objects, and removal after marks clear. The existing independent C
collector fixture now installs a weak key in a surviving dictionary: successful
collection removes both the key and its Symbol, while rejected tracing retains
both. This runs before Runtime exists through the shipped native constructor.
All 334 affected Id, Collect and Runtime library tests pass (15 ignored), and
the complete installed native construction suite passes on
`/tmp/birnam-83-retirement-index`. Its full portable image-v8 suite also passes
ordinary/rebuilt/baked execution, boxed identity, nested source-less code, resave,
boundary admission and forged-authority rejection. All six production source
files match the tested installation.

A sequential worker-wire comparison measured 11.13 seconds on the preceding
installation and 10.73 seconds with these changes. All twelve replies match byte
for byte and orderly shutdown passes. The production ten-second readiness
requirement remains unmet. The full worker-service runner passes ordinary and
rebuilt Cell checks, then fails AOT startup with `#handshakeTimeout` before its
nineteen service cases run. Evidence is
`.cargo-target/83-retirement-index-evidence.json`; worker-service acceptance,
legacy fixture migration and the full Designs 82/83 audit remain open. Release
validation remains waived.


### Deferred storage fixtures use production construction

Deferred Identity and Deferred Index now enter through Id with explicit Runtime
services and the selected Storage path. Their forwarding libraries call Storage's
common native entry and replace only the requested factory result. They no longer
forward removed `module_init`, dependency setters or factory-provider exports.
The fixtures copy the borrowed initialization protocol before retaining it, and
select pinned native method entries through Id instead of casting language
Method objects to C functions. Custom lookup delegates directly to its selected
native entry and preserves the original lookup result.

The two image hosts are one shared host with explicit fixture selection. Both
retain exact `Mapped`/`SourceRecovery` assertions, idle storage checks and the
post-eviction `BUSY` result. The unit/admission runners are shared too. The full
independent 128-row index model moved from an obsolete direct-export caller into
the existing production Id/Bootstrap support fixture, retaining all-bit values,
misses, repair writes, exact swap removal, capacity and moved-ownership checks.
Both ABI guides now describe the current Storage/Runtime responsibilities.

On `/tmp/birnam-83-retirement-index`, both loaded matrices pass source and
compiled execution with optimization disabled, enabled and forced recovery,
including source removal. Both complete image matrices pass ordinary startup,
source-free compiled execution, mapped/rebuilt/baked execution, exact host loader
outcomes, and forwarding runs with and without forced recovery. They preserve
allocation identities, finalization, saved 24-method dictionaries, replaced
lookup and Machine slot methods, parent/behavior changes, aliases and GC. The
102 relevant unit tests pass (eight existing ignored tests); all 76 support
admission cases and the Storage export/install-name audit pass. Evidence is
`.cargo-target/83-deferred-storage-evidence.json`.

This closes these two behavioral fixture groups, not all legacy fixture setup.
The old shared `namespace-support.h` chain still references deferred-storage
setters for its remaining consumers. Its migration, worker readiness/service
acceptance and the full section 6 audit remain open. Release validation is waived.


### Bounded driver return beneath a synchronous caller

The Runtime lifetime regression now sends to a real synchronous native method
which queues a native retirement, starts a two-step Machine with a one-step
budget, and resumes it to completion. It checks the live Frame at the bounded
return and its removal at completion. Both returns remain beneath the synchronous
caller; attempted checkpoints there release nothing. Returning through the
outer application entry releases the record once, and another outer checkpoint
does not release it again.

All four `runtime::turn_tests` pass. This adds the explicit bounded-driver case
to the existing outward-result rooting, callback reentry, collection-in-turn and
detached finalized-owner tests. It uses a counted native release callback to
observe checkpoint ordering; the installed image tests separately exercise
mapped code. No production policy or deadline changed. Full section 6 acceptance
still requires the remaining migration and worker checks recorded above.

### Restore saved compiled code during ordinary image startup

Image-v8 saved the portable code unit but omitted the compiled source-generation
unit. The host compensated with an unconditional `rebuildImageCode` send even
when native mapping succeeded. New saves include both units and the generation
manifest in a versioned bundle inside the checksummed Code record. The image
loader maps and admits the saved generation through the existing artifact
ABI/manifest and lifetime machinery. The host no longer requests a rebuild.
Explicit `--rebuild` and earlier v8 images missing that unit use embedded-source
recovery; malformed saved units are rejected. Canonical source-map, digest and
selection validation remains part of admission.

The authored `DispatchGeneration` initializer creates G0 in Store without filling
the native lifecycle cache. Image compilation now accepts that absence while
still rejecting a conflicting cached generation and validating the full durable
projection. This makes the existing generation compiler usable at save time.

`scripts/compiled-image-startup-smoke.sh` runs real production startup, disables
source compilation/evaluation entries before loading, and checks mapped and baked
application entry after deleting the input source. It also checks explicit and
missing-unit recovery. An independent Python reader checks bundle lengths, both
Mach-O units and the outer checksum, then constructs malformed-manifest, empty-unit
and trailing-byte rejection cases. The ordinary portable-image suite includes
this regression. The new format is documented in the portable-code ABI guide;
the image user guide describes compilation at save time and direct compiled boot.

Debug evidence uses `/tmp/birnam-83-compiled-image-v3`: 120 Image/Host/Codegen unit
tests passed with five existing ignores. The complete portable-image suite and
its compiled-image regression pass. Evidence is recorded in
`.cargo-target/83-compiled-image-evidence.json`.
Worker wire readiness is 7.48 seconds (previously 10.88), with twelve exact wire
exchanges and orderly shutdown passing. The full service runner's mapped/rebuilt
Cell checks and all nineteen AOT service cases pass. Its forced-source diagnostic
still times out; the user explicitly waived that timing as an acceptance gate.
The default worker runner now checks ordinary startup; `--force-source` retains
the optional diagnostic. The ten-second production deadline is unchanged.

These results close the unconditional image-recompilation bug. They do not close
the remaining Designs 82/83 fixture migrations or the full requirement audit.

The final image-loader review preserved error recognition through a live
`isError` send. The complete targeted compiled-image regression passes on
`/tmp/birnam-83-compiled-image-final`, including compiler-disabled mapped/baked
entry, explicit/missing-unit recovery and malformed-record rejection. The broader
portable-image and nineteen-case AOT worker results above precede this final
error-recognition adjustment; the final worker rerun was still running when the
user requested committing after targeted validation.
