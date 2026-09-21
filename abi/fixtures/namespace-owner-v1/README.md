# Native Namespace owner v1

The independent C header freezes the 56-byte completed Namespace descriptor.
The Rust mirror is [`namespace_owner.rs`](../../../rust/birnam-abi/src/namespace_owner.rs).
It contains the receiver, raw Namespace v8 API and storage context, and one
owner destruction operation. There are no future service slots.

`libbirnam_namespace:birnam_native_module_v1` takes three Words pointing to
completed Root Registry, Lifetime and Base Values descriptors. It constructs a
zero-payload receiver through Id, declares its layout, creates the actual binding
table, and registers a native root provider before publishing methods or names.
The root provider visits the receiver and both halves of every current binding.
It copies the references before calling the supplied mark function. Rebinding
releases the old value naturally; names remain part of the ordered namespace.

The native entry returns a Word pointing to the owner. Bootstrap/its caller keeps
that owner until it transfers ownership to upper Runtime. Duplicate construction
returns InvalidRequest, including following a failure with partial publication.
Id keeps entered native code pinned. Destroying the owner clears the provider's
live selection and frees its table; the registry may subsequently call the
provider, which then reports no roots. Consumers must stop using the descriptor,
receiver methods and borrowed table views before destruction.

| Selector on the Namespace receiver | Native operands | Result |
|---|---|---|
| `bindingAt:` | Object Symbol | Object value, or Unit for absent/cleared binding |
| `bindingAt:put:` | Object Symbol; Object value or Unit to clear | The supplied value/Unit |
| `bindingCount` | None | Word count, including cleared names |
| `bindingNameAt:` | Word index | Object Symbol, or Unit outside the table |
| `nameOf:` | Object value | First currently bound Symbol alias, or Unit |

Malformed operands return InvalidRequest without String, Error or numbers.
These operations use native Symbol/Word operands; language Integer-valued and
collection-valued conveniences are later extensions. The initial published names
are Namespace, Object, Boolean, False, Vtable, ObjectVt, Symbol and SymbolVt, all
referring to already completed objects.

Upper startup adopts this exact table and constructs its existing C-name import,
export, presence, name and enumeration adapters. It does not republish the early
names. It initializes its legacy weak dictionary view from the early bindings
and replaces the native publication method with completed Runtime observation
and mirror policy. That method continues to write the same native table. The
native reads and root provider continue unchanged. The temporary Runtime bridge
is now v4; the v3 bridge is removed. This is still an intermediate aggregate
Runtime constructor, not the final package boundary.

The installed native-loading test exercises this actual prefix with String,
Integer, Block and Error absent. It checks malformed arguments, bounds, order,
reverse aliases, raw API agreement and post-adoption C-name imports. One alias
can be cleared while another retains the object; clearing the last alias permits
precise collection. Namespace image workloads cover existing later consumers.
