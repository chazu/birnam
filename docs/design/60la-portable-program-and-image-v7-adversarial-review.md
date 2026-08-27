# 60LA — Adversarial Review: PortableProgram v1 and Image-v7 Bytes

> **Status:** Review complete; the reviewed grammar was accepted and frozen
> 2026-08-25 as
> [60L — PortableProgram v1 and Image-v7 Byte Grammar](60l-portable-program-and-image-v7-byte-grammar.md).
> This remains the adversarial rationale; 60L records format acceptance and
> implementation evidence.
> Mechanical corrections already incorporated into 60L are marked Resolved.
> D1 was accepted 2026-08-25 in favor of source-independent v7 authority.
> D2 was accepted the same day in favor of the minimal two-word CodeObject image
> payload. D3 was also accepted: publication generation is live-only runtime
> metadata. D4 was accepted in favor of fixed postorder node tables. D5 was
> accepted in favor of opaque nonempty NUL-free name and selector bytes. D6 was
> accepted in favor of preserving every binary64 bit pattern. D7 was accepted
> in favor of preserving explicit ordered capture schemas, including unused and
> repeated outer bindings. No semantic decision remains open. A representative
> exact fixture corpus and independent reader landed 2026-08-25. K0 structural
> preflight and K5 boot admission are separate accepted boundaries, and the
> expanded physical matrix subsequently closed K0.

## 1. Review method

The review attacked the draft from six independent-reader perspectives:

1. a hostile byte producer seeking ambiguous parses, excessive allocation, or
   mutation before complete validation;
2. a second compiler trying to emit the same semantic CodeObject without
   sharing the production encoder's data structures;
3. a future compiler choosing a different valid Frame layout;
4. a stripped mapped image with no source and no compiler;
5. a reflective Birnam program exercising non-source-spellable Symbols,
   CodeNodes, target objects, and ordinary sends; and
6. 60J's collector trying to reclaim every semantically unreachable program,
   Symbol, vtable, binding, and native mapping after churn.

It cross-checked 60L against 60I's common framing and frozen v5 Heap/Field
grammar, 60K's accepted CodeObject and Block semantics, 60J's root-aware
compaction invariants, the current Rust source-shaped compiler expression set,
and the current language selector-arity rule.

## 2. Severity summary

| Severity | Found | Resolved in draft | Open decision |
|---|---:|---:|---:|
| Critical | 4 | 4 | 0 |
| High | 8 | 8 | 0 |
| Medium | 8 | 8 | 0 |
| Low | 3 | 3 | 0 |

An Open finding is not necessarily a defect. It means two coherent contracts
exist and repository evidence cannot choose between them without fixing a
durable policy.

## 3. Critical findings

### C1. Provenance contaminated executable authority

**Attack:** The first draft hashed the complete CodeBinding row, including
optional provenance, into its materialization digest and image authority.
Stripping source provenance would therefore change authority and mapped-code
selection despite 60K explicitly making provenance nonsemantic.

**Resolution:** 60L now defines a per-binding semantic digest over target,
selector, implementation, and arity only, plus a canonical aggregate binding-
semantics digest. Provenance remains covered by the CodeBindings record's
corruption hash but is excluded from materialization selection and image-code
authority.

**Status:** Resolved.

### C2. Backend-specific FrameDescriptor hashes contaminated portable authority

**Attack:** The first Materialization row included the exact FrameDescriptor
hash, and image authority included the complete materialization value. A future
compiler choosing a different valid slot coloring would then change portable
authority even though CodeObject meaning and synchronous/resumable selection
were unchanged.

**Resolution:** `CodeMaterializationSelection` now freezes only subject,
disposition, selector, arity, capture count, and semantic digest. Exact
descriptors remain owned by the compatible native artifact manifest and are
cross-checked after compilation or mapping. A different valid descriptor can
realize the same accepted portable authority.

**Status:** Resolved.

### C3. Extra inventory programs became hidden immortal roots

**Attack:** Requiring nested programs to be present did not reject extra
unreachable rows. A writer could accumulate every historical CodeObject in the
inventory and force the materialization repository to retain them, recreating
the unbounded process/image growth that 60J is intended to remove.

**Resolution:** The inventory is now exactly the closure of Heap CodeObject
program references and their transitively embedded BlockLiteral programs.
Missing and extra rows both reject.

**Status:** Resolved.

### C4. Live outer code could construct an unavailable nested Block

**Attack:** The first selection required BlockBody rows only for vtables already
present in Heap. In a stripped image, a live outer Block could later evaluate a
nested BlockLiteral whose program had no compiled realization. With neither
source nor compiler, ordinary execution would fail after boot.

