# Native Bootstrap v2

`libbirnam_bootstrap.dylib:birnam_native_module_v1` initializes with Id and no
other operands. It returns a Word containing the 64-byte owner independently
declared in `bootstrap_v2.h`. Bootstrap's only Cargo dependency is `birnam-abi`.
It invokes Id's current `loadNative:` message for every load; it does not open
libraries itself, replace Id's carrier, or need Runtime, values or a collector.

`load(context, path, length, operands, count)` returns Id's complete native
Outcome unchanged. Object operands and the receiver/intern/load objects in
Protocol operands are temporarily retained during initialization. The result's
Object or Protocol references are retained before argument roots are released,
including nested loads and initializers returning an argument. Scalar values,
borrowed Bytes, opaque native contexts and native owner pointers retain their
ordinary explicit caller/module lifetime contracts.

Before a root registry exists, Bootstrap owns these references in its early
set and collection is disabled. `adopt_roots(context, complete_registry)` installs
real root records for that set before discarding the early set. It returns a
native status, rejects transfer during module entry or transfer reentry, and
accepts repeated adoption of the same registry without duplicating roots.
Retargeting another registry is rejected. Transfer must precede collector
activation. Loading and result retention already work before this operation;
there is no absent future-provider callback.

After transfer, loads retain their inputs/results directly in the same root
registry. `release_result(context, operand)` releases one matching retained
Object/Protocol result occurrence and returns 1, or 0 when none remains. Its
caller first establishes any ownership needed after release. Duplicate returned
objects have separate retained occurrences. `destroy` releases all outstanding
records and the native owner. All operations require the owner thread; the
owner/registry/code must outlive every user of their descriptors. Destruction
is forbidden during loading or transfer.

Production `begin_id` constructs Id and Bootstrap. Bootstrap captures Id's original
primitive entries before upper construction. A second common-loader request to
the same Bootstrap library takes `Bytes("construction"), Word(owner), Bytes(id_path)`
and returns a native entry owned by that Bootstrap instance. The path is copied,
fixes the sibling-library directory for this construction, and cannot be retargeted.
The native entry takes one Bytes stage name and returns a complete native outcome.
Its receiver is unused. Invoke it outside module initialization; nested graph
construction is rejected. No Bootstrap-state borrow crosses a module callback.

The loaded Bootstrap module owns the dependency order and arguments from roots
through the complete native application stack. Stages `roots`, `lifetime`, `values`, `namespace`, `objectModel`,
`string`, `integer`, `float`, `word`, `trace`, `collection`, `runtimeSupport`,
`runtimeValues`, `runtime`, `errorValue`, `printing`, and `blockLoop` return Word
pointers to completed descriptors. `control` installs Integer/Boolean control,
Block loops and Object/Symbol printing and returns Unit.
`error` returns its Object; `text` and `activate` return Unit. Each constructs its
required predecessors first. Repeated successful requests return the same owners.
`existingRoots` returns the current root descriptor or Unit without constructing
anything. Unknown stages fail with native InvalidRequest.

Host prefix entry points delegate to this same constructor; they contain no
second dependency sequence. `begin_floor` requests the roots stage. Normal
startup requests Runtime, then `control` before collections, and retains
Bootstrap until teardown. Normal startup has no legacy native module sequence.
Direct legacy module consumers and aggregate Runtime capability discovery still
need migration; complete construction does not imply those consumers are safe.

The installed `scripts/id-native-loading-smoke.py PREFIX` uses an independent C
module to initiate a nested Bootstrap load before any root-registry library is
available. The inner initializer returns an otherwise unrooted object; Bootstrap
retains it across transfer, Runtime adoption and precise GC. Explicit release
permits reclamation. The fixture repeats loading/release after Runtime exists,
checks Protocol results using the real Id module, and checks unchanged native
load failures. The host ABI/root-registry image workload covers normal startup,
source-free compiled modules, images and baked execution.

`pause(context, action)` runs one trusted native action after root adoption, with
new loads, result release and nested pause rejected. It rejects entry from a
module initializer. Bootstrap retains every existing root throughout the call;
publication resumes only after the synchronous action returns. Collect uses this
scope for validation, native-boundary installation, constructor replacement and
publication of Active. Failure keeps the existing roots and leaves collection
disabled; production startup then fails. No callback slot is left for a future
owner. The action and its context must remain live and must not unwind.

