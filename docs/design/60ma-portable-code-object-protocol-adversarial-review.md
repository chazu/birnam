# 60MA — Adversarial Review: Portable Code Object Protocol

> **Status:** Review complete; the reviewed proposal was accepted 2026-08-25
> as [60M](60m-portable-code-object-protocol.md).
> This review attacks
> [60M — Portable Code Object Protocol](60m-portable-code-object-protocol.md).
> This record retains the adversarial rationale; 60M records acceptance and
> authorizes K1 implementation.

## 1. Review method

The review treated the public protocol as a long-lived language surface and
attacked it from eight directions: a reflective Birnam author, an independent
PortableProgram producer, a hostile object graph, an exact-value round trip, a
bounded-churn process, a future compiler, an image reader, and the frozen IdApi
floor.

## 2. Findings and resolutions

### A. A PortableProgram is not a CodeNode

**Attack:** Reuse 60K's `program -> CodeNode` wording and invent a Program node
that contains bindings and statements.

**Failure:** 60L closes Node tags at the eleven expression forms. A synthetic
root either becomes a hidden twelfth kind or makes source-visible object shape
disagree with the durable grammar. It also has nowhere honest to put the
statement forest and Let-definition relation.

**Resolution:** Reify the whole value as CodeProgram. CodeNodes remain exactly
the frozen expression kinds. **Pass.**

### B. Redundant arity and capture inputs create rival authorities

**Attack:** Keep `fromProgram:arity:captures:` and merely validate agreement.

**Failure:** The extra arguments add no expressive power, create additional
failure states, and invite tools to treat the redundant schema as editable
state. 60L already derives arity and capture partitions from the binding table.

**Resolution:** Constructors take only CodeProgram and optional provenance.
CodeObject accessors derive arity and capture schema. **Pass.**

### C. Exposing codec rows would privilege physical layout

**Attack:** Return Node indices, raw tags, fixed-width records, and operand
offsets because the production reader already has them.

**Failure:** That freezes codec mechanics into the language and makes a future
semantic-version adapter impossible without pretending offsets are meaning.

**Resolution:** Expose named semantic kinds, ordered accessors, bindings,
statements, and nested programs. Raw byte grammar remains in 60L. **Pass.**

### D. CodeNode Oop sharing could accidentally create DAG semantics

**Attack:** Memoize object identity while encoding. If one CodeNode Oop appears
twice, emit one Node record referenced by two parents.

**Failure:** 60L explicitly rejects shared Node children. Canonical bytes would
then depend on incidental Oop sharing and a valid ordinary graph could encode
as an invalid PortableProgram.

**Resolution:** Oop identity is nonsemantic; every operand occurrence emits a
complete occurrence. Cycles still reject. **Pass.**

### E. Treating Block captures as expression children changes meaning

**Attack:** Model `block:captureBindings:` as a nested program plus arbitrary
outer CodeNodes.

**Failure:** Frozen BlockLiteral operands are outer lexical-binding indices.
Allowing expressions would add evaluation order and executable work absent
from PortableProgram v1.

**Resolution:** Capture accessors and constructors use outer binding indices.
The nested CodeProgram exposes its own Capture binding views separately.
**Pass.**

### F. Reflecting a Float value may destroy exact admitted bits

**Attack:** Let `float` answer a runtime Float and let construction accept only
that Float.

**Failure:** Signalling NaNs may be quieted by an ABI or host operation, and
NaN payload identity is difficult to assert through numeric equality. 60L
requires every binary64 bit pattern to round-trip exactly.

**Resolution:** `floatBits` uses an exact eight-byte little-endian String.
Numeric convenience methods may be library additions but cannot replace the
bit-exact protocol. **Pass.**

### G. A second Let table can disagree with statements

**Attack:** Have CodeProgram construction accept `lets:` and `statements:` as
separate caller collections mirroring the byte sections.

**Failure:** A caller could supply missing, reordered, or multiply defined Let
bindings. Validation can reject these states, but the API makes invalid states
needlessly easy and exposes a codec normalization detail.

**Resolution:** Let statements carry their binding names. CodeProgram derives
the ordered Let partition and defining-statement indices. **Pass.**

### H. Generic `childAt:` blurs different edge kinds

**Attack:** Copy the parser CST's `childCount`/`childAt:` protocol onto every
CodeNode.

**Failure:** Send operands are expression children, but a Block also owns one
nested CodeProgram and outer lexical capture edges. Flattening those values
into one child list loses their roles and makes generic rewriting capable of
changing scope accidentally.

