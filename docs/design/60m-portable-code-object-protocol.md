# 60M — Portable Code Object Protocol

> **Status:** Accepted 2026-08-25 after adversarial review and discussion.
> This record sharpens the ordinary-object surface accepted in
> [60K — Reified Portable CodeObjects](60k-portable-codeobjects.md) against the
> physical program model frozen by
> [60L — PortableProgram v1 and Image-v7 Byte Grammar](60l-portable-program-and-image-v7-byte-grammar.md).
> Acceptance authorizes K1 implementation. The accompanying
> [60MA adversarial review](60ma-portable-code-object-protocol-adversarial-review.md)
> records the rejected alternatives and the corrections incorporated here.

## 1. Decision in one paragraph

`PortableProgram` is reified as an immutable ordinary `CodeProgram`, not as a
synthetic `CodeNode`. A CodeProgram owns ordered lexical-binding and statement
views; its expressions are immutable CodeNodes. CodeObject construction takes
one CodeProgram plus optional provenance. Arity and capture schema are always
derived from that program, so no constructor accepts redundant values that
could disagree. Every semantic component is inspectable and every admitted v1
program is constructible through ordinary sends. Canonical bytes remain the
authority: decoded object identity, cache identity, and repeated references to
the same CodeNode Oop have no portable meaning.

## 2. Why 60K needs this sharpening

60K's minimum protocol says that `program` answers a CodeNode and that
CodeObject construction also accepts arity and capture schema. The frozen 60L
grammar makes both details untenable:

- a PortableProgram contains parameter, capture, and Let bindings plus an
  ordered statement forest; no one Node is its root;
- the eleven frozen Node tags are expression forms, so inventing a twelfth
  in-memory-only Program node would blur the durable grammar;
- program arity and capture schema are already semantic fields derived from
  its binding partitions; and
- Let statements, nested programs, exact Float bits, and BlockLiteral capture
  bindings need explicit ordinary-object representations.

This record changes no accepted portable meaning. It makes that meaning usable
without handing source code privileged access to codec records or compiler IR.

## 3. Ordinary immutable values

K1 introduces five public families:

| Family | Meaning |
|---|---|
| `CodeObject` | portable executable meaning plus optional provenance |
| `CodeProgram` | one complete PortableProgram semantic value |
| `CodeLexicalBinding` | a context-bearing view of one program binding |
| `CodeStatement` | one Expression or Let statement |
| `CodeNode` | one of the eleven v1 expression forms |

All five are ordinary Oops and receive ordinary sends. Their semantic fields
are immutable. Source-level transformation constructs successor values; it
does not mutate canonical bytes or an active CodeObject in place.

Immutability applies to semantic state, not to the family's method dictionary.
Reflect may still replace or extend methods on these ordinary families. Trusted
codec, image, and materialization paths read admitted private state rather than
reconstructing authority by sending replaceable public accessors; changing an
accessor therefore changes ordinary observed behavior without rewriting the
CodeObject's canonical portable meaning.

`CodeProgram`, `CodeLexicalBinding`, `CodeStatement`, and CodeNode Oop identity
is nonsemantic. A decoder may answer fresh views on different inspections and
may keep only weak caches. CodeObject retains the ordinary identity accepted in
60K, but two CodeObjects with equal bytes still need not be identical.

## 4. Exact minimum protocol

The selectors below are the K1 minimum. Later library conveniences may be
defined in Birnam without changing the native contract.

### 4.1 Block and CodeObject

```birnam
[aBlock codeObject]
[aBlock captureCount]
[aBlock captureAt: index]

[aCodeObject format]
[aCodeObject arity]
[aCodeObject captureSchema]
[aCodeObject program]
[aCodeObject digest]
[aCodeObject sourceProvenance]
[aCodeObject withProgram: aCodeProgram]
[aCodeObject withoutSourceProvenance]

[CodeObject fromProgram: aCodeProgram]
[CodeObject fromProgram: aCodeProgram provenance: provenance]
```

`format` answers `#portableProgramV1`. `captureSchema` answers an immutable
Vector of the same CodeLexicalBinding values visible through the program's
`captureAt:` order. `fromProgram:` is exactly the two-argument constructor with
`nil` provenance. `withProgram:` retains the receiver's provenance.

There is deliberately no `arity:` or `captures:` constructor argument. Both
are derived from the admitted CodeProgram before the CodeObject is returned.

### 4.2 CodeProgram

