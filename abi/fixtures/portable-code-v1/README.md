# Portable code v1 and image-v8 canonical fixtures

These files are frozen conformance evidence for
[Design 60L](../../../docs/design/60l-portable-program-and-image-v7-byte-grammar.md).
They are not compiler inputs, bootstrap inputs, or authority to infer the
grammar from an implementation.

Run:

```sh
python3 verify.py
shasum -a 256 -c SHA256SUMS
```

`verify.py` is an independent reader. It imports neither the Rust codecs nor
`build-fixtures.py`, emits no expected bytes, validates canonical structure and
semantic relationships, and checks targeted mutations plus truncation at every
byte boundary of the representative program and empty image.

`build-fixtures.py OUTPUT_DIRECTORY` is a separate manual encoder. It refuses
to regenerate this directory in place. Generate into a temporary directory,
then deliberately compare its compact bytes and `SHA256SUMS` with the committed
files. It also emits maximum-size generated vectors whose length and digest must
match `BOUNDARY_VECTORS`; those large repetitive binaries are deliberately not
committed. The Rust production encoders and independent reader reproduce the
same vectors without importing this builder.

## Inventory

| Family | Fixtures | Covered contract |
|---|---|---|
| PortableProgram | `program-*.bin` | empty and nonempty bodies/Blocks, opaque/repeated captures and binding names, non-UTF-8 names/selectors, exact signalling NaN bits, postorder send operands, Let timing, all auxiliary values, zero padding, future-Let scope, and nested repeated captures |
| PortableProgramInventory | `inventory-*.bin` | empty and one-row values, digest/byte sorting, duplicate elision, and exact transitive nested closure |
| CodeBindingInventory | `bindings-*.bin` | canonical Field encodings and sorting, selector arity, Block and non-Block implementations, semantic row digests, and provenance-only byte change with stable semantics |
| CodeMaterializationSelection | `selection-*.bin` | empty metadata, exact NamedAdapter/BlockBody coverage with mixed dispositions, and independence from physical FrameDescriptor hashes |
| image-v8 | `image-*.bin` | empty and mixed stripped images, identity-bearing boxed values, Block-valued bindings, shared CodeObjects, repeated Blocks, cyclic captures, CodeObject/portable Block heap contracts, retained Facts with tag 12, authority derivation, and final Code framing/checksum |

Exact lengths and SHA-256 digests are frozen in [`SHA256SUMS`](SHA256SUMS).
Replacing any binary is a physical-format change and requires deliberate
review of the new bytes, digest, production reproduction, and independent
reader result.

[`BOUNDARY_VECTORS`](BOUNDARY_VECTORS) freezes deterministic recipes, lengths,
and SHA-256 values for maximum-size admissible values. Synthetic equal-digest
tests separately prove the inventory's bytewise collision tie-break; they are
algorithm tests, not claims that a SHA-256 collision fixture exists.

## Evidence boundary

`image-mixed-code.bin` contains a small synthetic nonempty native-unit byte
string. It proves image-v8's final Code record, wyhash checksum, and portable
authority agreement. It is not evidence that a native artifact exports the
new selection/authority surface, maps successfully, or registers executable
entries. Likewise, stripped-image structural preflight proves format
completeness; the K5 boot-admission decision for mapped exports or a compatible
compiler remains outside this format-only corpus.

Actual compiler admission, mapped-loader execution, forced rebuild, runtime
CodeObject materialization, publication, and human acceptance remain later
implementation gates. The format codecs in this slice allocate no Birnam
objects and publish no CodeBinding.
