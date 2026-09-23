# Map

The initializer takes Namespace, publishes `Map`, `MapVt`, `MapNode`, and returns
Map. Dependencies are Collection's parent behavior, Object Model, Roots, Integer,
String, Vector and Boolean/Nil. All cross-owner operations use fresh Id messages.

Map retains its persistent 32-way HAMT, sparse bitmap nodes and full-hash collision
nodes. Insertions copy only changed paths. A replacement retains the original key;
storing the identical value for an existing key returns the original Map. Whether
an insertion adds a key is local to that operation, including nested callbacks.

| Message | Result |
|---|---|
| `new`, `clone` | Empty Map |
| `at:put:` key and value | Persistent Map |
| `at:` key | Stored value or nil |
| `at:ifAbsent:` key and callable | Stored value, otherwise callable `value` |
| `includesKey:` key | Boolean |
| `size` | Integer |
| `keys`, `values` | Vector in HAMT traversal order |
| `do:` callable | Iterate values through Vector, return original Map |
| `inject:into:` initial value and callable | Fold values through Vector |
| `keysAndValuesDo:` callable | Send `value:value:` per pair, return original Map |
| `printString` | String in `{key -> value, ...}` form |

Object Model traces the declared Map root and node reference slots. Scoped roots
protect operands, traversal snapshots and intermediate nodes across callbacks;
no Rust borrow spans a dependency send. Node tags and extents are checked before
native reads. No Map payload reader or constructor is exported to another owner.

## Keys

Concrete consumers are Map lookup/insertion and Vector `indexOf:`. Identity is
checked first. Otherwise they send `collectionEquals:` with the other Object and
accept the canonical true Object. Map sends `collectionHash` with no arguments;
the result is a native Word whose low 32 bits are the hash.

These are ordinary replaceable methods, not a shared value-family table. Owners
implement the established policy:

- Object Model supplies identity equality and mixed-address hashing by default.
  Word and collection objects retain that identity rule.
- Integer compares only exact-family signed values and hashes the 64-bit value.
- Float compares only exact-family values; both signed zeros hash identically.
  The same NaN object is found by identity; distinct NaNs remain unequal.
- String compares exact-family bytes and hashes all bytes, including embedded NULs.
- Symbols retain interned identity equality and the former FNV-1a name hash
  through the first NUL, obtained through Id's `symbolBytesOf:` message.

The numeric/address mix and byte FNV-1a algorithms are unchanged. Private numeric
or String behaviors fall back to identity for collection keys. Public `=` remains
separate, preserving the former distinction (notably Word and NaN behavior).

Replacement methods must maintain the usual key contract: equal keys have equal
hashes, and key hashes stay stable while stored. Changing a hash implementation
does not retroactively reindex an existing Map. There is no dependency on Float,
Word, String representations, or a central registry of recognized families.
