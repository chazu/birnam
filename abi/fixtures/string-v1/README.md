# Production String storage

`libbirnam_string.dylib` exposes `birnam_native_module_v1`. Bootstrap loads it
with the selected Id protocol and three Word operands: the completed root
registry owner, Lifetime owner and Namespace owner. Object Model's ownership
messages must already be installed. No upper IdApi, Error, Integer, Float,
Execution or collector is required.

Initialization returns a Word pointing to the existing 64-byte `StringApiV1`.
Its immutable 80-byte `StringStorageV1` describes the same 16-byte raw payload
and private byte buffers used by later String methods and image loading. The
[C mirror](string_v1.h) remains unchanged. Native initialization rejects wrong
operand counts/kinds, invalid descriptors and repeated initialization. An
unsuccessful entered initializer and its partial roots remain pinned; it does
not advertise an incomplete provider.

| Receiver | Selector and operands | Result |
|---|---|---|
| String object | `fromBytes:` Bytes | A new String containing a copy |
| String object | `bytes` | Borrowed Bytes, valid while the object remains rooted |
| String object | `length` | Word byte count |
| String object | `clone` / `new` | A new String with separately owned bytes |

Wrong operand counts/kinds and byte queries/clones on objects without String
storage return native InvalidRequest. Empty Strings have a non-null borrowed
pointer and length zero. Byte lookup uses the private ownership map, so changing
the raw payload or an object's behavior cannot redirect a native byte pointer.

Construction sends the current Id `allocate` message. Selected Id allocation
facts check the returned size and behavior before any payload write; already
owned objects and objects with finalizers are rejected. This permits method
replacement without allowing a language replacement to cause an out-of-bounds
write or overwrite another String's buffer. No map borrow crosses dispatch.
Clone roots its source across allocation. The result is rooted through payload
initialization and finalizer attachment in Lifetime. Root providers retain the
canonical behavior/prototype; individual buffers remain weak by object address
and their registered finalizers release them during collection.

Initialization installs the raw layout and owner association and publishes
`String`/`StringVt` in the existing Namespace. Production startup subsequently
attaches that exact operations table to Runtime and other consumers. It does
not run `birnam_string_prepare_v4`; that entry remains for legacy `id_init`
consumers. The native `installText:` extension on String accepts three Words:
completed base values, Integer and Float descriptors. It installs the fourteen
existing printing, comparison, indexing, transformation and numeric-conversion
methods before Block or Runtime. The extension owns a separate complete context;
the storage owner has no future numeric slots. Existing bindings are preserved.
Late `module_init` checks the Id and preserves the completed native extension.
Collection methods still mount separately.

Text calls root their receiver and arguments across callbacks. Concatenation
sends the current `printString` for a non-String argument and roots that result
while reading bytes. Numeric callbacks use the live tables. Indexing remains
byte-oriented and returns nil for invalid indices/types. `asSymbol` preserves
the existing first-NUL boundary; numeric parsing preserves its four-character
ASCII trim policy, distinct from `trimmed`'s six whitespace characters. Native
and legacy text methods share the trim, replacement and bracket-scanning code.

`scripts/id-native-loading-smoke.py PREFIX` exercises the actual twelve-library
construction prefix using `scripts/fixtures/early-string-native.c`, then resumes
the same production Runtime and tests retention, cleanup and construction again.
The same driver then completes the sixteen-library value/text prefix and runs
`early-text-native.c` before Block/Runtime and again after adoption with precise
GC, including collection during printable-result and byte callbacks.
`scripts/string-image-smoke.sh PREFIX` checks the completed String behavior in
source, compiled, saved-image and baked-image execution.
