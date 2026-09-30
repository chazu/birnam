# Float

`libbirnam_float` takes Namespace and returns Float, publishing Float and FloatVt.
It requires completed Roots, GC, ObjectModel, ObjectVt, Integer, String, Error,
true, false and nil. Bootstrap then sends Integer `installFloat:` and String
`installNumbers:`. No dependency is forward-filled during Float construction.

Float owns its f64 interpretation of eight raw payload bytes. Object Model
allocates and shallow-clones the storage; an inherited GC reference owner treats
numeric bits as non-references in precise payload mode, including after private behavior changes. Recognition keeps
the existing exact Float behavior rule. Dependencies and selectors belong to the
instance; no process singleton, shared table or cached foreign entry remains.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Float `fromBits:` | native Word | Float with those IEEE-754 bits. |
| Float `bitsOf:` | Object | Native Word for an exact Float, Unit otherwise. |
| Float instance `bits` | none | Native Word containing its bits. |
| Float instance `+`, `-`, `*`, `/` | Integer or Float | Float result; either signed zero divisor produces #zeroDivide. |
| Float instance `=` | Object | Exact Float equality; NaN is unequal, another family is false. |
| Float instance `<`, `<=`, `>`, `>=` | Integer or Float | Boolean comparison. |
| Float instance `negated`, `abs`, `sqrt` | none | Float; negative square root produces #domain. |
| Float instance `asInteger`, `floor`, `ceiling`, `rounded` | none | Integer; non-finite or out-of-range conversion produces #overflow. Rounding uses ties away from zero. |
| Float instance `printString` | none | Current String construction; preserves existing formatting, including `-0.0`, `NaN.0`, `inf.0`. |

Integer operands are read through Integer `wordOf:`; conversions send its current
`fromWord:`. Float results send current `fromBits:`. Factories, String and Error
are live messages, with inputs and intermediate results rooted across callbacks.
Arithmetic propagates incoming Errors using the current Namespace ErrorVt;
equality keeps its existing false result. Namespace's current Error factory
constructs diagnostics, preserving replacement behavior.

Run `mise run native-messages` for production tests of finite and non-finite
arithmetic, signed zero, bit preservation, conversion limits, mixed operations,
method replacement and collection during dependency calls. Compiler and Image
owner tests separately exercise generated arithmetic and bit-preserving recovery.

Collections use owner messages `collectionEquals:` (Object → Boolean) and
`collectionHash` (no arguments → native Word, low 32 bits). Float preserves
exact-family equality and the existing bit mix, normalizing both signed zeros.
Identity is tested first by collections, so the same NaN key is retrievable while
distinct NaNs differ. Private behaviors use identity; public `=` remains separate.
See [Map's key contract](../birnam-map/README.md#keys).

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
See [System GC controls](../birnam-system-gc/README.md).
