# Representation query investigation

Before this slice, addition used 175 sends and seventeen single-object Id
representation queries. The counts are about metadata authority and validation,
not seventeen independent type-policy decisions. Four fresh allocations each
receive ObjectModel admission and a numeric-owner pre-write validation; input
reads and the final Word getter account for the rest.

## Implemented atomic batches

Id exposes `tryRepresentationsOf:behavior:bytes:`. It receives a borrowed native
ABI array of two to 32 opaque Object identities, an expected behavior Object and
a native byte count. Word 1 certifies every live allocation's exact behavior,
extent and nonzero representable identity. Word 0 rejects a mismatch; Unit declines
when the query, original single query or any omitted getter has a changed meaning,
or an identity cannot be represented. It is not a portable byte format.

The implementation reads only Id-owned heap/dictionary metadata. It never reads
Word/Integer payloads. The existing single and new batch queries share the same
canonical getter/lookup checks; the batch also checks its own exact method/context.
The default-allocation admission includes the new dependency. No verdict is reused
across calls. A forwarding query wrapper cannot get a positive stock certificate
while its own binding is noncanonical.

Word reads the certified bits from its own storage immediately, before another
message/callback. A declined or missing query executes the original ordered
single reads and their getter fallbacks; other query errors propagate. No per-call
heap buffer is allocated by the Word batch path. Inputs remain covered by their
caller/owner for the complete synchronous request.

The overflow expression's four input checks become one metadata message. Valid
binary Word operations combine their two operand checks into one. Malformed
arity/right carriers still execute the old left-read-before-rejection sequence;
a native regression verifies a left query failure wins in all these cases.
Constructor validation, permission, post-callback epoch fences and result
construction remain. Payload reads are neither moved to Id nor performed by a
foreign owner.

## What remains

Addition now uses eleven single metadata queries and two batch queries, thirteen
messages total rather than seventeen. Its total falls from 175 to 171 sends.
Three temporary Words and one Integer still allocate; no CPU speedup is inferred
from the census. Relative to the 177-send starting point for the four investigations,
cleanup saves two sends and representation batches save four, a 3.4% net reduction.

The eleven single queries are eight fresh-allocation checks (four in ObjectModel,
four in the numeric constructor owners), two Integer input reads and the final
Word getter. The two batches cover the ordinary addition operands and four inputs
to overflow checking. Separate constructor checks look repetitive, but a replaced
allocator can return or mutate an incompatible Object. Removing the receiving
owner's check without a current allocation/shape contract risks an unsafe payload
write. No such check is removed by this slice.

A certified allocation handoff could carry both the Object and metadata, but must
cover current allocator replacement, callback mutation, exact identity and the
lifetime of the returned reference. Adding a certificate plus another guard is
not automatically cheaper than the existing check. This remains a possible owner
protocol/compiler-region investigation, not a completed optimization or proof
that the remaining thirteen messages are minimal.

## Evidence

Native tests cover matching/mismatching behavior/size, repeated aliases, stale
reassociation, unknown identities, malformed requests, count limits and replacement
of the single query/getters/batch itself. Word tests retain exact left/right and
malformed-request error order. Existing tests cover collecting/replaced permission,
32-input descriptors, signed/mixed Float/Error behavior and the cleanup regression.

Lean requires every batch shape and current service/identity conditions for a
positive certificate. `RepresentationBatch.tla` checks both inputs and original
fallback order; ignored second input, omitted getter/self witnesses, delayed reads
and reordered fallback variants must fail. Proofs are abstract owner contracts;
there is no claim of verified Rust or arbitrary dishonest providers. The new
primitive is an ordinary Id message with no IdApi layout or image format change.
All cold-bootstrap Rust owners remain. Loading machine-code images still does not
implicitly recompile them.

Final acceptance passed 22 Rust/C harness checks, 26 native checks across
21 binaries, fourteen task assertions at quanta 128 and 1, all 146 Lean theorems
and all 94 TLA+ configurations including required unsafe counterexamples. A fresh
release prefix passed its saved-image startup gate, then reproduced 171 sends
and the same four allocated Objects with stable inputs. See the [receipt](../measurements/representation-batch-investigation-20261009.json).
