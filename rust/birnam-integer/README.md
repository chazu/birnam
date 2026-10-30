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
Integer behavior rule: a changed behavior is not an exact Integer. The resident
module state stores its own selectors, identities and optimization Method witnesses;
it does not inspect foreign layouts or cache dependency executables.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Integer `fromWord:` | Word or immediate size Oop | Integer interpreting the Word as signed 64-bit bits. |
| Integer `wordOf:` | Object | Word or immediate size Oop for an exact Integer, absent reference otherwise. |
| Integer instance `word` | none | Word or immediate size Oop containing its signed bits. |
| Integer instance `+`, `-`, `*`, `/`, `mod:` | Object | Checked i64 arithmetic, or Error. Division truncates toward zero; MIN mod -1 is zero. |
| Integer instance `=` | Object | Exact Integer value equality, false for another family. |
| Integer instance `<`, `<=`, `>`, `>=` | Object | Comparison, preserving existing f64 comparison rounding. |
| Integer instance `printString` | none | Decimal String through current `fromBytes:`. |
| Integer instance `to:do:` | end Integer, callable Object | Inclusive iteration by live `value:` sends; receiver on completion, first Error on failure. MAX is visited without overflowing the loop counter. |
| Integer `evaluate:receiver:argument:with:` | selected Method, receiver, argument, evaluator object | Result object for a recognized arithmetic Method; absent reference for an unknown Method. See below. |
| Integer `canOmitIndexing` | none | Immediate size 1 when the current `fromWord:`/`wordOf:` Methods and allocation path are ordinary; size 0 otherwise. Used for omitted compiler-generated PC/index operations. |
| Integer `installFloat:` | completed Float Object | Absent reference; install mixed arithmetic/comparisons and `asFloat`. |

Mixed arithmetic Methods capture the completed Float object as their Oop context.
There is no leaked native extension record or permanent Roots token.
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

Run `bash scripts/oop-id-slice-test.sh`. A fresh-process numeric check loads the
real Integer, Float, String and Bytes owners together. It exercises arithmetic,
signed limits, parsing, mixed operations, ErrorVt replacement, iteration,
collecting callbacks and optimization guards. Error is an independent C test
provider. Execution/compiler, image and worker integration remain pending.

Collections use owner messages `collectionEquals:` (Object → Boolean) and
`collectionHash` (no arguments → immediate size Oop, low 32 bits). Integer preserves
exact-family signed equality and the existing 64-bit value mix independently of
public `=`. Private behaviors use identity; see [Map's key contract](../birnam-map/README.md#keys).

## Execution's scalar evaluator

`evaluate:receiver:argument:with:` recognizes ordinary Method objects against this
owner's retained bootstrap Methods, including installed mixed arithmetic. These
witnesses support optimization checks; ordinary dispatch still uses current Id
lookup. Their code, context and owner must match. Copied native Methods
can match without sharing identity. Unknown Methods decline before reading inputs.

The evaluator receives three Oops: operation (1 add, 2 subtract, 3 multiply,
4 divide, 5 remainder), left signed-i64 bits and right signed-i64 bits. Sizes or
Word objects preserve the full width. A result goes through current `fromWord:`;
an absent reference requests ordinary checked arithmetic. Integer protects a
new left Word while constructing the right one. Calls use current lookup and
propagate failure returns separately from ordinary Error objects.

The native Method owner implements a bulk comparison for copied/unknown Methods.
It reads the selected Method once and checks candidates without per-candidate Id
messages or a new registry. Comparison checks the selected code, context and owner; it does not consult an
application selector on the Method.

## Arithmetic predicates

`evaluatePredicate:then:left:right:limit:in:epoch:with:` takes two Symbol selectors,
three operand Objects, compilation Namespace Object, expected size Oop behavior
epoch and result receiver Object. For current owned arithmetic followed by
comparison on exact Integer inputs, it sends `commitNumericPredicate:` once with
an existing Boolean Object and returns the send's result. Refusal returns an absent reference
without sending. The receiver is rooted through delivery and temporary-root
cleanup. Only Oops cross this boundary.

The owner checks current lookup/entries, default index and allocation semantics,
live ErrorVt and the caller's epoch before and after its final input reads. The result
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

## Private construction roots

New construction/validation values use a nested owner-local scope reported by
this module's registered `rootsDo:` provider, following Execution's existing
pattern. The scope appends before callbacks and restores its previous length
at return or failure. Provider enumeration snapshots the inventory before
visitor sends. Fresh values remain protected; no public Root tokens are needed
for this private inventory. Inputs still borrow the caller's Id lifetime.
Current allocator/getter/factory messages and shape checks remain live. See
[implementation and validation](../../docs/archive/2026-10-runtime-refactors/helper-lifetime-and-boxing.md).

## Atomic representation facts

The owner uses Id `tryRepresentationIdentityOf:behavior:bytes:` for exact behavior,
extent and nonzero allocation identity. Canonical admission is callback-free and
consumed immediately. Unit/UNBOUND uses the original ordered getter sequence;
other failures propagate. Metadata/query/lookup replacement declines admission.
See [the contract and evidence](../../docs/archive/2026-10-runtime-refactors/representation-validation-and-lifetime.md).
