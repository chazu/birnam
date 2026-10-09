# Integer

Language-facing typed operations include `integerValue:` (normalize through live
`wordOf:`/`fromWord:`), `compareInteger:with:` (exact signed comparison returning
-1/0/1 or nil for non-Integers), and `fromWordResultOf:selector:` (send a caller's
zero-argument query through Id, then box its Word through live `fromWord:`).
They contain no Range or System policy and read no foreign value representation.

`libbirnam_integer` initializes with Namespace and returns Integer. It requires
Roots, GC, ObjectModel, ObjectVt, String, true, false and nil; Error must already
be published. It publishes Integer and IntegerVt. Repeat loading returns the
published object without reinstalling methods. Float and Block are not needed
to construct this family.

The instance owns its selectors and family identities. Eight raw payload bytes
hold an i64; only Integer reads that representation. Object Model allocates and
shallow-clones the storage. In precise payload mode, a GC reference owner declares no object edges,
including through private behaviors. Recognition preserves the existing exact
Integer behavior rule: a changed behavior is not an exact Integer. No process
singleton, service table, foreign layout or cached dependency entry is used.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Integer `fromWord:` | native Word | Integer interpreting the Word as signed 64-bit bits. |
| Integer `wordOf:` | Object | Native Word for an exact Integer, Unit otherwise. |
| Integer instance `word` | none | Native Word containing its signed bits. |
| Integer instance `+`, `-`, `*`, `/`, `mod:` | Object | Checked i64 arithmetic, or Error. Division truncates toward zero; MIN mod -1 is zero. |
| Integer instance `=` | Object | Exact Integer value equality, false for another family. |
| Integer instance `<`, `<=`, `>`, `>=` | Object | Comparison, preserving existing f64 comparison rounding. |
| Integer instance `printString` | none | Decimal String through current `fromBytes:`. |
| Integer instance `to:do:` | end Integer, callable Object | Inclusive iteration by live `value:` sends; receiver on completion, first Error on failure. MAX is visited without overflowing the loop counter. |
| Integer `evaluate:receiver:argument:with:` | selected NativeEntry, receiver Object, argument Object, borrowed evaluator NativeEntry | Result Object for an owned arithmetic entry; Unit for an unknown entry. See below. |
| Integer `canOmitIndexing` | none | Native Word 1 when the current `fromWord:`/`wordOf:` entries and allocation path are ordinary; Word 0 otherwise. Used for omitted compiler-generated PC/index operations. |
| Integer `installFloat:` | completed Float Object | Unit; install mixed arithmetic/comparisons and `asFloat`. |

The mixed extension is a complete private context created after Float exists.
It reads Float through `bitsOf:` and constructs through `fromBits:`. It replaces
only its own original Integer implementations and preserves prior user changes;
`asFloat` is installed only if absent. It retains Float for its methods' lifetime,
without a future service slot in Integer. Repeat extension installation is rejected.
Bootstrap owns this sequencing.

Native numeric entries, bootstrap-arithmetic admission and the numeric visitor
borrow incoming receiver/argument coverage from the Id ABI for the complete call,
including callbacks. They do not re-register those inputs. Fresh output boxes,
iteration temporaries, diagnostics and obtained factories retain their existing
local protection. The Rust cold-bootstrap implementation remains.

Results use the current `fromWord:`/`fromBits:` factories. Errors use the current
Namespace Error binding and its `kind:message:` method, with String diagnostics.
The existing live ErrorVt binding determines incoming Error propagation, except
that equality returns false. Overflow, zeroDivide and typeMismatch retain their
established meanings. Receivers, arguments and intermediate results are rooted
across sends; no borrowed native state spans a dependency call.

Run `mise run native-messages`. Production checks cover arithmetic boundaries,
rounding, mixed operands, replacement, collecting callbacks, iteration and raw
payload tracing. Descriptor-based construction and numeric declarations v4/v5
are retired. Execution now uses the owner-local evaluation message below;
the first discarded-clone consumer also uses `canOmitIndexing`. Broader
allocation-elision remains pending; deferred materialization is not a prerequisite.

Collections use owner messages `collectionEquals:` (Object → Boolean) and
`collectionHash` (no arguments → native Word, low 32 bits). Integer preserves
exact-family signed equality and the existing 64-bit value mix independently of
public `=`. Private behaviors use identity; see [Map's key contract](../birnam-map/README.md#keys).

## Execution's scalar evaluator

`evaluate:receiver:argument:with:` recognizes the exact function/context pair of
this Integer instance's arithmetic entries, including its installed mixed-number
extension. Unknown entries return Unit before inspecting operands. Recognized
entries use the same validation, live ErrorVt propagation and mixed-number path as
ordinary arithmetic. Only a pair of exact Integers sends
`numericOperation:left:right:` to the supplied evaluator Object.

The evaluator receives three native Words: operation (1 add, 2 subtract, 3 multiply,
4 divide, 5 remainder), left signed-i64 bits and right signed-i64 bits. A Word
answer is boxed through the current `fromWord:`; Unit requests the ordinary
checked arithmetic/error path. Integer roots the evaluator across this synchronous
send, which uses fresh lookup and propagates failures. Integer owns operand
decoding and language semantics; Execution supplies arithmetic through its normal
receiver. No declaration table or foreign service cache is introduced.
Production Machine tests cover receiver replacement, precise GC during evaluation,
Unit fallback, exact failure propagation, old NativeEntry operand rejection,
overflow/zero division, live ErrorVt and replacement fallback.


## Arithmetic predicates

`evaluatePredicate:then:left:right:limit:in:epoch:with:` takes two Symbol selectors,
three operand Objects, compilation Namespace Object, expected native Word behavior
epoch and result receiver Object. For current owned arithmetic followed by
comparison on exact Integer inputs, it sends `commitNumericPredicate:` once with
an existing Boolean Object and returns the send's outcome. Refusal returns Unit
without sending. The receiver is rooted through delivery and temporary-root
cleanup. NativeEntry visitor operands are rejected.

The owner checks current lookup/entries, default index and allocation semantics,
live ErrorVt and the caller's epoch before its final input reads. The result
receiver must have default lookup; customized lookup selects baseline execution
so lookup effects cannot intervene between the final reads and delivery. Delivery
still selects the receiver's current method. Execution commits the pending inline
return before Integer releases roots. Both paths share the same owner-local
arithmetic/comparison helpers. Overflow, zero division, mixed inputs and failed
guards decline for baseline behavior. This consumer removes one intermediate
Integer; it does not construct an optimized numeric result.
[Details and evidence](../../docs/design/90-numeric-predicate.md).

Payload enumeration reads GC `precisePayloads` by message before each object.
In precise mode, raw bits are not references; in conservative mode the owner
checks its own payload words through Id `allocationAt:` and sends `visit:` for
matching allocations. Private behaviors keep the same representation ownership.
See [System GC controls](../../docs/design/authored-small-modules.md#system-gc).
