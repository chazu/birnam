# String, Bytes and text construction

`libbirnam_string` takes one Namespace Object and returns String. Bootstrap loads
it after Object Model. It requires completed Roots, GC, ObjectVt, true, false and
nil bindings. It publishes String, StringVt, StringBuilder, StringBuilderVt,
Bytes and BytesVt;
there is no separate builder library. Repeat initialization returns the published
String without reinstalling methods.

Every cross-module operation is a fresh Id message. The instance owns selectors,
family objects and a weak address map of native byte buffers. String's two raw
payload words and Builder's three words mirror pointer/length/capacity for native
diagnostics; consumers must use messages, never these layouts. Native access
uses the owner's map, so payload mutation and private behavior changes cannot
redirect borrowing or cleanup. No mutable process/TLS state or service tables
are introduced. Calls are confined to the owning Id thread.

## Messages

| Receiver / selector | Arguments | Result |
|---|---|---|
| String `fromBytes:` | borrowed native Bytes | New String with copied bytes, including NUL/non-UTF-8. |
| String instance `bytes` | none | Borrowed native Bytes. Copy before any subsequent send that can collect or invalidate the owner; retaining the object does not authorize retaining this borrow across arbitrary callbacks. |
| String instance `length` | none | Native Word byte count (existing native protocol). |
| String instance `new`, `clone` | none | New canonical String with an independent copy of the receiver's bytes. |
| String instance `printString` | none | Receiver. |
| String instance `,` | Object | New String; non-String input receives `printString`, then `bytes`. |
| String instance `=` | Object | true/false byte equality with a String owned by this instance. |
| String instance `trimmed` | none | New String with ASCII whitespace trimmed. |
| String instance `replace:with:` | two Strings | New String, left-to-right nonoverlapping substitution; empty needle leaves bytes unchanged; non-String inputs return nil. |
| String instance `asSymbol` | none | Id Symbol; preserves the existing first-NUL boundary. |
| StringBuilder instance `new`, `clone` | none | New empty builder (existing semantics). |
| StringBuilder instance `add:` | Object | Append text, using `printString` for non-String input; return receiver. |
| StringBuilder instance `asString`, `printString` | none | Independent String snapshot through the current String `fromBytes:` method. |

Operations producing text send the current `fromBytes:` method. Resource-aware
String cloning constructs its own buffer and installs its own GC finalizer.
Printing callbacks and allocating sends hold no byte-map borrow. Native Bytes
inputs and callback results are copied before further sends; receiver/argument
objects are retained through Roots for the call. Builder mutation observes GC's
construction phase before changing its buffer.

## Lifetime

String implements the existing owner messages `rootsDo:`, `referencesOf:do:` and
`finalize:`. Its root inventory retains family identities and selectors, not all
buffer keys. Text contains no object references. GC attaches this owner to String, Bytes
and Builder behaviors, and attaches a finalizer to every constructed buffer.
Finalization removes the owned buffer after Id detaches the object, without
consulting detached Id metadata or calling another dependency. Private behaviors
inherit the reference owner. Changing/removing a finalizer carries responsibility
for the replaced resource cleanup, as with other native finalizers.

Number-independent operations are installed immediately. After Integer and Float
are complete, Bootstrap sends String `installNumbers:` with both prototype
Objects. That complete extension installs absent methods while preserving prior
replacements. It reads numbers through `wordOf:` and constructs through
`fromWord:`/`fromBits:`, never a foreign layout. Repeat installation is rejected.

| Receiver / selector | Arguments | Result |
|---|---|---|
| String instance `size` | none | Integer byte count. |
| String instance `at:`, `byteAt:` | Integer index | One-byte String or Integer byte value; nil for invalid index/type. |
| String instance `copyFrom:to:` | two Integer indices | String slice with exclusive upper bound; nil for invalid range/type. |
| String instance `indexOf:` | String | Integer index, nil if absent; empty needle gives zero. |
| String instance `bracketDepth` | none | Integer bracket balance, ignoring strings/escapes and line comments. |
| String instance `asInteger` | none | Parsed signed i64, nil on overflow or invalid text. |
| String instance `asNumber` | none | Integer when possible, otherwise Float; nil for invalid text. |
| StringBuilder instance `size` | none | Integer byte count. |
| StringBuilder instance `addByte:` | Object | Append low eight bits of an Integer; ignore another type; return receiver. |

