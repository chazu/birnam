# String collection adapter v1

Design [81AH](../docs/design/81ah-production-string-collection.md) moves String
split: and its byte-copy helper into `birnam-string-collection`. String's early
bootstrap module stays independent of Vector. Load the adapter after String and
Vector and before Collection, attaching String v1 and Vector v2 dependencies
before initialization. Collection still sets String's parent and installs split:
at its original point, before Vector methods.

Initialization only enables `birnam_string_collection_provider_v1`. It creates
no objects, layouts or root providers. Attach the resulting immutable descriptor
through `birnam_string_collection_dependency_v1`. The arm64 descriptor is 24
bytes: version 1 at offset 0, byte size at 8, and one function pointer at 16.
The [independent C mirror](fixtures/string-collection-v1/string_collection_v1.h)
checks this layout. Admission rejects null, wrong version/extent, missing install
and retargeting; the same descriptor pointer may be attached repeatedly.

`install(IdApi *registration, const IdApi *execution)` registers the native
split: Method using the current registration table. Execution must be Collection's
original retained table, even if parent-setting callbacks reenter initialization
with a different table before split: is first registered. The adapter retains the
first supplied execution pointer and reads its live callbacks on every call.
Both tables and native code must remain process-owned and pinned; installation
and calls run on the runtime owner thread. Reinstallation uses the supplied
registration table without replacing the retained execution table.

The method preserves exact String checks and callback order. An invalid separator
produces a Vector containing the original receiver, including null or an inexact
receiver. Valid separators copy receiver bytes before separator bytes. Empty
extents are accepted before null-byte rejection. Matching is nonoverlapping and
byte-oriented, retaining leading, trailing and adjacent empty parts. An empty
separator produces a fresh String, whereas an invalid separator preserves receiver
identity. Live String make results, including null and aliases, are appended
unchanged. Construction uses native Vector empty/append operations and Vector's
retained allocator, without public Vector dispatch. The adapter caches no Oops.

There is no Collection forwarding Method, early String-to-Vector dependency,
new IdApi field, layout or image format. Run `mise run string-collection` for the
native oracle and `scripts/string-collection-image-smoke.sh PREFIX` for dynamic
startup, a source-free compiled module, a v8 native Code image and a baked app.
