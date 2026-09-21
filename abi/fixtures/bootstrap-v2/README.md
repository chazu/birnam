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
through Runtime and its control extensions. Stages `roots`, `lifetime`, `values`, `namespace`, `objectModel`,
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
Bootstrap until teardown. The host's later
legacy module entries and aggregate Runtime capability discovery still need
migration; this constructor is not a claim that the entire upper graph is done.

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