**Resolution:** Every inventory program has exactly one BlockBody selection row,
and the inventory is transitively closed. A stripped mapped unit realizes all
rows. A compiler-bearing boot may realize lazily only at the named safe point
before publishing the new Block.

**Status:** Resolved.

## 4. High findings

### H1. Image-v7 authority has two coherent owners

**Attack:** V5/v6 Authority is the captured DispatchGeneration base digest,
which is source-revision based. A source-less v7 rebuild can retain that digest
as an opaque namespace, but cannot derive it from the portable meaning it is
rebuilding. Conversely, deriving a new authority from portable inventories
changes the meaning of the existing Authority record in v7.

**Recommendation:** Accept 60L's source-independent authority derivation. V7 is
already a clean format cut, and a CodeObject-authoritative image should be able
to prove and reproduce its code authority without unavailable source history.
The unchanged 64-lowercase-hex representation preserves outer framing; only
the version-qualified derivation changes.

**Alternative:** Preserve the opaque DispatchGeneration digest. This minimizes
conceptual change to the outer record but makes source-free rebuild dependent
on an authority token whose derivation is no longer present and makes resave
authority rules harder to explain.

**Decision:** Accepted 2026-08-25. Image-v7 derives source-independent portable
image-code authority exactly as specified by 60L §9.4. It does not retain the
v5/v6 source-derived DispatchGeneration digest as an opaque token.

**Status:** Resolved.

### H2. CodeObject needs an exact v7 heap representation

**Attack:** A generic Heap decoder cannot find PortableProgram bytes, validate
Block locators, or allocate CodeObjects before materialization unless v7 fixes
which CodeObject slot owns the program reference. Merely saying CodeObject is
an ordinary object leaves preflight dependent on an implementation-private
layout provider.

**Recommendation:** Accept the proposed two-word all-Oop v7 payload:
`[PortableProgramRef, provenance]`. Arity, captures, and digest are derived
from the one canonical program; caches stay weak and nonsemantic. This is the
smallest physical representation and does not expose offsets as the public
object protocol.

**Alternative:** Store program, arity, capture schema, digest, and provenance
as five Oops and require redundant agreement. This makes heap inspection more
literal but adds allocation, image bytes, validation branches, and additional
objects whose only meaning is derived.

**Decision:** Accepted 2026-08-25. A v7 CodeObject has the exact two-word
all-Oop payload `[PortableProgramRef, provenance]`. Arity, capture schema, and
digest are derived from the admitted program and exposed through ordinary
sends; they are not redundant heap words.

**Status:** Resolved.

### H3. Byte-unique program storage must not collapse CodeObject identity

**Attack:** Sorting and deduplicating program bytes could accidentally intern
CodeObjects by digest and violate 60K's ordinary Oop identity decision.

**Resolution:** Program rows deduplicate only immutable byte values. Every
CodeObject remains a distinct Heap object with its own HeapRef and may share one
`PortableProgramRef`. Hash collisions compare complete bytes.

**Status:** Resolved.

### H4. A CodeBinding target or implementation may not be a HeapRef

**Attack:** Restricting both fields to HeapRef/ExportRef would silently make
canonical singletons and interned Symbols unimageable as reflective objects,
narrowing the ordinary send surface. Allowing inline Float/String instead would
reconstruct value equality without preserving the particular private-vtable
owner's identity.

**Resolution:** The draft admits Nil, True, False, Integer, Symbol, HeapRef, and
ExportRef, subject to the runtime's durable private-vtable contract, and rejects
inline Float/String. The loader still rejects a target that cannot honestly own
a reconstructed method dictionary.

**Status:** Resolved, subject to fixture proof.

### H5. Heap traversal order leaks into binding indices and authority

**Attack:** Binding target and implementation fields may contain global Heap
IDs. Reordering otherwise equivalent Heap objects changes binding bytes and v7
authority.

**Resolution:** This is acceptable for image-generation identity. 60K rejects
content-derived Oop identity; two resaves with different heap identities are
successor images even if programs are byte-equal. Mapped and rebuilt paths for
one exact image still agree. No claim of graph-isomorphism canonicalization is
made.

**Status:** Resolved.

### H6. The native v7 artifact ABI is not frozen here

**Attack:** Image-v7 maps code only if an artifact can export inventory hashes,
selection rows, descriptors, resolvers, and registrations. 60I's v1 native
envelope is source-manifest based and cannot simply be reinterpreted.

**Resolution:** 60L explicitly limits itself to durable bytes. K3/K5 still need
a separately reviewed private native ABI revision whose complete declaration is
covered by the Runtime ABI hash. No image-v7 implementation may claim mapped
boot merely because the durable grammar is accepted.

