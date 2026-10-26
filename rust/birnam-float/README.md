# Float

`libbirnam_float` takes Namespace and returns Float, publishing Float and FloatVt.
It requires completed Roots, GC, ObjectModel, ObjectVt, Integer, String, Error,
true, false and nil. Bootstrap then sends Integer `installFloat:` and String
`installNumbers:`. No dependency is forward-filled during Float construction.

Float owns its f64 interpretation of eight raw payload bytes. Object Model
allocates and shallow-clones the storage; an inherited GC reference owner treats
numeric bits as non-references in precise payload mode, including after private behavior changes. Recognition keeps
the existing exact Float behavior rule. Dependencies and selectors belong to the
resident native module state. Native Methods carry the Float prototype as their
Oop context; there is no cached foreign executable or shared payload catalog.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Float `fromBits:` | Word or immediate size Oop | Float with those IEEE-754 bits. |
| Float `bitsOf:` | Object | Word or immediate size Oop for an exact Float, absent reference otherwise. |
| Float instance `bits` | none | Word or immediate size Oop containing its bits. |
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

Run `bash scripts/oop-id-slice-test.sh` for isolated native checks of IEEE bit
preservation (including NaN payloads), signed zeros, arithmetic, signed conversions,
live constructors/ErrorVt and GC during allocation and conversion. The Integer
and Error providers are independent C test objects at this checkpoint. Real
Integer, compiler and image integration remain pending.

Collections use owner messages `collectionEquals:` (Object → Boolean) and
`collectionHash` (no arguments → immediate size Oop, low 32 bits). Float preserves
exact-family equality and the existing bit mix, normalizing both signed zeros.
Identity is tested first by collections, so the same NaN key is retrievable while
distinct NaNs differ. Private behaviors use identity; public `=` remains separate.
See [Map's key contract](../birnam-map/README.md#keys).

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
See [System GC controls](../../docs/design/authored-small-modules.md#system-gc).

Only Oops cross Id. Full-width IEEE bits travel through Word's low/high projections
and constructor; Float never reads Word storage. Printing sends a byte Sequence
to String's current `fromBytes:`. Id protects incoming call objects; an owner-local
scoped root list protects intermediates without per-call retain/release messages.
