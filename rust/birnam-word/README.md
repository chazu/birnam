# Word

The native initializer takes Namespace (Object), publishes `Word` and `WordVt`,
and returns Word. Bootstrap loads it before authored numeric policy. Repeating
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

## Opaque identity selection

`Word selectIfSameObject:with:ifEqual:ifDifferent:` takes four native Object
operands and returns the supplied equal or different Object by opaque identity.
The default method has no allocation, payload inspection, outgoing message or
branch invocation. The caller keeps all inputs live under the existing Id ABI;
invalid count/kind/null carriers fail with `INVALID_REQUEST`. The operation remains
an ordinary replaceable Id method. It replaces Boolean's temporary comparison-Word
round trip while Boolean policy and branch invocation remain above this owner.
See [the measured result](../../docs/archive/2026-10-runtime-refactors/boolean-selection-simplification.md).

## Native input lifetime

Numeric entries and guarded expression evaluation borrow receiver/Object argument
coverage from the Id invocation contract, including callback reentry. They no
longer register the same inputs through Roots. The expression still snapshots its
incoming byte plan and operand array before callbacks and retains current
permission/lookup/epoch checks. Newly allocated outputs keep their first Root
through representation validation; these are not covered by caller input roots.
No payload access crosses representation owners.

## Current expression admission

The native `tryPrimitiveExpression:` and `tryPrimitiveExpressionAtEpoch:` entries
check each operation against its current Method witness. They do not remember
primitive meanings by epoch: Method data can change without a dictionary edit,
including during another admission callback. Inputs and the byte program remain
protected under the existing call and snapshot rules.

Allocation omission is checked before work and again before returning a result,
after admission callbacks. A callback can change allocator Method data without
advancing the binding epoch; the final check prevents losing constructor effects.
The epoch check remains an additional check for dictionary changes. A refused
expression produces no arithmetic result, and native failures still propagate.

See the [current-guard investigation](../../docs/measurements/word-expression-current-20261031/README.md)
for regressions, measurements and the cost of removing the unsound shortcut.

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