Indexing remains byte-oriented. Numeric parsing trims space/tab/CR/LF, preserving
its historical distinction from `trimmed`'s six whitespace characters. Splitting
uses the collection extension below. Run `mise run native-messages` for production
coverage, including replaced constructors and collection during conversions.

## Collection extension

After Vector exists, Bootstrap sends String `installCollections:` with the Vector
prototype Object. It installs `split:` only if no existing method supplies it.
This is completed String functionality; the old String Collection crate is deleted.

`split:` takes a separator Object and returns a Vector. An exact-family String
separator matches binary bytes, left to right without overlap, retaining empty
and trailing segments. An empty separator produces one newly copied String. A
non-String separator returns a one-element Vector containing the receiver itself.
String constructs each segment through its live `fromBytes:` method and builds
the result through Vector `new` and `add:` messages, rooting each intermediate.

String also owns its `collectionEquals:` and `collectionHash` implementations;
see [Map's key protocol](../birnam-map/README.md#keys). They preserve exact-family
byte comparison and hashing independently of public `=`.

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
For String/Builder, this covers the payload mirror only, never text bytes or Rust
containers. See [System GC controls](../birnam-system-gc/README.md).

## Bytes prototype

`Bytes` is an ordinary empty prototype with its own delegated `BytesVt`, not an
ABI pointer wrapper. It shares this module's private buffer storage, tracing and
finalization. Its contents are arbitrary bytes, with no text decoding or object
references. String/Builder/Bytes ownership kinds remain distinct even after
private behavior changes. Bootstrap publishes Bytes when it loads String; no new
native module, native export or install step is needed.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Bytes `fromBytes:` | borrowed native Bytes operand | New Bytes with an independent copy, made before any outgoing send. |
| Bytes instance `new`, `clone` | none | New canonical Bytes with a copy of the receiver's contents. |
| Bytes instance `bytes`, `length` | none | Borrowed native Bytes operand or native Word count, following String's borrow rules. |
| Bytes instance `size` | none | Integer byte count. |
| Bytes instance `at:` | Integer | Integer 0–255; nil for a negative/out-of-range/non-Integer index. |
| Bytes instance `asString` | none | Binary-safe String copy through current `String fromBytes:`. |
| String instance `asBytes` | none | Bytes copy through current `Bytes fromBytes:`. |

`size` and `at:` install with the existing numeric extension. Normal method
replacement and Reflect publication apply to Bytes and its clones. The native
ABI's borrowed `Operand::Bytes` remains distinct from this language prototype.
The `embedding` production test covers all 256 byte values, conversion, cloning,
MOP publication, collection/reclamation and live collecting replacements.

`Bytes` supplies bounded `copyFrom:to:`, `readU32At:`/`readU64At:`, `sha256`
and `wyhash64`. The 64-bit reader returns Integer word bits, including the sign
bit. `readTaggedFieldAt:schema:` and `readTaggedRowsWithSchema:limit:` interpret
caller-supplied tag payload shapes (`0` empty, `1` fixed eight bytes, `2` u32,
`3` length-prefixed bytes, `4` nonempty NUL-free name). Unknown tags and malformed
framing answer nil. Row tables expose ordinary read-only indexed views over an
owned byte snapshot. These temporary views own no language references and are
collected with their buffers; callers retain all format and reference rules.

`readTaggedTableWithSchema:limit:` returns one compact table rather than eagerly
allocating a language object for every row. `emptyTaggedRows` supplies an empty
table. `readTaggedRecordsAt:count:prefixBytes:fields:schema:` decodes fixed-prefix
records with caller-selected tagged fields, requiring exact framing through the
end of the buffer. Prefix bytes are opaque; the caller assigns their meaning.

Table views support `concat:`, `copyFrom:to:`, locator filters, membership scans,
reference bounds and sparse reference projections. Masks are ASCII `0`/`1`
strings indexed by byte tag. Rows expose `sourceIndex`; filtering and slicing
preserve original positions, while concatenation assigns new sequence positions.
Record scans can report byte offsets for ordered errors. All table metadata is
native-owned bytes and scalars, with no language references. Integer inputs and
outputs go through live Id messages to their owner. Format policy belongs to the
authored caller, including accepted tags, reference families and error ordering.

`Bytes withSymbolName:` appends an Id-owned Symbol name to an owned byte prefix.
It copies the borrowed name before result allocation, returns nil for a non-Symbol,
and leaves formatting policy to authored callers such as `printing.birnam`.