The control constructors use the common native descriptor and these exact operands:

| Library | Operands after Id | Result |
|---|---|---|
| Error Value | Word(String API) | Word(Error helper API v3) |
| Integer | Bytes("control"), Word(Runtime API), Word(namespace presence), Word(namespace import), Word(Error helper API) | Unit; installs `to:do:` only |
| Printing | Word(Runtime API), Word(String API) | Word(Printing API v1); installs Object/Symbol defaults |
| Block Loop | Word(Runtime API), Word(namespace presence), Word(namespace import), Word(Error helper API) | Word(Block Loop API v3); installs `whileTrue:` and `whileFalse:` |
| Boolean | Bytes("control"), Word(Runtime API) | Unit; installs Block-taking branches and short-circuit operations |

Runtime's common descriptor also accepts Bytes("namespacePresence") or
Bytes("namespaceImport") followed by its completed Runtime API pointer. These
return the existing live namespace views after checking the Runtime/Id pair.
They create no new namespace and have no compiler dependency. Constructor
results and installed code follow the existing process-lifetime contracts.
Recursive entry into an unfinished control constructor is rejected; repeated
completed construction retains the same execution selection. Legacy native
consumers still use their existing callable descriptors; migrating those
consumers to checked message dispatch remains separate required work.

Collection construction also uses the common native descriptor. In this table,
`F` is the six explicit operands `Word(Runtime API), Word(namespace presence),
Word(namespace import), Word(namespace export), Word(root owner v1),
Word(payload layouts)`. It is shorthand for six operands, not another ABI table.
All inputs are complete before the entry is invoked.

| Library | Operands after Id | Complete result |
|---|---|---|
| Collection | F | Object(Collection behavior), published and rooted |
| StringBuilder | F, Word(finalizers), Word(Integer), Word(String) | Object(StringBuilder prototype), with storage and methods installed |
| Collection Value | Word(Integer), Word(Float), Word(String) | Word(value-policy API v1) |
| Vector | F, Word(Integer), Word(String), Word(value policy), Object(Collection behavior) | Word(Vector API v6), with representation, methods and exports installed |
| Vector | Bytes("comma") | NativeEntry for its exact concatenation implementation; available only after Vector construction |
| Range | F, Word(Integer), Word(String), Word(Vector), Object(Collection behavior), NativeEntry(Vector concatenation) | Word(Range API v6), including Integer `to:` |
| Map | F, Word(Integer), Word(String), Word(Vector), Word(value policy), Object(Collection behavior) | Word(Map API v6) |
| String Collection | Word(Runtime API), Word(String), Word(Vector), Object(Collection behavior) | Word(String collection API v1), with String inheritance and splitting installed |
| Store | F, Word(vtable enumeration), Word(method observation), Word(trace hooks), Word(Integer) | Object(Store), with Cells, Globals, roots and observation installed |
| Store | Bytes("compensation"), Word(Runtime API) | Word(existing compensation API v1), after Store construction |

Bootstrap stages `collectionBase`, `stringBuilder` and `store` return their
completed Objects. `valuePolicy`, `vector`, `range`, `map`, `stringCollection`
and `storeCompensation` return the descriptors above. `collections` completes
StringBuilder, Vector, Range, Map and String's extension and returns Unit.
Vector completes before Range/Map/String Collection; Store follows collections.
Bootstrap releases temporary Object-result roots after the module has registered
its own roots. Construction scopes retain newly allocated objects and selectors
until those permanent roots are available. Range retains the complete typed
Vector callable without repeating Vector's installation or discarding context.

The parser group uses the same native descriptor and completed value families:

| Library | Operands after Id | Complete result |
|---|---|---|
| `libbirnam_id_artifact.dylib` | F, Word(Integer), Word(String), Object(Vector) | Object(Artifact); descriptor inspection, materialization and failure reporting |
| `libbirnam_id_peg.dylib` | F, Word(trace hooks), Word(finalizers), Word(Integer), Word(String), Object(Vector) | Object(Peg); patterns, grammars, matching, CST results and syntax errors |
| `libbirnam_id_lang.dylib` | Word(Runtime API), Word(namespace presence), Word(namespace import), Word(namespace export), Word(root owner v1), Word(Integer), Word(String), Object(Peg) | Object(Lang); the completed Birnam grammar and parsing facade |

