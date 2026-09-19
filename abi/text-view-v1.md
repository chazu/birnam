# Borrowed text views v1

Design [81AK](../docs/design/81ak-production-text-view.md) moves Prelude's exact
String-byte and printable-value helpers into `birnam-text-view`. All existing
Prelude consumers use the dependency: output, file/environment arguments, module
paths and source-load diagnostics. Their OS/IO and error policies stay in Prelude.
Other similarly named helpers have different ownership or recognition semantics
and are outside this slice.

Load the provider immediately before Prelude, attaching prepared String v1 first.
Initialization only enables `birnam_text_view_provider_v1`; it creates no objects,
methods, roots or buffers and retains no supplied IdApi. Attach the immutable,
process-pinned descriptor through `birnam_text_view_dependency_v1`. Its arm64
layout is 32 bytes: version 1 at 0, extent at 8, exact at 16 and printable at 24.
Admission rejects null, wrong version/size, either missing operation and retargeting;
the same descriptor pointer may be attached repeatedly. The
[independent C mirror](fixtures/text-view-v1/text_view_v1.h) checks both layouts.

Both operations receive `const IdApi *` and an Oop and return a 16-byte result:
data pointer at 0 and length at 8. Null data means unavailable; nonnull data with
zero length means present empty bytes. Nonempty views preserve the selected bytes
callback's pointer exactly. Rust adapters encode/decode Option slices without
copying bytes or adding callbacks. Operations use the supplied live table each
time; there is no first-table selection, retained Oop or cached selector.

Exact first rejects null, then calls the supplied vtableOf and compares with the
selected String behavior. It reads the live String bytes operation afterward.
Zero length is accepted before a null-byte rejection. Printable first tries exact;
for any nonnull failure, including an exact String with unavailable bytes, it
interns printString, performs one ordinary send and tries exact on that result.
Preserve fresh/custom lookup and null, non-String and empty result distinctions.
No escaping, Unicode interpretation or public conversion send is added.

A view borrows its provider's storage; this API transfers no ownership and extends
no lifetime guarantee. Keep objects/storage alive under the existing callback
contract and preserve each consumer's existing read points. In particular,
writeFile:contents: reads path bytes before contents bytes but interprets the path
afterward, and source diagnostics retain the path across evaluation callbacks.
The extraction must preserve these aliases, including callback-visible mutation;
it must not turn a view into an owned snapshot or add a collection opportunity.

IdApi v7, String representation, image encoding and output/error behavior are
unchanged. Run `mise run text-view` and `scripts/text-view-image-smoke.sh PREFIX`
for focused native debug checks, startup, a source-free compiled module, saved
v8 native Code image and baked executable.
