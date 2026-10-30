# Current bulk byte messages

Bytes represents a variable-length binary value, including zero bytes. Passing
that value is an ordinary message argument. The language requirement is bulk
handling of whole buffers, stated in [INVARIANTS.md](../../INVARIANTS.md).
This file describes the current protocol, not an immutable implementation design.

## Reading a buffer or range

`sizeWord` supplies the byte count. `copyByteValuesFrom:count:to:` asks a provider
to copy a range to a destination. The provider snapshots that range before calling
the destination's `appendByteValues` operation. Out-of-range requests fail.

The destination accepts byte values in 0..255. A whole-buffer read requires the
exact promised count. Invalid values, overflow, incomplete output and ignored
destination errors fail the read. Nested reads have distinct destinations, and a
destination retained after its read ends rejects further delivery. Collection and
callbacks must not invalidate a read already in progress.

Bytes, String, and the bootstrap Name, Symbol and Sequence implementations support
this protocol. Custom providers implement the bulk operation. There is no
automatic per-byte fallback. Replacing `atWord:` changes indexed access; replacing
the bulk operation changes whole-buffer access.

`Bytes matchesByteValues` compares its contents against a group of immediate byte
values and returns immediate 0 or 1. Generated native factories currently use it
to compare checksums without one message per checksum byte.

## Constructing byte values

Native producers construct Bytes with the current `fromByteValues` and
`appendByteValues` messages in bounded groups. String factories receive that
ordinary byte provider. The shared native helper contains no runtime state and
adds no Id operation. A complete native buffer need not become a Sequence with
one Oop per byte. Explicit Oop argument lists and layout-kind lists remain valid
uses of Sequence.

## Current native implementation

The current implementation delivers groups of at most 8192 Oops. Producers copy
their data before callbacks; consumers collect it in temporary native buffers.
The shared native helper tracks active receiving objects and rejects delivery
after an operation completes. Native modules declare receiving methods when
loaded. The executable facade currently declares its callback through TCC.

Those batch sizes, copies, helper structures and callback declarations are
implementation choices. They are not new language concepts or invariants, and
should be reconsidered if a simpler message-based implementation meets the same
behavior. No native consumer is entitled to decode another object's private
payload merely because it wants the bytes.

[Implementation and measurements](../measurements/bulk-byte-transfers-20261029.json)
record the current fix and its limits. That report is evidence, not design authority.