Bootstrap stages `artifact`, `peg` and `lang` return those completed Objects;
`parsers` constructs Artifact and Lang (including PEG) and returns Unit. Artifact
and PEG follow Vector. Lang receives the completed PEG object explicitly and
builds the grammar with PEG messages. It neither compiles source nor depends on
TCC, Codegen or Runtime compiler admission. Lang needs no payload-layout input:
PEG owns its grammar storage. The host constructs Store before this group in the
full startup sequence, then continues to the remaining compiler/system entries.

Temporary constructor roots retain each new facade and behavior until permanent
module roots are registered. Repeating a completed constructor preserves the
original grammar rather than rebuilding it. Vector operands name an already
published predecessor; ordinary Artifact/PEG methods continue to resolve Vector
through the live namespace. Native inputs are trusted under Design 83's boundary.
The legacy entry points remain for existing direct native consumers; this change
does not yet migrate their internal language sends to checked dispatch.

Compiler construction uses completed admission and Portable Code predecessors:

| Library | Operands after Id | Complete result |
|---|---|---|
| Runtime | Bytes("compiler"), Word(Runtime API) | Word(compiler/admission API v1), constructed before TCC |
| TCC | F, Word(vtable enumeration), Word(trace hooks), Word(finalizers), Word(Integer), Word(Float), Word(Word), Word(String), Word(compiler/admission API) | Object(Tcc); C compilation, Units, runtime binding, symbols and native installation |
| TCC | Bytes("artifactRuntime") | NativeEntry exposing the completed TCC image's C ABI for artifact linking |
| Portable Code | F, Word(vtable enumeration), Word(trace hooks), Word(finalizers), Word(Integer), Word(String), Word(namespace names), Object(Vector), Object(Error) | Unit; complete CodeObject/CodeProgram families and native portable APIs |
| Codegen | F, Word(vtable enumeration), Word(GC settings), Word(finalizers), Word(Integer), Word(String), Word(namespace names), Word(compiler/admission API), Word(Store compensation), Object(Tcc), Object(Store), Object(Globals), Object(Lang), Object(CodeObject), NativeEntry(TCC artifact runtime) | Object(RustCodegen); compiler, reflection and portable operations |

The `compilerAdmission`, `tcc`, `portable` and `codegen` stages expose those
results. `codegen` first completes Store, parsers, Portable Code and TCC. Portable
Code needs values/execution but no compiler, so it precedes Codegen instead of
leaving Codegen's portable operations dependent on a later module. Admission
requires completed Runtime values and precedes TCC's generated-code hooks.

TCC and Codegen's native paths resolve compiler-owned entries through the
supplied admission descriptor, keeping its context. Existing legacy entry points
retain their compatibility lookup path. Codegen receives and installs Store
compensation before any of its methods are published; the host no longer probes
and configures that dependency after initialization. Its Tcc/Store/Globals inputs
are the completed predecessor objects; ordinary language namespace lookups remain
live. Remaining non-compiler private symbol lookups still require migration.

Bootstrap preserves `BIRNAM_TCC_DYLIB` as the exact TCC override path. The selected
library must implement the common native descriptor. Other native compiler-group
libraries come from the Id library directory. New TCC/ForeignAddress/Codegen
objects retain constructor roots, and Portable Code scopes its temporary family
roots until their namespace exports are installed.

Generated Units bind all Codegen helper imports explicitly, including their Rust
spellings. They do not require Codegen's native image to be globally visible.
Unit-owned cache callbacks retain that ownership for both symbol spellings; the
TCC Runtime binding supplies the resumable entry bridge exactly once.

Compiled Mach-O artifacts still import the frozen public `birnam_*` C ABI.
Before mapping an artifact, Codegen invokes the supplied TCC entry with its
original context, no receiver and no arguments, then exposes its own C ABI.
Each operation retains a `NOW | GLOBAL | NOLOAD` reference to its already
constructed native image, selected by an address within that image. It does
not initialize another module or resolve a future provider. The TCC action
returns Unit and is available only after its native constructor completes.
Ordinary native loading and JIT compilation keep their local library scope.

