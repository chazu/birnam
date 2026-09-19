# Native Word dependency v1

`birnam-word` owns Word's canonical behavior, zero prototype, eight-byte unsigned
payload and three existing methods. It depends on IdApi v6 and explicitly acquires
Integer for conversion. Id initializes without Word; its still-combined Machine
layer receives Word through `birnam_word_dependency_v1`.

`WordApiV1` is a 72-byte C table: version, byte size, stable native context,
behavior/prototype identities, constructor, reader, exact recognizer and a pointer
to an immutable 48-byte `WordStorageV1`. The codec declares payload size/alignment
and pure unsigned initialization/read callbacks. The independent C declaration is
[`word_v1.h`](fixtures/word-v1/word_v1.h).

Ordinary consumers copy current operations before calls. Construction consults
the current Id allocator; recognition consults its checked vtable operation and
accepts only the canonical behavior. Native payload reading retains the unsafe
live-payload contract after behavior reassociation. Machine's internal reader
uses the admitted codec: no allocation, Id calls, collection, reentry or mutable
module state. No Word optimizer declaration or allocator-context guard is added.
TCC keeps its generated-code symbol names and unsigned conversion policy; Image
keeps inherited-family recognition and its current boxed Word bytes.

The host pins Word's library and calls `birnam_word_prepare_v1(IdApi*)` before
Float preparation and Integer initialization. Partial roots are registered before
either identity is allocated. The provider returns null until the primitive table
is complete. Attach the table to the host and combined runtime before ordinary
publication can reenter a Word consumer. Word's `module_init` remains after Float,
with Integer attached; it sets the owner, exports `Word`/`WordVt` and installs the
methods. Later native consumers declare `birnam_word_dependency_v1`.

Repeated initialization keeps the canonical objects and namespace bindings but
reinstalls the three methods, matching the method-only module. Repeated attachment
of the same pointer succeeds; invalid tables or a different mounted family are
rejected. Libraries and native instances remain live for all published methods,
root callbacks, codecs and contexts, including after startup failure. Test runtimes
own separate instances and API adapters instead of using native global bindings.

Word no longer occupies a floor bootstrap enumeration position; its identities
and names follow module construction/publication. Names, first-alias selection,
rebinding and value semantics retain their contracts. ForeignAddress is a separate
family and is unchanged. All native modules require rebuilding for IdApi v6's
37 fields / 296 bytes. Historical v1-v5 manifests and fixtures remain unchanged;
Method, Frame and image formats remain unchanged.

Run `mise run word` and `scripts/word-image-smoke.sh PREFIX` against a debug prefix.