**Status:** Resolved as a delivery gate; implemented by 60N and the K3/K5
carrier slices on 2026-08-26.

### H7. Persisting live publication generation would restore history growth

**Attack:** 60K describes a live CodeBinding as knowing its current publication
generation. Serializing predecessor generation identities into every binding
would either retain historical authority or load stale process-local identity.
Omitting the field without explanation appears to contradict 60K.

**Recommendation:** Treat publication generation as live carrier/lifetime
metadata, not durable CodeBinding meaning. V7 persists target, selector,
implementation, and optional provenance; recovery publishes a successor live
generation. Retained history remains governed by Store policy rather than the
current-binding record.

**Alternative:** Persist a provenance-only predecessor generation digest. It
must be excluded from materialization and authority and cannot be restored as
the live generation, so it offers little beyond optional SourceProvenance.

**Decision:** Accepted 2026-08-25. Publication generation is live process-local
carrier/lifetime metadata. V7 does not serialize it or its predecessor digest;
recovery publishes a fresh successor generation. 60K now distinguishes these
live fields from durable CodeBinding meaning.

**Status:** Resolved.

### H8. Code record framing was only inherited by implication

**Attack:** A document claiming sole authority for image-v7 bytes cannot leave
the optional Code payload at “v5-compatible”; an independent reader needs the
exact checksum, digest, and unit boundaries.

**Resolution:** 60L now repeats the exact offsets, minimum length, checksum
function, authority comparison, and final-record rule.

**Status:** Resolved.

## 5. Medium findings

### M1. Postorder fixed tables are not the only canonical AST encoding

**Attack:** A recursive length-delimited node stream is smaller for simple
programs and naturally prevents dangling references. The proposed tables add
indices and directory sections.

**Recommendation:** Keep fixed tables. They allow bounded validation before
object allocation, deterministic inspection without recursive parsing, direct
node identity for public CodeNodes, and explicit mutation testing. Postorder
plus single-parent validation closes the DAG/cycle ambiguity.

**Alternative:** Recursive TLV nodes reduce fixed overhead but require nested
length arithmetic, make independent random inspection harder, and need a
second identity scheme for CodeNode reflection.

**Decision:** Accepted 2026-08-25. PortableProgram uses fixed-width Nodes and
Operands sections in canonical left-to-right postorder. A recursive node stream
is not part of v1.

**Status:** Resolved.

### M2. Opaque reflected names weaken literal source reconstruction

**Attack:** V1 allows non-UTF-8, non-source-spellable binding/global/selector
names. 60K says names support reconstructed source.

**Recommendation:** Keep nonempty NUL-free bytes. Exact source reconstruction
was explicitly not promised; reflection can render escaped diagnostic spelling.
Restricting public CodeNode construction to today's parser grammar would make
syntax, rather than the ordinary Symbol/send surface, a new authority boundary.

**Alternative:** Require parser-spellable identifiers and selectors. This makes
all CodeObjects printable as current Birnam source but cuts off otherwise valid
ordinary runtime Symbols and makes future syntax extensions a format issue.

**Decision:** Accepted 2026-08-25. Binding, global, Symbol, and selector names
are nonempty NUL-free bytes and need not be UTF-8 or source-spellable. Inspection
uses escaped byte rendering where needed; SourceProvenance owns authoring
fidelity.

**Status:** Resolved.

### M3. NaN canonicalization can erase observable bit payloads

**Attack:** Canonicalizing all NaNs makes encoders converge but loses the exact
bits of a Float object admitted through public CodeNode construction. Admitting
all payloads requires every backend to construct them without a source literal.

**Recommendation:** Preserve all binary64 bits, as drafted. A backend can use a
bitcast or byte copy rather than a C floating literal. This keeps PortableProgram
honest about the object value it claims to preserve.

**Alternative:** Admit only one quiet-NaN encoding. Simpler backend support, but
constructing a CodeObject from another NaN must reject rather than normalize.

**Decision:** Accepted 2026-08-25. PortableProgram admits and preserves every
IEEE-754 binary64 bit pattern, including signalling/quiet NaNs and their exact
payloads. Compatible materializers must construct such values without
normalization.

**Status:** Resolved.

### M4. First-use capture order rejects intentionally unused captures

**Attack:** Public construction might intentionally retain an unused object in
a closure as a lifetime anchor. Canonical first-use order removes that slot.

**Revised recommendation after cross-checking 60K:** Preserve the explicitly
ordered capture schema, including unused entries. `captureCount` and
`captureAt:` make the schema observable ordinary-object state, so an unused
capture is not actually semantically invisible. Its memory ownership is an
ordinary, diagnosable Block edge rather than hidden history. Source compilation
still emits only free variables by default, in first-use order.

