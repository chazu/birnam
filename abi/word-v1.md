# Native Word dependency v1

`birnam-word` owns Word's canonical behavior, zero prototype, eight-byte unsigned
payload and existing printing, equality and Integer-conversion methods. Production
constructs it through Id after Integer and before Block or Runtime. Its inputs
contain no Float dependency. The remaining combined Machine receives the same
completed family through `birnam_word_dependency_v1`.

`WordApiV1` is a 72-byte C table: version, byte size, stable native context,
behavior/prototype identities, constructor, reader, exact recognizer and a pointer
to an immutable 48-byte `WordStorageV1`. The codec declares payload size/alignment
and pure unsigned initialization/read callbacks. See the independent
[`word_v1.h`](fixtures/word-v1/word_v1.h) declaration.

## Production construction

`birnam_native_module_v1` receives the real Id protocol and seven operands:
Words pointing to completed root-registry, Lifetime, base-values, Namespace and
String descriptors, the Error factory as an Object, then a Word pointing to the
completed Integer table. Object Model is already installed. The result is a
Word pointing to the existing operations table. No new descriptor or library is
introduced.

Word creates its behavior and zero prototype through current Id messages, records
its raw layout in Lifetime, associates its owner and publishes Word/WordVt.
`fromBits:` constructs from a native Word operand, and `bits` returns the full
unsigned payload as a native Word. `printString` keeps lowercase hexadecimal
formatting with a `0x` prefix. Equality accepts exact Words only. `asInteger`
returns an Integer through its live operations table, or constructs the existing
`overflow` Error above `i64::MAX` using current Namespace/Error factory and String
messages. No upper Runtime is required for these success or failure paths.

Native construction sends the current Id allocator and checks the actual returned
allocation's size and behavior before writing. Exact recognition checks registered
storage and the canonical behavior. The unsafe payload reader remains valid after
behavior reassociation. Pure codec callbacks do not allocate or dispatch; Machine
and Image retain their existing codec, inherited-family recognition and boxed-byte
contracts. No Word optimization declaration or allocator-context guard is added.

## Adoption and lifetime

The root provider retains canonical objects, predecessor references and cached
selectors, including during construction and after method removal. The provider
returns null until construction completes. Production skips legacy Word
preparation and attaches the completed table to its consumers. Late `module_init`
checks the Id identity and preserves early bindings, layouts and native method
contexts. Rooted early Words survive Runtime adoption and precise collection;
releasing their last root permits reclamation. Libraries, tables and native
contexts remain pinned after entry, including after initialization failure.

The legacy `birnam_word_prepare_v6`/IdApi constructor remains for native clients
pending migration. On an already completed native Word it returns the same table
when called on the same Id. On the legacy path, preparation precedes numeric
activation, then `module_init` publishes the identities and installs methods
with Integer/String/Error Value attached. Legacy reinitialization retains its
original method-reinstallation behavior. Native consumers must use the current
IdApi; historical manifests are evidence only. ForeignAddress is a separate family.

The [eight-symbol inventory](rust-word-dylib-v1.json) includes the native entry.
Run `scripts/id-native-loading-smoke.py PREFIX` for the real early construction
prefix and adoption/GC checks. `scripts/word-image-smoke.sh PREFIX` and
`scripts/word-error-value-image-smoke.sh PREFIX` cover source, compiled, saved-image
and baked execution; `mise run word` exercises the legacy debug native client.