```birnam
[aCodeProgram format]
[aCodeProgram arity]
[aCodeProgram parameterCount]
[aCodeProgram parameterAt: index]
[aCodeProgram captureCount]
[aCodeProgram captureAt: index]
[aCodeProgram letCount]
[aCodeProgram letAt: index]
[aCodeProgram bindingCount]
[aCodeProgram bindingAt: index]
[aCodeProgram statementCount]
[aCodeProgram statementAt: index]

[CodeProgram parameters: parameterNames
             captures: captureNames
             statements: statements]
```

The three constructor arguments are finite ordinary collections responding to
`size` and `at:`. Parameter and capture entries are exact nonempty NUL-free
String bytes. Statement entries are CodeStatements. Let bindings are derived,
in statement order, from `#let` statements; there is no second Let table for a
caller to keep in agreement.

`bindingAt:` traverses Parameters, Captures, then Lets, matching 60L's binding
index space. The partition-specific `...At:` selectors traverse their own
dense zero-based ordinals. Inspection selectors ending in `At:` answer `nil`
for a non-Integer, negative, or out-of-range index, matching Birnam collection
inspection. Counts disambiguate a missing entry from a semantic `nil` value.

### 4.3 CodeLexicalBinding

```birnam
[aCodeLexicalBinding kind]
[aCodeLexicalBinding index]
[aCodeLexicalBinding ordinal]
[aCodeLexicalBinding name]
[aCodeLexicalBinding definingStatementIndex]
```

`kind` is exactly `#parameter`, `#capture`, or `#let`. `index` is the binding's
complete-program index. `ordinal` is its dense index within its kind.
`definingStatementIndex` answers `nil` for Parameters and Captures and the
defining statement's dense index for a Let. A binding view is produced by a
CodeProgram; it has no independent public constructor and cannot be attached
to a different program as authority.

### 4.4 CodeStatement

```birnam
[aCodeStatement kind]
[aCodeStatement expression]
[aCodeStatement bindingName]

[CodeStatement expression: aCodeNode]
[CodeStatement letNamed: name expression: aCodeNode]
```

`kind` is `#expression` or `#let`. `expression` answers the statement root.
`bindingName` answers the exact String name for a Let and `nil` for an
Expression. The enclosing CodeProgram derives the Let's binding index and
defining-statement relation from statement order.

### 4.5 CodeNode

Every CodeNode answers `kind`, whose result is exactly one of:

```text
#integer #float #string #symbol #nil #true #false
#lexicalRead #globalRead #send #block
```

The minimum inspection protocol is:

```birnam
[aCodeNode kind]
[aCodeNode integer]
[aCodeNode floatBits]
[aCodeNode string]
[aCodeNode symbol]
[aCodeNode bindingIndex]
[aCodeNode global]
[aCodeNode selector]
[aCodeNode operandCount]
[aCodeNode operandAt: index]
[aCodeNode program]
[aCodeNode captureCount]
[aCodeNode captureBindingAt: index]
```

An accessor that does not apply to the receiver's kind answers `nil`.
`operandCount` and `captureCount` answer zero when inapplicable. Indexed
inspection also answers `nil` for an invalid index. `integer` answers an
Integer. `floatBits` answers an exact eight-byte little-endian String so every
admitted binary64 pattern, including signalling NaNs, survives reflection.
`string` answers a String and `symbol`, `global`, and `selector` answer Symbols.

For a Send, operand zero is the receiver and the remaining operands are
arguments in order. For a Block, `program` is the nested CodeProgram and each
`captureBindingAt:` result is the outer program's binding index for that nested
capture position. A Block's captures are lexical bindings, not arbitrary
expression children.

The public constructors are:

```birnam
[CodeNode integer: value]
[CodeNode floatBits: eightLittleEndianBytes]
[CodeNode string: value]
[CodeNode symbol: value]
[CodeNode nilLiteral]
[CodeNode trueLiteral]
[CodeNode falseLiteral]
[CodeNode lexicalReadAt: bindingIndex]
[CodeNode globalRead: symbol]
[CodeNode send: selector operands: codeNodes]
[CodeNode block: aCodeProgram captureBindings: bindingIndices]
```

`operands:` and `captureBindings:` accept finite `size`/`at:` collections and
copy their logical contents before returning. Selector arity, value kinds,
name constraints, and local per-node quantity limits are checked immediately.
Whole-program scope, Let timing, nested capture agreement, aggregate bounds,
depth, and canonical encoding are checked by CodeProgram construction.

