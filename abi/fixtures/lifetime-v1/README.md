# Initialized Lifetime records v1

`lifetime_v1.h` independently declares the 144-byte owner returned by
`libbirnam_lifetime.dylib:birnam_native_module_v1`. Its three initializer operands
are Words containing the complete payload-layout, trace-hooks and finalizers
factory descriptors, obtained through each library's native module entry.
The result is a Word containing the initialized owner address. Id and these
primitive algorithms are its only construction dependencies.

The owner creates the existing public APIs with its own record storage and Id
membership facts. Set/query operations and failure paths work before Runtime,
GC or language values. Payload bytes are copied before replacing an existing
record, including when the input borrows that record's own bytes. Finalizer
registration ignores missing objects and, after the upper permanence policy is
installed, permanent objects. Hook selection uses the existing bounded inherited
selection algorithm and the current behavior parents supplied by its consumer.

Canonical descriptor operations read or replace exact records without public
admission or lookup. Their caller establishes membership, retains the relevant
objects/code and serializes mutation. Record keys are weak; storing a description
or finalizer does not keep an object alive. Use the root registry for owned
references. Borrowed payload views expire on replacement or retirement.

`retire` releases layouts and removes hooks for the supplied native addresses;
it leaves finalizers intact. The collector copies a finalizer, clears that entry
through `replace_finalizer` with an empty slot, detaches Id allocation storage,
invokes the copied callback, then releases the payload. A behavior's finalizer
therefore survives retirement of that behavior's metadata. `destroy` releases
all remaining description bytes before destroying the capabilities and owner;
it never invokes object finalizers or frees Id objects.

The initial access policy checks the owner thread and uses an Id storage callable
selected during construction. `set_access` replaces that complete policy with
another complete policy. Production Runtime installs its observation admission
and current allocation/behavior/permanence facts after construction; no operation
waits for that policy to become usable. Fact flags are allocated=1, behavior=2,
permanent=4. Canonical collection queries invoke no admission callback.

The installed `scripts/id-native-loading-smoke.py PREFIX` constructs this owner
with all value/collector/Runtime libraries absent. Independent C uses the same
owner to describe a rooted object graph, a native trace edge and three finalizers
before Runtime, then verifies preservation, reclamation and weak-record removal
after normal production adoption. The existing payload-layout, trace-hooks and
finalizers image workloads cover their public APIs after adoption.
