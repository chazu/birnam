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

Production `begin_id` constructs Id and Bootstrap. `initialize_roots` uses
Bootstrap to construct the root registry and transfers its early set.
`initialize_lifetime` uses that same loader for the complete representation owner.
`begin_floor` preserves the combined Id/Bootstrap/root prefix, and ordinary host
startup uses these steps. The loaded host retains Bootstrap until teardown and
exposes explicit result release. The remaining aggregate Runtime construction
and legacy module entries still need migration to the complete construction graph.

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