System construction starts with Prelude's common native entry. It takes
`Word(Runtime), Word(namespace presence), Word(namespace import),
Word(namespace export), Word(root owner v1)` and returns the rooted System
object. It publishes no IO, GC or source-loading method before its implementer
is constructed. In the following table `S` means `Word(Runtime), Word(root
owner v1), Object(System)`. Each entry installs its methods before returning
its existing callable descriptor as a Word (Native Loader returns System); repeated construction preserves the
same Runtime and System selection. Constructor selectors have temporary roots.

| Library | Operands after Id | Operations available on return |
|---|---|---|
| Text View | Word(String) | Exact and printable String bytes |
| System GC | S, Word(GC settings), Word(Integer) | `gc`, `liveObjects`, `autoGc:`, `preciseGc:` |
| System Output | S, Word(Text View) | `print:`, `println:` |
| System Input | S, Word(namespace presence), Word(namespace import), Word(String), Word(Error helpers) | `readLine` |
| Environment | S, Word(namespace presence), Word(namespace import), Word(String), Word(Error helpers), Word(Text View) | `getEnv:` |
| File IO | Same as Environment | `readFile:`, `writeFile:contents:` |
| Native Loader | S, Word(namespace presence), Word(namespace import), Word(Integer), Word(Error helpers), Word(Text View) | `loadModule:` and `loadModule:arguments:` through Id |
| Source Loader | S, Word(namespace presence), Word(namespace import), Word(Integer), Word(String), Word(Error helpers), Word(Text View), Object(Codegen) | `load:` and its errors |

`systemBase` returns System; `systemIo` completes all entries except Source
Loader and returns Unit. It needs no collections, Store, parsers or compiler.
`system` also completes Source Loader and returns System. Named stages
`textView`, `systemGc`, `systemOutput`, `systemInput`, `environment`, `fileIo`,
and `sourceLoader` return their completed descriptors; `nativeLoader` returns System.
The host delegates to this sequence. Native Loader supplies only the explicit
Object operands selected by its caller to Id; it has no aggregate initializer
or provider/installer export. See [the language contract](../../native-loading.md).


The application constructors complete the native stack. `F` has the same six
operands defined above; Object inputs name already published predecessors.

| Library | Operands after Id | Complete result |
|---|---|---|
| Isolate | F, Word(finalizers), Word(Integer), Word(Float), Word(String), Object(Vector), Object(Map), Object(Error), Object(Globals) | Object(Isolate); worker, handle and failure operations |
| Relational | F, Word(Integer), Word(Float), Word(String), Object(Store), Object(Vector), Object(Map) | Object(Store), also published as db; entities, logic variables and queries |
| Alien | F, Word(vtable enumeration), Word(finalizers), Word(Integer), Word(Float), Word(String), Word(namespace names), Object(Tcc), Object(Store), Object(Vector), Object(Error) | Object(Alien); foreign addresses, libraries, wrappers and callbacks |
| Image | F, Word(vtable enumeration), Word(GC settings), Word(Integer), Word(Float), Word(Word), Word(String), Word(namespace names), Word(compiler/admission), Object(System), Object(Codegen), Object(Store), Object(CodeObject) | Object(ImageSaveReport); installs System image operations |

Stages `isolate`, `relational`, `alien` and `image` return these Objects.
`applicationBase` completes Relational, Isolate and Alien, leaving Image absent;
`applications` also completes Image. Both aggregate stages return Unit. New
families and constructor selectors have temporary roots until their module roots
are registered. Repeated construction preserves the selected Runtime and objects.
Image uses the completed compiler descriptor for compiler-owned entries, and its
native constructor establishes its canonical implementation without an environment
variable written by the host.

The [production construction manifest](../../production-construction-v1.json)
records native entries, operands, completed predecessors and operations for all
56 installed native libraries, including the 52 former host startup rows. The
installer checks library coverage, missing predecessors, cycles and the library
inventory in Bootstrap's source, and records the manifest hash in its receipt.
These static checks do not prove semantic dependency completeness. Prefix tests
exercise real operations while later libraries are physically unavailable, then
complete full startup with zero legacy module initializations. Legacy `module_init`
exports and direct consumers remain compatibility work to remove under Design 83.

Native Runtime begins with weak Symbol retention. Previously the first legacy
module load enabled that policy as a side effect; native-only startup must not
retain the entire interner while waiting for an initializer that no longer runs.
Explicit legacy admission can still select its bounded compatibility policy.
