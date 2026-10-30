# 60K — Reified Portable CodeObjects

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

> **Status:** Semantic design accepted 2026-08-25 after adversarial review and
> decision-by-decision discussion; K1-K6 implementation completed 2026-08-27.
> The
> normalized source-semantic program layer, ordinary-object surface, closure
> layout, materialization lifetime, image-v7 direction, compatibility boundary,
> and bounded-churn obligations are specified below. The accompanying
> [60KA adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/60ka-portable-codeobjects-adversarial-review.md)
> records the corrections incorporated before discussion. Acceptance authorizes
> [Design 60L's physical-format work](../../design/60l-portable-program-and-image-v7-byte-grammar.md),
> which was accepted and frozen 2026-08-25. The K1 public surface is sharpened
> by the accepted
> [Design 60M](../../design/60m-portable-code-object-protocol.md); K1 implementation is now
> authorized. The authority split in §4 was
> accepted 2026-08-25: CodeObject owns portable executable meaning; compiled
> units and vtable dictionaries remain the primary physical executable and live
> dispatch layer. CodeObject and CodeNode immutability was accepted the same
> day: transformation constructs a successor value and live change replaces a
> Block or binding rather than mutating code beneath an active Frame. Ordinary
> Oop identity for CodeObjects was also accepted: digests are integrity/cache
> keys, and optional weak deduplication requires complete byte equality. The
> portable Block payload was accepted as one CodeObject Oop followed by its
> captures; the non-callable zero-payload Block family prototype remains an
> explicitly distinct schema. Reflect's implementation argument remains
> protocol-based rather than Block-restricted; non-Block callables require
> their own honest durable reconstruction contract for image save. Authoring
> source is optional, nonsemantic provenance: stripping it changes textual
> fidelity and diagnostics, never behavior, digest, or materialization choice.
> New durable saves make a clean image-v7 cut; v5/v6 retain their frozen legacy
> readers and are never ambiguously reinterpreted. A reachable Block remains a
> native-materialization owner in the first implementation; cross-generation
> re-homing may optimize that later but is not required for correctness.
> Image-v7 persists current CodeBindings directly as target, selector,
> implementation object, and optional provenance; source replay is not required
> to reconstruct current methods. A separate
> [Design 60L](../../design/60l-portable-program-and-image-v7-byte-grammar.md) owns the exact
> PortableProgram v1 and image-v7 byte grammars frozen before implementation.
> Its D1 authority decision was accepted 2026-08-25: v7 derives a reproducible
> source-independent portable image-code authority rather than retaining the
> v5/v6 source-derived DispatchGeneration digest as an opaque token.
> 60L D2 was accepted the same day: the v7 CodeObject image payload is exactly
> `[PortableProgramRef, provenance]`; arity, capture schema, and digest remain
> ordinary semantic fields derived from the immutable program authority.
> 60L D3 was also accepted: publication generation is live process-local
> carrier/lifetime metadata, not durable CodeBinding meaning; v7 recovery
> publishes a fresh successor generation rather than serializing an old one.
> 60L D4 was accepted in favor of fixed-width node/operand tables in canonical
> left-to-right postorder; PortableProgram v1 has no recursive node stream.
> 60L D5 was accepted in favor of nonempty NUL-free binding, global, Symbol, and
> selector bytes; current parser spellability is not an object-authority limit.
> 60L D6 was accepted in favor of preserving every IEEE-754 binary64 bit
> pattern, including exact signalling/quiet NaN payloads.
> 60L D7 was accepted in favor of preserving explicit ordered capture schemas,
> including unused and repeated outer bindings; source compilation still emits
> only actual free variables in first-use order.
> PortableProgram has no fallback interpreter: execution requires either an
> admitted compiler/materializer or a compatible native unit. CodeNodes are
> publicly inspectable and constructible ordinary objects in v1; validated
> successor CodeObjects can be authored without privileged compiler access.
>
> This record extends
> [60F — Resumable Compiler Lowering](60f-resumable-compiler-lowering.md),
> [60I — Stackless Artifact Byte Grammar](../../design/60i-stackless-artifact-byte-grammar.md),
> and
> [60J — Runtime Identity and History Compaction](60j-runtime-identity-and-history-compaction.md).
> It amends their assumption that a generated Block's durable executable
> identity must be recovered through a source revision and one captured native
> artifact. It does not enlarge `IdApi`, change ordinary lookup, or make Store
> part of the send hot path.

## 1. Decision in one paragraph

Every portable Birnam Block owns an immutable, ordinary `CodeObject` plus its
captured values. The CodeObject contains a canonical pointer-free
`PortableProgram` at the normalized language-semantic layer, its signature and
capture schema, and optional source provenance. It does not contain native
addresses, compiler-routing annotations, physical Frames, or a required source
string. Compilers lower the PortableProgram into any valid synchronous or
stackless materialization. Native code, descriptors, thunks, registrations, and
private block vtables are disposable process state. A live Block, method
binding, Frame, Continuation, or protected dispatch reader keeps the required
materialization live; removing those owners permits quiescent retirement while
the CodeObject remains an independently inspectable and rematerializable value.

## 2. Why this is necessary now

The current runtime has two incomplete durable stories for a Block:

1. source recovery can reparse a recorded definition; and
2. mapped recovery can use a `ResumableManifest` row and the matching captured
   native unit.

Neither makes the Block's executable meaning an object in the image. A Block
whose authoring source is unavailable cannot be rebuilt from a manifest or
FrameDescriptor: those values identify and describe a materialization, but do
not contain the program. Conversely, keeping every old TCC unit, unique Globals
root, generated Symbol, and vtable alive merely because it once implemented a
method makes retained space proportional to process age.

60J requires retained space to be bounded by live semantic state, explicit
history, live identities, and active or quiescing code. Reified CodeObjects make
the missing semantic owner explicit. Quiescence may then reclaim a native
materialization; ordinary reachability decides whether the program and captures
remain live.

## 3. Canonical vocabulary

| Term | Meaning |
|---|---|
| `PortableProgram` | Canonical, target-independent, pointer-free source-semantic program for one Block body. |
| `CodeObject` | Immutable ordinary object containing one PortableProgram, signature, capture schema, digest, and optional provenance. |
| `Block` | Callable closure object whose payload is one CodeObject followed by the captured Oops required by that CodeObject. |
| `CodeBinding` | Current semantic association between a live target/selector pair and an implementation object. The implementation is usually a Block but may be another callable object. |
| `Materialization` | Process-local executable realization of a CodeObject: native functions, descriptors, registrations, thunks, and block vtables. |
| `CodeGeneration` | Lifetime and publication owner for one or more native materializations. It remains the process-local code-reclamation unit. |
| `SourceProvenance` | Optional authoring bytes and source-map identity. It improves editing and diagnostics but is not required to execute or recover a CodeObject. |

“CodeObject” does not mean a stack Frame, Block instance, compiler AST pointer,
native function, `CompiledUnit`, source definition, or manifest row.

## 4. Authority split

**Accepted 2026-08-25.** CodeObject is the canonical portable meaning of a
closure. Compatible compiled units and live vtable dictionaries remain the
canonical physical execution and live-dispatch layer. This acceptance does not
yet accept the remaining 60K representation, lifecycle, image, or rollout
choices.

The following authorities remain distinct:

| Question | Authority |
|---|---|
| What program does this closure mean? | CodeObject PortableProgram |
| What values did this closure capture? | Block payload |
| What implementation is currently installed on this target and selector? | live object dictionary, mutated through the Store-arbitrated MOP |
| What portable implementation object backs that binding? | current CodeBinding |
| Which native carrier executes it in this process? | Runtime registration and CodeGeneration |
| Why and where was it authored? | optional SourceProvenance |
| What was true at an earlier retained basis? | Store history within its explicit retention window |

This refines, rather than discards, “compiled units are primary.” Compatible
compiled units remain the primary **physical executable layer** and ordinary
dispatch still reads native vtable dictionaries. CodeObjects become the primary
**portable executable meaning** of closures. Source and Store facts do not
become an interpreter or a send-path authority.

## 5. Constitutional invariants

An implementation of this record is acceptable only if all of these remain
true.

1. **Ordinary sends remain ordinary.** Block invocation, method invocation,
   `Reflect defineMethod:on:as:`, replacement, and removal use the existing
   object/send surface. No Store query or CodeObject decoding enters every send.
2. **Code is semantic state, not compiler direction.** Nothing attached to a
   source definition instructs one compiler to choose a hidden backend. A
   PortableProgram says what the Block does; a separately materialized
   `ArtifactSelection` still says which execution disposition was selected.
3. **Source is optional.** Dropping SourceProvenance may degrade source-level
   editing and diagnostics, but never changes program behavior or prevents
   compatible rematerialization.
4. **Compiler internals remain replaceable.** Logical CFGs, liveness, slot
   coloring, Frame layouts, generated C, and optimizer choices are derived.
   They never occur in canonical PortableProgram bytes.
5. **Captures are values, not code.** A CodeObject contains a capture schema;
   each Block contains the corresponding captured Oops. Two Blocks may share a
   CodeObject and differ in captures.
6. **Malleability is object-level.** CodeObjects and their decoded CodeNode
   values are inspectable and constructible through sends. Live change creates
   a new immutable CodeObject or Block and publishes a replacement binding; it
   does not mutate bytes underneath active Frames.
7. **No hidden immortal roots.** A compiler cache, Symbol interner, manifest
   catalog, source table, or materialization repository is not by itself a
   semantic root.
8. **Current reachability wins over quiescence.** Quiescence permits retirement
   only after publication owners are withdrawn. It never proves that an
   otherwise reachable Block will not be called again.
9. **Images are honest.** A current image contains enough PortableProgram and
   capture state to rebuild every admitted portable Block without its original
   source or prior native mapping.
10. **The floor stays frozen.** The feature adds no `IdApi` field and no new
    dispatch case. Private versioned host/runtime seams may create, validate,
    materialize, and retire objects above that floor.
11. **Failure is explicit.** Invalid portable bytes, incompatible versions,
    resource ceilings, nonportable implementations, and materialization failure
    reject construction, installation, save, or load with a kinded Error. They
    do not silently substitute source, `nil`, or an old native carrier.
12. **Space is proportional to live authority.** With a fixed live graph and
    bounded history, repeated definition/replacement/removal reaches a plateau
    after collection and code quiescence.

### 5.1 Portable Code rung placement

**Accepted 2026-08-25.** Reified portable-code values belong to a dedicated
native rung above Collection and Error, not to the identity floor. This keeps
the bootstrap kernel free of language-semantic payloads while ensuring that
every publicly available portable constructor can answer the kinded Errors and
immutable Vectors required by Design 60M. The rung uses ordinary vtables,
methods, exports, allocation, tracing, and root-provider registration through
the frozen `IdApi`; it adds no dispatch case or floor field.

The host expresses only the dependency order. It does not activate hidden
floor functionality. Reload preserves existing family identity and object
compatibility by reusing their exported vtables and replacing methods/hooks in
place. Private versioned codec/materialization seams, when K5/K2 require them,
are owned by this rung and remain outside ordinary dispatch.

## 6. CodeObject and Block semantics

### 6.1 CodeObject is an immutable value

**Accepted 2026-08-25.** CodeObjects and CodeNodes are immutable values.
Inspection and construction remain ordinary sends; an edit answers a successor
CodeObject and publication replaces the relevant Block or CodeBinding. An
active Frame therefore retains one exact program meaning for the lifetime of
its captured materialization.

A CodeObject has these semantic fields:

| Field | Meaning |
|---|---|
| format | exact PortableProgram schema and semantic version |
| arity | number of invocation arguments accepted by the Block |
| capture schema | ordered capture entries used by Block construction and entry materialization |
| program bytes | complete canonical pointer-free PortableProgram encoding |
| digest | domain-separated SHA-256 of the complete canonical PortableProgram bytes |
| provenance | optional SourceProvenance or `nil` |

Program bytes are the one stored program authority. `program` decodes them into
an immutable ordinary CodeNode view; encoding that view must reproduce the
bytes exactly. A CodeObject never stores an independently mutable byte stream
and node graph that could disagree. The view may be cached weakly, but the cache
is neither durable authority nor a semantic root.

The digest is an integrity and cache key, not object identity and not sole
lookup authority. Equal canonical bytes may be represented by distinct
CodeObject Oops. An implementation may weakly deduplicate them only after
byte-exact comparison. Re-interning after collection may produce a fresh Oop.

**Accepted 2026-08-25.** CodeObjects retain ordinary Oop identity. No digest,
portable implementation identifier, or mandatory interning table becomes an
identity floor. Weak byte-exact deduplication is permitted only as a replaceable
cache.

CodeObject fields cannot be changed in place. Transformations answer a new
CodeObject after full validation. This protects active Frames from seeing a
program change underneath a captured descriptor while preserving live
replacement as an ordinary operation.

Each capture-schema entry has a dense zero-based index and a reflected lexical
name. The index is semantic; the name supports inspection and reconstructed
source. V1 captures are all Oops, so the schema contains no physical slot kind
or Frame location. Each nested BlockLiteral supplies exactly one in-range outer
operand per capture-schema index, in schema order. A body may read a capture
zero, one, or many times, and distinct entries may intentionally receive the
same outer binding. These ordinary inspectable edges are semantic Block state,
not compiler-generated hidden roots.

### 6.2 Block layout

**Accepted 2026-08-25.** A portable Block stores exactly one CodeObject word
followed by its ordered captures. This adds one Oop word per closure on the
current ABI and permits any number of closures to share one CodeObject while
retaining different capture values.

Every new portable Block has this all-Oop payload:

```text
word 0       CodeObject
word 1..C    captures in the CodeObject's canonical capture order
```

Thus the payload occupies `(C + 1) * pointerBytes`. Entry materialization copies
capture `i` from payload word `i + 1`; word zero is never a language capture.
The all-Oop rest layout remains precise-GC compatible.

Block creation validates:

- the value in word zero is a CodeObject admitted by the current semantic
  version;
- the payload has exactly `captureCount + 1` words;
- invocation selector arity agrees with CodeObject arity; and
- every capture is rooted until stored in the new Block.

The public reflective surface includes at least `codeObject`, `captureCount`,
and bounds-checked `captureAt:`. It does not expose raw payload offsets as the
semantic API.

#### 6.2.1 K2 native capability seam

**Accepted 2026-08-26.** Generated code reaches the Portable Code rung through
one private versioned `PortableCodeApiV1` capability, not through an IdApi
extension, a source annotation, an ordinary constructor send, or a direct
artifact dependency on the rung dylib. The rung exports one reload-stable raw
capability object containing its ABI version, exact byte size, and native
`admitProgram` and `newBlock` entry pointers. Codegen validates that object and
binds thin shims into each JIT or AOT unit.

The compiler derives canonical PortableProgram bytes directly from the lowered
Block AST and embeds those bytes in the native unit. The generated unit admits
each block body's bytes once, retains the resulting CodeObject in the existing
generation-owned root cache, and shares it among every closure created for that
body. No source definition carries compiler-control metadata.

`newBlock` receives the private Block vtable, admitted CodeObject, and rooted
capture array. On first use it validates the exact `value:`-family arity,
installs the portable all-Oop schema and reflective methods, and gives the
vtable a private carrier owner whose sole Oop field is that CodeObject. Reuse
requires the same admitted meaning. The carrier is the schema marker and
lifetime edge: no payload-size guess or permanently growing side registry is
allowed. The constructor then atomically allocates and fills
`[CodeObject, captures...]` or answers a kinded Error.

The stackless floor learns that physical layout through one append-only
registration carrier, not by importing Portable Code or performing reflective
sends. `birnam_register_resumable_v1` remains frozen: a v1 BlockBody's captures
begin at receiver payload word zero, as required by existing image-v5/v6 and
native artifacts. New compiler output uses `birnam_register_resumable_v2`.
V2 is the exact 136-byte v1 prefix followed by:

```c
uint32_t receiver_capture_offset_words; /* 0x88 */
uint32_t reserved1;                     /* 0x8c */
```

The complete v2 carrier is 144 bytes with alignment 8. Prepare requires offset
one for `BlockBody`, offset zero for `NamedAdapter`, and zero `reserved1`.
Abort requires both appended words to be zero. The Runtime copies only this
checked integer into its private ResumableEntry; it does not retain the carrier
or reinterpret word zero. Frame materialization requires an exact receiver
payload of `offset + captureCount` words and copies capture `i` from
`offset + i`. This is a private append-only symbol and does not widen frozen
IdApi or reinterpret the existing runtime-ABI hash.

Codegen's generation roots and live Blocks retain their vtables; the vtable
owner retains the CodeObject. Withdrawing a generation therefore releases an
unused vtable/carrier/CodeObject graph naturally, while any surviving Block
keeps the exact graph live. K3 may replace the generation cache with the full
materialization repository without changing this Block contract.

The exported `Block` family prototype is a non-callable bootstrap prototype,
not a portable closure, and may retain its zero-word payload. Sending closure
inspection or `value:`-family messages to it answers the ordinary protocol
error. No decoder, invocation path, or image writer guesses whether word zero
exists from payload size; the admitted private vtable schema distinguishes a
portable closure from the family prototype and every legacy layout.

### 6.3 Other callable objects

**Accepted 2026-08-25.** `Reflect defineMethod:on:as:` remains open to any
object implementing the appropriate late-bound `value:` family protocol. A
CodeObject-backed Block is the standard portable closure, not a new nominal
restriction on method implementations. Imageability is validated separately.

`Reflect defineMethod:on:as:` continues to accept an implementation object and
invoke its late-bound `value:` family protocol. A Block is portable because it
has a CodeObject. Another callable object is portable only if its own type
supplies an admitted image/reconstruction contract. A foreign callback, raw
native Method wrapper, or opaque external resource does not become portable by
being mislabeled as a CodeObject.

The runtime may install such a process-local implementation, but image save
must reject a current binding that has no honest durable representation unless
the user removes or replaces it first.

## 7. PortableProgram semantic model

**Accepted 2026-08-25.** Authoring source and source maps are optional
provenance outside PortableProgram meaning. A source-less CodeObject remains
executable, structurally inspectable through CodeNodes, and rebuildable by a
compatible compiler. It does not promise recovery of comments, whitespace,
original spelling, or exact source spans.

### 7.1 Layer boundary

**Accepted 2026-08-24.** PortableProgram is canonical normalized
language-semantic structure above all compiler lowering and below optional
authoring source. It is not physical stackless IR, generated C, or a required
source string.

**Accepted 2026-08-26.** PortableProgram is the sole semantic input to both
Block-body backends. Source-authored Blocks canonicalize to PortableProgram
before synchronous or resumable lowering; source-less CodeObjects enter the
same lowering seam from their admitted canonical bytes. Backend lowering reads
explicit binding identities, bit-preserving float values, and nested capture
operands directly. It must not reconstruct source, rediscover lexical identity
from names, or maintain a second source-shaped Block compiler with independent
semantics. A compiler may retain a transient validated view or physical IR
below this seam, but neither is durable meaning or a competing authority.

**Accepted 2026-08-26.** Frozen stackless selection continues to name a source
Block occurrence by its lexical block path. Source canonicalization derives a
checked compiler-local bijection from those paths to the corresponding
BlockLiteral Node occurrences in each enclosing PortableProgram. Portable
backend emission selects a nested disposition by canonical Node occurrence,
never by nested-program byte equality: distinct occurrences may carry
byte-identical meaning while retaining different physical dispositions. The
bridge is transient compiler state and adds no field to PortableProgram,
ArtifactSelection, or an image.

Compilation policy is an explicit per-Block code-generation map. Lexical path
identifies the exact source Block and the compiler-local bridge translates that
identity to its canonical BlockLiteral occurrence; neither source position nor
semantic-byte equality chooses policy. The map's value is the already-admitted
synchronous or resumable disposition and, where required, its manifest entry.
The compiler validates and obeys this map deterministically. How an
orchestrator, user tool, profiler, or later optimizer constructs the map remains
an open policy seam: whole-revision selection and sparse explicit selection are
valid producers, while selector spelling, source annotations, and hidden
compiler heuristics are not policy authority.

Optional source provenance may use the same checked occurrence mapping only to
attach diagnostic source text or spans. It cannot replace PortableProgram
operands, change binding identity, or alter the selected execution disposition.

PortableProgram is normalized language structure:

```text
authoring source (optional)
        |
        v
parser CST (transient)
        |
        v
PortableProgram / CodeNodes       durable semantic code
        |
        +-------------------------------+
        v                               v
synchronous lowering              resumable lowering
        |                               |
        v                               v
native Method                 CFG -> layout -> descriptor -> StepMethod
```

This layer is above either backend and below source spelling. It preserves
Birnam evaluation and lookup semantics without preserving comments,
whitespace, literal spelling, parser wrapper nodes, PCs, slots, or C names.

### 7.2 Canonical node set

PortableProgram v1 admits the language subset already represented by the Rust
source-shaped compiler, after lexical resolution:

- Block signature and ordered statements;
- immutable literals: signed integer value, bit-preserving floating value,
  decoded String bytes, Symbol bytes, `nil`, `true`, and `false`;
- lexical reads by explicit binding identity;
- global reads by name;
- `let` binding with a fresh lexical binding identity;
- nested Block literal with its embedded PortableProgram and ordered capture
  operands; and
- ordinary Send with receiver, exact selector bytes, and left-to-right
  argument list.

Lexical reads and global reads are distinct nodes. The decoder never repeats
the old source heuristic “a Global-shaped name is local if an environment
happens to contain it.” Parameters, lets, and captures have explicit dense
binding identities within one CodeObject. Names are retained for reflection and
source reconstruction but do not substitute for those identities.

Nested Blocks carry their own CodeObject program and a list of outer lexical
operands. Evaluating the literal reads those operands once, in order, and
constructs the nested Block. The nested CodeObject contains no captured Oop.

### 7.3 Canonicalization

**Accepted 2026-08-25.** 60K owns PortableProgram semantics;
[Design 60L](../../design/60l-portable-program-and-image-v7-byte-grammar.md) will be the sole
authority for exact PortableProgram v1 and image-v7 bytes,
hash domains, bounds, canonical ordering, independent reader, golden fixtures,
and malformed-input matrix. Implementation does not begin before that physical
format gate closes.

Canonical bytes are determined from semantic values:

- integers use one signed 64-bit encoding after range admission;
- floats preserve the exact admitted IEEE-754 bits, with one documented NaN
  policy;
- Strings and Symbols carry length-delimited bytes, not C strings;
- selectors carry exact length-delimited bytes and validated arity;
- counts and binding references are bounded fixed-width unsigned integers;
- child nodes occur in source evaluation order;
- unknown required tags or nonzero required flags reject;
- duplicate binding identities, forward lexical references, invalid capture
  operands, trailing bytes, and depth/count/byte-limit violations reject; and
- source spans, paths, comments, and native disposition are excluded.

Decoded CodeNode graphs must be finite and acyclic. Constructors reject a
cycle, shared mutable child, or node owned by an incompatible semantic version.
The semantic version fixes the meaning and evaluation order of every node tag;
supporting a later version requires an explicit compiler/loader admission path,
not reinterpretation by the current decoder.

A subsequent physical-format section or dedicated byte-grammar record must fix
the exact magic, offsets, tags, widths, alignment, bounds, golden fixtures,
malformed matrix, and independent reader before implementation begins. The
semantic choices above are not delegated to that codec.

### 7.4 Source-sensitive compilation

Compile-time source operations must be resolved before CodeObject construction.
For example, an admitted embedded file becomes an ordinary String-literal node
containing the embedded bytes. PortableProgram never records “run
`System embedFile:` again later.”

Top-level module declarations, method-publication forms, artifact selection,
and other compilation-unit operations remain outside an individual Block's
program. An ordinary dynamic send with the same selector inside a Block remains
an ordinary Send node.

## 8. Ordinary object surface

**Accepted 2026-08-25; exact protocol sharpened by accepted Design 60M.**
CodeNodes are publicly inspectable, transformable, and constructible
ordinary immutable objects in v1. The compiler is one producer, not the
privileged owner of code construction. Creating a successor CodeObject
validates scope, captures, arity, cycles, resource bounds, and canonical
encoding before returning the value.

CodeObject and decoded program values are ordinary Birnam objects. The original
minimum sketch was:

```birnam
[aBlock codeObject]
[aBlock captureCount]
[aBlock captureAt: index]

[aCodeObject arity]
[aCodeObject captureSchema]
[aCodeObject program]
[aCodeObject digest]
[aCodeObject sourceProvenance]
[aCodeObject withProgram: aCodeNode]
[aCodeObject withoutSourceProvenance]

[CodeObject fromProgram: aCodeNode arity: n captures: schema]
```

Design 60M replaces this sketch with an exact CodeProgram, CodeStatement,
CodeLexicalBinding, and CodeNode protocol. In particular, a whole
PortableProgram is not one CodeNode, and CodeObject construction does not
accept redundant arity or capture-schema arguments. Constructors validate and
canonicalize; they never accept a trusted “already valid” flag from source
metadata.

`program` answers a decoded immutable CodeProgram graph, not merely an opaque
byte String. An implementation may lazily decode and weakly cache that view,
but the canonical bytes remain independently decodable and the cache does not
root an otherwise unreachable CodeObject.

## 9. Compilation and materialization

### 9.1 Materialization key

A native materialization is selected by at least:

```text
complete PortableProgram bytes
PortableProgram semantic version
compiler/lowering contract version
execution disposition
target ABI and runtime-shim ABI
applicable optimization/debug policy
```

The CodeObject digest alone is insufficient: admission compares the complete
bytes and every compatibility component. SourceProvenance is not part of this
key unless a debug/source-map artifact explicitly names its own provenance
digest.

### 9.2 Private canonical-byte capability

K3 obtains canonical bytes through the private
`_BirnamPortableProgramViewApiV1` native capability, outside frozen `IdApi`.
Its exact C-layout payload is `{u64 abi_version, u64 struct_size,
copy_program}`. `copy_program(CodeObject, destination, capacity)` returns the
nonzero exact program length for a valid CodeObject, writes the complete bytes
only when `destination` is non-null and `capacity` is sufficient, and otherwise
performs a size query without a partial write. Zero means rejection. The caller
roots the immutable CodeObject across query and copy; the capability allocates
nothing, performs no send, and lends no pointer beyond the call. This seam is
compiler-private and grants no ordinary-send authority.

**Accepted 2026-08-26.** A materialized Block vtable owns one private two-Oop
carrier `[CodeObject, materializationOwner]`. The first edge preserves and
checks portable meaning; the second retains the process-local TCC unit or other
native owner while the vtable is live. `PortableCodeApiV2` constructs this
shape, while V1 supplies `nil` as the materialization owner for existing
generation-owned paths. A CodeObject alone therefore does not pin native code.
After repository eviction and complete Block/Frame/call quiescence, the
vtable, carrier, and native owner may all become unreachable and finalize.

### 9.3 Publication

Materialization is staged before publication:

1. validate the complete CodeObject and resource bounds;
2. lower it without consulting a live method target;
3. create and validate descriptor, native carrier, registration, and vtable;
4. enter the Runtime's rollback-capable staged generation context;
5. publish the Block or CodeBinding through the existing supported MOP; and
6. commit generation ownership only after the dictionary and semantic binding
   agree.

Failure before publication disposes the staged materialization. Failure during
publication restores the exact previous binding and leaves no CodeObject,
native unit, Symbol, or vtable rooted merely by the failed attempt.

### 9.4 Invocation

Ordinary lookup still returns a Method-shaped carrier from the live vtable
dictionary. Shared Runtime resolve-and-pin classifies it exactly as Designs 60E
and 60F require. A named CodeBinding adapter obtains its retained
implementation object and performs the ordinary late-bound `value:` family
send with method receiver followed by arguments. It never reads capture words
directly or assumes the implementation is a Block.

For a Block-body entry, Frame construction copies the CodeObject-declared
captures from payload words `1..=C`. The CodeObject remains reachable through
the Block for the complete invocation; the Frame pins the selected physical
materialization independently.

### 9.5 Reuse and rematerialization

**Accepted 2026-08-25.** “Portable” means target-independent executable
meaning, not self-executing bytes. Birnam does not add a PortableProgram
interpreter. A malleable image may rebuild through an admitted compatible
compiler; a stripped deployment must carry a compatible native
materialization. Boot with neither rejects with an explicit compatibility
Error.

Several Blocks with byte-identical CodeObjects may share one immutable block
vtable and registration when every materialization-key component agrees. This
is an optimization, not semantic identity. A cache entry is weak unless a live
Block, binding, registration, Frame, Continuation, or explicit generation lease
owns it.

If a reachable CodeObject has no compatible materialization, the compiler or
image loader may rematerialize it at a named safe point. Ordinary send does not
perform an unbounded compilation. A Block must have an admitted carrier before
it is published into callable live state; after publication, either its live
vtable keeps that carrier usable or an explicit re-homing operation moves it to
another already-admitted carrier before retirement.

## 10. Reflect and CodeBinding

**Accepted 2026-08-25.** CodeBinding is the portable reconstruction authority
for a current method binding. It names the target object, selector,
implementation object, and optional provenance. The live vtable dictionary
remains dispatch authority, Store histories the semantic binding under its
configured retention policy, and image-v7 persists the current binding
directly. An unimageable target or implementation makes save reject honestly.

The current ad hoc reflective path creates one unique `__rust_mblk_N` Global
and one TCC unit rooted for process life. 60K replaces that ownership model; it
does not replace the public send.

For each current supported binding, Codegen/Runtime retains a CodeBinding whose
durable semantic fields identify:

- the target object or stable target reference;
- selector;
- implementation object;
- optional definition provenance.

Live carrier metadata additionally names the current publication generation
and its physical ownership. That generation is process-local and is not part of
durable CodeBinding meaning or image-v7 bytes. Image recovery publishes the
durable association into a fresh successor generation; retained predecessor
history belongs only to Store's configured retention policy.

The native Method in the vtable is a physical projection of that binding. Store
records the semantic MOP operation and current implementation object within the
configured history policy; it does not drive dispatch. The process-local
materialization repository may index the binding but cannot be its only durable
owner.

Replacement is ordered:

```text
stage new implementation/materialization
  -> withdraw old semantic mirror
  -> replace live dictionary carrier
  -> publish new CodeBinding mirror
  -> retire old binding/materialization after quiescence
```

Removal withdraws the dictionary carrier and current CodeBinding. It does not
destroy the implementation object if another Block, fact, export, binding, or
Frame still reaches it. A self-replacing method keeps its old materialization
through the active invocation and dispatch-read extent; physical unload occurs
only at a later safe point.

No per-definition unique Global or permanently rooted TCC-unit vector is part
of this design.

## 11. Lifetime and reclamation

### 11.1 Semantic reachability

Ordinary tracing owns semantic lifetime:

- a Block strongly reaches its CodeObject and captures;
- a current or retained CodeBinding reaches its implementation object;
- a CodeObject strongly reaches its canonical program and optional provenance;
- decoded-view and digest indexes are weak; and
- dropping source provenance does not drop executable meaning.

### 11.2 Physical lifetime

**Accepted 2026-08-25.** A reachable Block remains an owner of the native
materialization named by its admitted private vtable, even when it has no active
call. Method removal releases only the CodeBinding's ownership. The first
implementation does not need to re-home live Blocks merely to unload an older
generation; retained code remains correctly attributed to the live Block.

A materialization remains live while any of these owns it:

- a live Block vtable carrier;
- a current method dictionary binding;
- a protected dispatch reader that may invoke its carrier;
- an active Frame;
- a captured Continuation;
- an explicit rollback or generation lease; or
- a staged publication/compensation operation.

Replacement or removal first prevents new acquisitions from the affected
binding. Retirement then waits for every owner above to quiesce. A live Block's
header vtable is itself an owner: code cannot unload merely because no call is
currently active. If the runtime wants to unload that generation while the
Block survives, it must first materialize a compatible successor and re-home
the Block through a reviewed safe-point operation.

### 11.3 Bounded repositories

Slot and generation identifiers may be reused only with generation-qualified
capabilities, so stale native carriers cannot invoke a new binding that happens
to occupy an old slot. Repository vectors may retain capacity proportional to
peak simultaneous live/quiescing materializations, but not one permanent entry
per historical definition. Diagnostics separate live, quiescing, reusable,
and physically unloaded slots and bytes.

## 12. Image-v7

**Accepted 2026-08-25.** Portable CodeObjects require a clean image-v7 format.
V5 and v6 remain readable through their exact legacy contracts. New saves use
v7's CodeObject-aware Block locator, `captureCount + 1` payload rule, portable
inventory, and current CodeBinding recovery rather than extending or
reinterpreting the frozen v5/v6 heap grammar.

### 12.1 Why v7 is required

Image-v6 deliberately retained image-v5's Heap and Code grammar. A generated
Block header currently identifies its private vtable by one
`ResumableManifest` entry index. That reference cannot recover executable
meaning without the exact captured artifact and does not name the CodeObject in
the Block payload.

60K therefore requires a clean image-v7 cut. Existing tag values and v5/v6
readers remain frozen. V7 adds a CodeObject-aware block-vtable locator and a
complete portable-code inventory bound into image authority. The exact bytes
belong to the same follow-on physical-format gate as §7.3.

### 12.2 Heap relationship

A portable Block is serialized as an ordinary heap object with:

- a vtable locator referencing the heap identity of payload word zero's
  CodeObject;
- word zero encoded as the same CodeObject heap reference; and
- capture words encoded as ordinary Oop fields.

Preflight requires the locator and word-zero reference to agree, requires that
the referenced object is a valid CodeObject, and verifies
`word_count == capture_count + 1` before mapping or mutation.

Forward references and cycles remain legal. Loading is staged:

1. preflight all records, CodeObjects, references, bounds, and authority;
2. allocate exported-vtable objects and CodeObjects;
3. admit or map compatible materializations for the complete required
   CodeObject inventory;
4. allocate Block shells with the resolved private vtables;
5. allocate remaining heap shells;
6. patch every payload edge, including cyclic captures; and
7. reconstruct current CodeBindings before user entry.

No user code runs between these steps.

### 12.3 Native code record

An image may still carry one authority-bound native Code unit for fast mapped
boot. Its manifest indexes materializations by complete CodeObject authority,
not by source position alone. Mapped boot proves byte agreement among image
inventory, artifact inventory, registrations, Block vtables, and CodeBindings.

Forced rebuild compiles from PortablePrograms and selection bytes. It does not
require SourceProvenance. A stripped image without a compatible mapped unit and
without an admitted compiler cannot rebuild; it rejects honestly instead of
claiming cross-target portability it cannot exercise.

### 12.4 CodeBinding recovery

V7 persists each current portable method binding as a binding record or
equivalent canonical heap relationship naming target, selector, and
implementation object. A source operation may accompany it as provenance but
is not the recovery authority. Recovery materializes implementation carriers,
publishes dictionary entries through the same tracked MOP, and verifies the
result before entry.

Targets and implementation objects must themselves be imageable. An anonymous
dynamic target whose vtable has no admitted durable representation makes save
fail; CodeObject portability does not conceal an unportable target identity.

## 13. Compatibility and cutover

Images v5 and v6 remain readable through their existing contracts.

- Mapped legacy recovery may execute its captured manifest-indexed unit using
  the legacy capture-only Block layout.
- Source rebuild may compile legacy source into new CodeObjects and the new
  payload layout.
- A legacy Block and a 60K Block are distinguished by admitted image/runtime
  format and registered vtable schema, never by guessing from payload size.
- Saving a newly loaded legacy image as v7 requires every surviving portable
  Block and current CodeBinding to have been reconstructed as CodeObject-backed
  state.
- A surviving legacy source-less value that cannot be reconstructed rejects the
  v7 save with `portableCodeUnavailable`; it is not serialized with a false or
  empty CodeObject.

There is no mixed-layout vtable. A private block vtable admits exactly one
payload schema for its complete lifetime.

## 14. Security, validation, and resource bounds

PortableProgram is untrusted durable input. Structural decoding completes
before native code mapping or heap mutation and enforces explicit limits on:

- complete bytes;
- node count and nesting depth;
- parameters, bindings, captures, statements, sends, and arguments;
- literal, name, selector, and provenance bytes;
- nested CodeObjects and total transitive bytes;
- materialization count and native bytes; and
- image CodeBinding count.

Semantic validation proves binding scope, capture agreement, selector arity,
node-tag legality, evaluation order, literal validity, and nested-program
closure. Native materialization then passes the existing artifact,
FrameDescriptor, registration, Runtime-ABI, and target-ABI checks.

Canonical bytes and SHA-256 detect drift and corruption; they do not make
native code from an untrusted producer safe. Mapping an artifact remains a
separately named trusted-code operation.

## 15. Required diagnostics

At minimum, the reflected diagnostic surface reports:

- live CodeObject count and canonical bytes;
- CodeObjects with and without SourceProvenance;
- live Blocks and capture words;
- current and retained CodeBinding counts;
- active, quiescing, cached, and unloaded materialization counts and bytes;
- weak-cache hits, misses, and rematerializations;
- the semantic owner preventing retirement;
- legacy Blocks not yet v7-upgradable; and
- the exact resource bound or compatibility component that rejected an
  operation.

## 16. Acceptance matrix

The implementation is incomplete until executable tests prove all of these.

1. A source-authored Block exposes an ordinary CodeObject whose decoded nodes
   reproduce its literals, lexical/global distinction, sends, nested Blocks,
   arity, and capture schema.
2. A CodeObject constructed without SourceProvenance executes identically to
   the sourced value through synchronous and resumable materializations.
3. Two Blocks share one CodeObject but retain different capture values and
   answers.
4. A nested source-less Block survives save, process exit, v7 load, forced
   rebuild, and resave.
5. Mapped and rebuilt v7 boot agree on CodeObject inventory, CodeBindings,
   block-vtable identities, behavior, and generation authority.
6. Replacing or removing a reflected method during its own invocation keeps
   the old invocation valid, routes later sends to current dispatch, and unloads
   old code only after the read extent and Frame pins quiesce.
7. A Block retained outside a removed method remains callable; its CodeObject,
   captures, vtable, and required materialization remain live.
8. Dropping the last Block/binding/history owner allows its CodeObject,
   captures, Symbols, vtable, registration, and native mapping to disappear.
9. Three fixed-live-set batches of at least 5,000 reflective
   define/replace/remove operations plateau after warm-up, compaction,
   collection, and code-repository reap.
10. Deliberately retaining old Blocks or unbounded history grows and attributes
    that growth to the retained semantic owner.
11. Invalid programs, hash collisions injected in tests, stale repository
    capabilities, wrong capture counts, unimageable targets, and incompatible
    native units reject without partial publication.
12. Image-v5/v6 compatibility boots through the legacy contract, upgrades when
    complete portable meaning exists, and rejects an impossible upgrade
    explicitly.
13. The frozen native IdApi client/module fixture and ordinary/Machine
    shared-resolution tests remain byte- and behavior-compatible. The archived
    v4 image is provenance evidence, not a current admission gate.
14. Same-run send benchmarks prove no CodeObject decode, Store lookup, digest,
    allocation, or compilation was added to the steady-state ordinary-send hot
    path.

## 17. Delivery sequence

The sequence is intentionally vertical and keeps legacy recovery available
until v7 proves itself.

1. **K0 — Design 60L physical grammar — complete 2026-08-25.** PortableProgram v1 and
   image-v7 exact bytes, bounds, independent reader, fixtures, and malformed
   matrix. A representative executable codec/fixture/independent-reader slice
   landed 2026-08-25. K0 structural preflight freezes the authority vectors and
   boot-admission contract but neither maps nor rebuilds native code. Stored
   goldens, generated boundaries, malformed cases, and bounded properties close
   the physical gate.
2. **K1 — Reified values — complete 2026-08-25.** CodeObject, CodeProgram,
   CodeLexicalBinding, CodeStatement, and CodeNode are owned by the dedicated
   Portable Code rung. Ordinary protocols, canonical codec reuse, validation,
   kinded diagnostics, scoped roots, reload-safe family reuse, and collectable
   decoded views are proven without changing execution.
3. **K2 — Block cutover — complete 2026-08-26.** Every synchronous, selected,
   candidate, and image-generated Block constructor now uses
   `[CodeObject, captures...]`. The append-only registration-v2 envelope carries
   BlockBody offset one without changing frozen v1 or IdApi; ordinary sends
   expose CodeObject and captures; exact payload materialization, shared meaning,
   distinct captures, invocation, and GC ownership are proven. Until K5, the v6
   writer rejects an ordinary portable Block as `portableCodeUnavailable` while
   projected globals continue to recover through authority-bound image code.
4. **K3 — Materialization repository — complete 2026-08-26.** Materialize from
   CodeObjects, use generation-qualified Runtime ownership, integrate
   resolve-and-pin, and retire after complete ownership quiescence. Executable
   eviction, live-carrier, nested-carrier, shared-identity, and separate-turn
   retirement tests cover synchronous and resumable materializations.
5. **K4 — Reflect/CodeBinding — complete 2026-08-26.** Unique
   `__rust_mblk_N` Globals and process-lifetime TCC roots are replaced by current
   semantic CodeBindings and quiescence-aware portable NamedAdapters. Ordinary
   sends, arbitrary callable implementation objects, semantic reflection,
   replacement, removal, self-mutation, Store arbitration, and bounded
   retirement are executable gates.
6. **K5 — Image-v7 — complete 2026-08-26.** New saves persist CodeObjects,
   portable Blocks, the three canonical materialization inventories, current
   CodeBindings, and a real native Code unit. Ordinary boot maps and verifies
   that unit; `--rebuild` and deliberately stripped images compile the same
   source-free authority in memory. Nested closures, current bindings,
   mapped/rebuilt behavior, process exit, resave, independent fixture reading,
   and final Code-record framing are executable gates.
7. **K6 — Compatibility and bounded churn — complete 2026-08-27.** V5/v6 keep
   their exact legacy readers and upgrade through source reconstruction where
   honest; v4 remains retired. Design 47 composed-generation reconciliation is
   preserved without serializing native publication identity. The full 60J
   method, Symbol, bounded-history, retained-growth control, mapped successor,
   and save/load/continued-churn/resave matrix passes and closes J5.

K1 may land behind diagnostics, but no new-layout Block may enter a production
image until K0 is frozen and K2-K5 agree on the same CodeObject authority.

## 18. Explicit non-goals

This record does not add:

- public continuation syntax or durable Continuations;
- static dispatch or selector sealing;
- a Store-driven interpreter;
- mutable CodeObjects underneath active Frames;
- mandatory content-addressed Oop identity;
- universal serialization of foreign resources or raw native callbacks;
- a new source annotation choosing synchronous versus resumable compilation;
- compiler stripping from every portable image;
- cross-isolate shared CodeObject Oops; or
- debugger mutation of physical Frame slots.

Those features may build on CodeObjects later, but none may weaken the authority,
ordinary-send, image-honesty, or lifetime rules above.