**Alternative:** Require every Capture binding to be read and derive schema
order from first use. This guarantees minimal source closures, but prevents
ordinary constructors from deliberately creating a Block with additional
inspectable retained state and conflates compiler optimization with the public
CodeObject model.

**Decision:** Accepted 2026-08-25. PortableProgram preserves the explicitly
ordered capture schema. Captures may be unused, and distinct schema entries may
read the same outer binding. Source compilation still emits only actual free
variables in first-use order. Every retained value remains an ordinary,
inspectable, attributable Block edge.

**Status:** Resolved.

### M5. Duplicate reflected lexical names are ambiguous to humans

**Attack:** Two binding IDs may have the same name, so reconstructed source can
appear to capture or read the wrong binding.

**Resolution:** Dense binding IDs remain authority. Inspection must disambiguate
duplicate names with binding IDs, and reconstructed source may alpha-rename.
Rejecting duplicates would change existing shadowing behavior.

**Status:** Resolved.

### M6. Expression depth needed its own ceiling

**Attack:** The draft bounded nested PortablePrograms but not a million-deep
chain of Sends inside one program. A recursive validator or compiler could
overflow its native stack.

**Resolution:** Expression-tree depth is independently limited to 4,096, and
admission must be iterative or prove equivalent safety.

**Status:** Resolved.

### M7. The initial row ceilings contradicted 60I's fixed-record aggregate

**Attack:** Allowing more than two million Materialization rows contradicted the
inherited 1,048,576 fixed-record ceiling before any implementation choice.

**Resolution:** Distinct programs and CodeBindings are each limited to 262,144;
selection rows are exactly their sum and at most 524,288. CodeObject identities
may still reach 1,048,576 because multiple identities can share program bytes.

**Status:** Resolved.

### M8. “Existing image ceiling” was not an exact limit

**Attack:** A reader cannot implement a phrase. The current image owner already
uses a 1 GiB file ceiling.

**Resolution:** 60L names 1 GiB exactly.

**Status:** Resolved.

## 6. Low findings

### L1. Empty-program length is easy to miscompute

**Attack:** Six directory entries plus Metadata means empty sections begin at
`0x120`, not `0x100`.

**Resolution:** 60L states the exact 288-byte empty length and offsets. Golden
bytes must prove it.

**Status:** Resolved.

### L2. Selector syntax and selector arity are different contracts

**Attack:** Reusing the parser grammar would reject runtime-valid Symbols;
accepting arbitrary bytes without an arity rule would make argument counts
ambiguous.

**Resolution:** Names are nonempty NUL-free bytes. Arity uses the existing
colon-count/first-byte rule independently of source spellability.

**Status:** Resolved.

### L3. ProgramRef could become a general hidden String interner

**Attack:** Accepting `PortableProgramRef` in arbitrary Fields would create a
new image-level String identity/interning behavior.

**Resolution:** Tag 11 is legal only in CodeObject word zero. Ordinary Strings
retain the existing tag-6 grammar.

**Status:** Resolved.

## 7. Decision order

The remaining choices should be discussed in dependency order. D1 is retained
here as the accepted prerequisite:

1. **D1 — accepted:** source-independent v7 authority;
2. **D2 — accepted:** minimal two-word CodeObject image payload;
3. **D3 — accepted:** live-only publication generation;
4. **D4 — accepted:** fixed postorder tables;
5. **D5 — accepted:** opaque nonempty NUL-free names and selectors;
6. **D6 — accepted:** preserve every binary64 bit pattern;
7. **D7 — accepted:** constructor-preserved explicit capture schema, including
   unused and repeated outer bindings.

Accepted D1 fixes the authority construction used by all later image rows, and
accepted D2 fixes how Heap reaches that authority. Accepted D3 fixes what
CodeBinding means durably, accepted D4 fixes structural node encoding, and
accepted D5 keeps runtime-valid names inside the portable language surface.
Accepted D6 fixes exact Float value preservation. Accepted D7 finishes
PortableProgram canonicalization without reopening image authority.

## 8. Review conclusion

After the incorporated corrections, no known ambiguity permits two valid
decoders to assign different structure to the same bytes, and no known hidden
inventory root defeats 60J compaction. All identified semantic choices are now
resolved. The stored-golden, generated-boundary, malformed, and property corpus
now prove
the common framing, PortableProgram semantics, inventories, selection coverage,
image-v7 authority, heap contracts, Facts, and Code framing and close 60L's K0
physical gate. They deliberately do not prove native artifact exports, mapped
boot, forced rebuild, runtime CodeObject materialization, or CodeObject
execution. K0 format acceptance therefore remains distinct from K1-K6
runtime-adoption evidence.
