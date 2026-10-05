# Word

The native initializer takes Namespace (Object), publishes `Word` and `WordVt`,
and returns Word. Bootstrap loads it after Integer and String/Error. Repeating
initialization returns the published object.

| Receiver / message | Arguments | Result |
|---|---|---|
| Word `fromBits:` | Native Word containing unsigned 64-bit bits | Word object |
| Word instance `bits` | None | Native Word |
| Word instance `=` | Object | Boolean; exact Word-family bit equality |
| Word instance `asInteger` | None | Integer, or overflow Error above `i64::MAX` |
| Word instance `printString` | None | String, lowercase hexadecimal with `0x` prefix |

Word owns its raw eight-byte representation. Allocation and shallow cloning use
Object Model; in precise payload mode, an inherited GC reference method declares no edges, including when
a value has a private behavior. No storage descriptor or Runtime adoption adapter
remains. Cross-owner conversion, errors and printing use current Id methods.

Collection keys retain the established **identity** rule for Word. Its value `=`
does not change that rule; see [Map's key protocol](../birnam-map/README.md#keys).

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
See [System GC controls](../../docs/design/authored-small-modules.md#system-gc).
