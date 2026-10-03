# Image format owner over Id

`ImageFormat` is a separately loaded native module implemented in Rust. It owns
complete image-v8 container composition, frozen bytes, structural admission,
record/reference/quantity bounds, checksum/authority validation and inert record
extraction. It has no live heap, Store state, source registry, code mapping or
publication transaction. Existing codec algorithms were moved unchanged to the
local [codec library](../birnam-image-codec/README.md).

The constructor takes completed Namespace and publishes `ImageFormat` and
`ImageFormatVt`. Its creating-thread receiver/method contexts have process lifetime.
Only ordinary Objects and canonical Bytes cross Id:

- `compose:` accepts a Vector of architecture Integer, ABI Integer (64-bit word
  bits), programs Bytes, bindings Bytes, selection Bytes, heap Vector of Bytes,
  facts Vector of Bytes, optional code Bytes, optional sources Bytes and optional
  behaviors Bytes. Optional fields are nil. It returns encoded image Bytes.
- `inspect:architecture:abi:` accepts image Bytes and the expected architecture/ABI
  Integers. It returns a Vector of programs, bindings, selection, heaps, facts,
  optional code, optional sources and optional behaviors, using the same types.

Byte borrows are copied before sends. Format refusal retains Image's distinguished
native refusal detail `0x494d4147`; the public Image API continues to answer nil
with its rejection report. Other dependency failures propagate unchanged.

Image projects live objects into canonical record payloads and interprets the
validated payloads for restoration. Complete format composition/admission always
uses the current ImageFormat receiver. Image keeps atomic file publication, heap
restoration, Store/source exchange, generation recovery and exact compensation.
The small frozen field/value codecs remain available for object projection and
offline Rust tools; no full-format validation or encoding bypass remains in the
production Image restoration owner.

Validation uses frozen codec fixtures and malformed/truncated/checksum cases,
byte-identical native roundtrip, collection and failure replacement at the Id
seam, installed mapped/rebuilt/resaved recovery and compensation, and baked entry.
