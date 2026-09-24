# Ordinary objects and private behaviors

Bootstrap loads Object Model after GC activation, passing Namespace. It returns
and publishes an `ObjectModel` Object. The module obtains completed Roots and GC
objects from Namespace, installs complete methods, and registers its roots,
reference enumeration and metadata retirement through messages. Repeat loading
in the same Namespace returns the published instance.

Mutable state belongs to the instance: weak behavior-owner associations, weak
ordinary-allocation records, exact slot descriptions, and temporary clone buffers.
There is no module singleton, dependency table, retained dependency implementation,
or foreign registry view. The module uses only the common Id ABI.

## Existing object semantics

These methods are installed on Id's Vtable receiver:

| Selector | Native operands | Result |
|---|---|---|
| `ownerOf:` | candidate Object (including null) | Owner Object, or Unit if unowned. |
| `ownerOf:put:` | behavior Object, owner Object or Unit | Behavior Object. Unit clears the association. Both objects must be live. |
| `prepareBehaviorMutationOf:` | object Object | Current behavior Object, after allocation admission. |
| `ensurePrivateVtableFor:` | object Object | Its private behavior. A behavior is already its own mutation target; an existing association is reused. Otherwise delegate from its current behavior, associate the new behavior with the object, and reassign the object's behavior. |

Every dependency operation is a fresh Id message. In particular, private-behavior
construction sends `prepareBehaviorMutationOf:`, `ownerOf:`, `delegated`,
`ownerOf:put:` and `reassociateBehaviorOf:with:`. Input objects and intermediate
behaviors stay retained across those sends. Admission still happens when an
existing private behavior can be reused; collection callbacks cannot bypass the
phase rule by taking that fast path.

`clone` and `new` on Object's behavior both shallow-copy an ordinary object's
padded payload into a distinct allocation with the same behavior. They do not
copy finalizers or make a private behavior eagerly. A clone initially inherits
method changes through its shared behavior; giving that clone a private behavior
delegates from the shared behavior. Identity comparison is the existing Boolean
module's `==`, and Id continues owning allocation identity and behavior epochs.

The default clone copies storage allocated by Object Model, plus empty ordinary
objects. It rejects unowned nonempty payloads and Id behavior records. Native
values must implement their own cloning and resource ownership; this module
cannot decode String, Integer, execution frames or another family's payload.

A payload snapshot stays rooted until the clone is filled. Even if an allocating
method replaces the source's references and collects during reentry, the copy's
references remain alive. No allocation/state borrow spans a callback.

## Ordinary storage and slot metadata

Native operations on the ObjectModel receiver are:

| Selector | Operands | Result |
|---|---|---|
| `allocate:bytes:` | behavior Object, payload-size Word | A zeroed ordinary Object allocated through the behavior's guarded `allocate` message; ownership is recorded here. |
| `setLayoutOf:to:rest:` | behavior Object, Bytes of slot kinds, tail-kind Word | Unit. Copy the prefix and replace this exact behavior's description. |
| `slotKindOf:at:` | behavior Object, word-index Word | Kind Word; zero for an unset description. |
| `allOopLayoutOf:words:` | behavior Object, extent Word | Word 1 iff an explicit description marks the entire extent as Oops, otherwise 0. |
| `slotAt:in:` | word-index Word, ordinary Object | Referenced Object, or Unit for null. Reject out-of-bounds and explicitly non-Oop slots. |
| `slotAt:in:put:` | word-index Word, ordinary Object, referenced Object or Unit | The ordinary Object. Retain inputs and admit mutation before writing. |

These are native construction/access operations. Word indexes and kind codes do
not introduce dependencies on Integer or a shared API catalog. The copied layout
algorithm comes from the deleted Payload Layout crate: descriptions are exact,
never inherited, accept arbitrary byte codes, and distinguish unset from an
explicitly empty prefix. Code 1 marks an Oop; other explicit codes are not traced.
An unset description conservatively checks this owner's padded payload words
against Id's allocation membership. Descriptions and ownership records never
expose borrowed layout pointers or Rust containers to another module.

Use these operations for ordinary object storage. Native value representations
remain private to their value modules and register their own GC enumeration.
Object Model fails enumeration of unowned nonempty payloads rather than reading
them through an inherited default hook.

## Collection and retirement

Object Model implements `rootsDo:`, `referencesOf:do:` and `retire:` on its own
receiver. GC selects these implementations through messages. Its reference
provider is registered for ordinary Object behavior and for Id's behavior objects.
Id still owns all header and dictionary decoding. Behavior references are visited
in the established order: header, parent, upper owner, dictionary keys.

The module's roots include its dependencies, selectors and temporary clone buffers;
**they do not include the association map's keys or owners**. An owner is traced
only from a reachable behavior. Thus rooting a private behavior keeps its owner
alive, while an otherwise unreachable object/private-behavior cycle is collected.

GC sends `retire:` with the dead-address Bytes before index retirement or physical
reclamation. Object Model removes dead ownership, storage and layout entries.
Allocation identities additionally fence records against address reuse. User
finalizers therefore see already-retired metadata. These messages introduce no
Object Model layout knowledge into GC or Id.

Id's existing `behaviorCount`, `behaviorAt:`, `selectorAt:in:`, `parentOf:` and
binding/lookup messages supply reflection. The redundant enumeration adapter and
Runtime adoption wrappers are deleted. Runtime must not reinstall the old
construction/ownership implementations over the completed production methods.

## Validation and remaining scope

`mise run native-messages` loads the real production libraries through Siphon and
Bootstrap. Tests cover canonical/private behaviors, identity and shallow cloning,
independent method mutation, exact layout boundaries, slot reachability, live
replacement, collection during clone allocation, finalizer non-copying, native
payload rejection, and repeated cycle retirement. `birnam-object-model` also keeps
the copied-layout algorithm's boundary checks as unit tests.

This completes basic object behavior over the current production prefix. Runtime's
execution-specific deferred materialization, recovery, guarded specialization and
code lifetime remain for Execution's migration. Their algorithms and semantic
tests are retained. Full Runtime/CLI, value families and image integration are
still pending; this module does not recreate their old descriptor adapters.

Collection keys use ordinary owner methods. Object's `collectionEquals:` takes
an Object and returns canonical Boolean identity equality; `collectionHash`
returns the established mixed-address hash as a native Word. Symbol overrides
hashing using Id's `symbolBytesOf:` and the former name-byte FNV-1a rule. Upper
value owners supply their own overrides without extending Object Model's value
catalog. See [Map's consumer contract](../birnam-map/README.md#keys).


## Allocation omission queries

`ObjectModel canDiscardClone:of:` takes the selected NativeEntry and source Object
and returns native Word 1 only for this instance's original clone entry, an
ordinary zero-slot source and the current default allocation path. It recognizes
implementations, including aliases, rather than the selector `clone`. Nonempty
or unsupported storage and changed entries decline with Word 0.

`ObjectModel canOmitAllocationFor:` takes a behavior Object and returns native
Word 1/0. It checks its own current `allocate:bytes:` wrapper and asks GC's same
named query about the behavior's actual allocator. Integer uses this for the
index boxes omitted with the callee. These queries inspect only owner-local
state, retain inputs across sends and export no native service table. The caller
must fence callback-time mutation before committing omission. They do not reserve
memory or replay the omitted allocation's GC timing; see
[the consumer contract](../../docs/design/90-discarded-clone.md).