**Resolution:** Use `operandCount`/`operandAt:` for Sends and distinct
`program`/`captureBindingAt:` access for Blocks. The `...Count`/`...At:` shape
still follows Birnam's established indexed vocabulary. **Pass.**

### I. Retaining caller collections would make immutable nodes mutable

**Attack:** Store the supplied Vector or any duck-typed `size`/`at:` collection
as the semantic operand carrier.

**Failure:** A foreign collection may change behavior after construction, and
even persistent collection identity is not part of canonical meaning.

**Resolution:** Constructors synchronously read, type-check, bound, and copy
logical collection contents before returning. **Pass.**

### J. Interned inspection Symbols could become immortal history

**Attack:** Cache every decoded selector/global/Symbol strongly for the life of
the process.

**Failure:** Unique generated code would make a fixed live graph accumulate
Symbol and decoded-view history, violating 60J's plateau requirement.

**Resolution:** Canonical bytes own names. Runtime Symbol interning remains
weak, and decoded views/caches are collectable. A live view may naturally keep
the Symbols it exposes live, but dead views leave no semantic roots. **Pass at
the contract level; K1 and K6 retain executable proof.**

### K. Duck-typed fake nodes could smuggle mutable meaning

**Attack:** Accept any object responding to `kind` and the apparent accessors
as a CodeNode, then invoke it repeatedly while validating and encoding.

**Failure:** User methods may mutate, return inconsistent values, re-enter the
runtime, or allocate without bound between reads. Canonicalization would not
have one stable input meaning.

**Resolution:** The public factory sends are open and ordinary, but the values
they return are admitted immutable CodeNode/CodeStatement/CodeProgram families.
Whole-program construction accepts those admitted values, not arbitrary
lookalikes. Users retain full construction and transformation capability
without turning validation into repeated behavior execution. **Pass.**

### L. A CodeProgram object could become a second byte authority

**Attack:** Let CodeObject store bytes while a cached CodeProgram stores an
independently editable node graph.

**Failure:** The two representations can diverge, and image or materialization
code must choose which one to believe.

**Resolution:** All values are immutable; CodeObject retains one canonical byte
sequence. A standalone constructed CodeProgram is canonicalized before use,
and a decoded view must byte-round-trip. Caches are replaceable projections.
**Pass.**

### M. K1 could accidentally alter execution

**Attack:** Materialize newly constructed CodeObjects or change Block payloads
as part of proving that public constructors work.

**Failure:** That combines the reified-value seam with K2/K3 ownership,
capture-offset, dispatch, and retirement risks.

**Resolution:** K1 is inspection, construction, validation, codec reuse, and
GC behavior only. It does not make a CodeObject callable. **Pass.**

## 3. Rejected alternatives

1. **Synthetic Program CodeNode.** Rejected because it contradicts the frozen
   expression grammar and cannot honestly model the whole program.
2. **Opaque byte String only.** Rejected because it fails 60K's ordinary
   malleability requirement.
3. **Raw codec-record objects.** Rejected because offsets and indices are
   physical representation, not source-semantic vocabulary.
4. **Mutable builder as authority.** Rejected because active meaning and image
   authority require immutable, atomically admitted values. A future
   convenience builder may produce the same immutable inputs.
5. **One generic property Map per node.** Rejected because misspelled keys and
   unchecked value roles defer errors and weaken discoverability.
6. **Protocol-based arbitrary node lookalikes.** Rejected for constructors
   because repeated user sends cannot supply a stable atomic snapshot.
7. **Compiler-only constructors.** Rejected because the compiler must remain
   one producer rather than the privileged owner of code meaning.

## 4. Residual implementation risks, not design blockers

1. K1 must choose a private runtime allocation seam without adding an IdApi
   field or treating a loadable module's globals as durable roots.
2. Iterative graph conversion must root every temporary Oop across allocation
   and re-entrant ordinary sends while enforcing aggregate limits.
3. Weak decoded-view caching must be measured under churn rather than inferred
   from Rust ownership.
4. The public prototypes and their native layouts need image-v7 treatment in
   K5; K1 must not imply that legacy images already preserve them.
5. Exact constructor error messages may improve, but their kinds and atomicity
   must remain as specified by 60M.

## 5. Verdict

**Accepted 2026-08-25.** The distinct CodeProgram root is the smallest honest public model of frozen
PortableProgram v1. The stabilized protocol preserves ordinary sends and
source-level construction while keeping canonical bytes, lexical scope,
execution materialization, and Oop identity separate. No adversarial finding
requires changing the central shape. Acceptance of 60M closes the public
surface gate and authorizes K1 implementation.