## 5. Tree occurrence and canonicalization rules

PortableProgram v1 is a tree, not a DAG. Object construction therefore treats
each occurrence in an operand collection as a separate semantic occurrence.
If the same CodeNode Oop occurs twice, encoding emits its complete subtree
twice. Oop sharing cannot change canonical bytes. Decoding likewise need not
preserve or invent sharing.

Construction walks the supplied graph iteratively, rejects cycles and resource
limit excess before publication, and emits the exact 60L left-to-right
postorder. It does not expose Node indices, accept codec records, retain caller
collections, or trust a caller-provided validity flag. Encoding a decoded
CodeProgram must reproduce the complete original PortableProgram bytes.

Nested CodePrograms retain their own binding index spaces. A BlockLiteral's
capture-binding indices are interpreted only in its enclosing program; the
nested program's Capture bindings retain the distinct inner indices exposed by
that nested CodeProgram.

## 6. Errors and atomicity

Inspection is side-effect free and total under the `nil`/zero rules above.
Constructors either answer a completely admitted immutable value or a kinded
Error. They never answer a partially valid object, mutate an input collection,
substitute a default value, or publish a CodeObject before complete validation.

K1 uses the existing `#typeMismatch` kind for wrong input families and one new
`#invalidPortableProgram` kind for malformed scope, selector, tree, or nested
program relationships. Resource-ceiling failures answer
`#portableProgramLimit`. Error messages identify the rejected field or
relationship but are diagnostic text, not protocol authority.

## 7. Authority and lifetime

Canonical PortableProgram bytes are the only program authority. A CodeProgram
is an ordinary decoded/constructed semantic view over those bytes, not a
second independently mutable representation. Implementations may choose a
byte-owning CodeProgram internally while it is standalone, but CodeObject
construction must canonicalize it and retain exactly one authoritative byte
sequence.

Decoded views, binding vectors, Symbols created for inspection, and digests may
be weakly cached. No such cache roots a CodeObject or becomes required for
future decoding. K1 adds no materialization, Block-layout change, Store query,
dispatch case, or IdApi field.

### 7.1 Native rung ownership

**Accepted 2026-08-25.** The five public families are owned by the dedicated
native Portable Code rung, loaded after Collection and Error. They are not
installed by the identity floor and are not present in a partially usable
pre-Error state. The rung depends only on frozen `IdApi`, the pointer-free
PortableProgram codec, and the ordinary `Vector` and `Error` protocols.

This placement makes dependency readiness structural: when `CodeObject` is
publicly reachable, immutable Vector construction and kinded Error construction
are already available. Reload reuses the exported family prototypes and
vtables, replacing their ordinary method dictionaries and native hooks rather
than accumulating successor prototypes. The rung owns a scoped root provider
for reentrant construction; no temporary-root seam or CodeObject field is added
to the floor.

Later trusted image and materialization paths may use a private versioned seam
exported by this rung to read admitted canonical bytes. Such a seam is not an
`IdApi` extension and does not make replaceable public accessors codec
authority.

## 8. K1 acceptance and proof

Acceptance of this record authorizes a test-first K1 implementation with this
minimum proof:

1. all thirteen stored and two generated PortableProgram fixtures decode into
   ordinary views and re-encode byte-exactly;
2. ordinary sends inspect every binding, statement, and Node form, including
   non-UTF-8 names and exact Float bits;
3. ordinary constructors reproduce every admitted form and reject wrong types,
   scope errors, cycles, sharing-as-DAG, and resource overflow atomically;
4. repeated references to one CodeNode Oop encode as repeated tree occurrences;
5. CodeObject arity and capture schema can never disagree with its CodeProgram;
6. decoded-view caches are collectable and do not become semantic roots; and
7. the complete workspace and frozen native IdApi client/module fixture remain
   green; archived v4 image boot is not a current compatibility requirement.

This K1 protocol slice is non-executing. K2 was the first stage allowed to
change Block layout, and K5 supplied image-v7 boot admission; both completed
2026-08-26.

## 9. Accepted decision

**Accepted 2026-08-25.** K1 uses the distinct CodeProgram root and the exact
minimum protocol above. The principal rejected alternative is a synthetic
Program CodeNode plus redundant constructor metadata; 60MA shows why that
would create two authorities and an in-memory node kind absent from the frozen
grammar.
