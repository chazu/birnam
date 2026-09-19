# 81AH — String's Vector-producing split adapter

> **Status:** Implemented, following complete Map ownership in 81AG.

Move the existing String split: body and its byte-copy helper from Collection
into `birnam-string-collection`. This is the late String-to-Vector adapter:
String's early representation/bootstrap module remains independent of Vector.
The adapter loads after both dependencies and installs split: at the existing
Collection point, after String parenting and before Vector method installation.
It owns the actual implementation, with no forwarding method left in Collection.

The immutable 24-byte descriptor has one installation operation. Supply the
current registration IdApi and Collection's retained original execution IdApi
separately. A parent/registration callback can reenter Collection initialization
with another table before the first split installation; selecting whichever
registration table reaches the adapter first would change method execution.
Retain the supplied original execution table on first installation, use the
current table only for interning/registration, and keep reading live callbacks.
Module initialization only enables the provider. The adapter caches no Oops,
registers no root provider and adds no layouts or objects.

Copy both bodies exactly. Preserve arity/carrier checks, the first separator
snapshot, exact String checks and their order, and the invalid-separator path
that returns a Vector containing the original receiver without validating it.
For valid separators, copy receiver bytes before separator bytes. Preserve
empty extents before null-byte rejection, nonoverlapping byte search, leading,
trailing and adjacent empty parts, the empty-separator fresh String result,
String make failures and native Vector empty/append ordering. Existing live
String callbacks and Vector's first selected allocator remain authoritative.
Do not introduce public String/Vector dispatch or retain a borrowed byte extent
across another callback. Leave shared Collection parenting/coordination separate.

Use a C scalar oracle for binary strings and separators, inherited/invalid
receivers, copied methods, live callbacks, byte snapshot mutation, nested calls,
GC, null/empty byte extents, null String make, and reinitialization including
first-install reentry. Compare native callback traces with installed 81AG.
Verify an external debug prefix, source-free compiled module, saved v8 native
Code image and baked executable before committing. No release validation.

## Validation

- The split and byte-copy bodies match 81AG after normalizing formatting. The
  Method resolves into the new adapter; Collection retains no forwarding body.
  Its registration point remains after String parenting and before Vector methods.
- An independent C scalar oracle checks 4,843 cases in both normal startup and
  first-install reentry. It covers binary inputs/separators, UTF8 byte fragments,
  greedy nonoverlapping matches, leading/adjacent/trailing empty parts, null and
  inherited receivers, invalid separators, arity and malformed argument carriers.
  Development and fresh installed traces match the prior installed 81AG build.
- Live String callbacks preserve receiver-before-separator byte snapshots, empty
  extents before null rejection, mutation during later callbacks, null make
  results and returned aliases. Nested splitting and collection from allocation,
  bytes and make callbacks pass, as does precise collection after returning.
  Copied Methods remain callable after public split replacement; replaced public
  Vector new/add:/at:/size methods do not intercept native construction.
- Reentry from String parenting registers split through a different table before
  the outer registration. Execution still uses Collection's original table,
  including when invoked during the registration callback. Reinstallation and a
  later direct installer call retain that original execution table. The adapter
  registers no roots and does not install methods during module initialization.
- Descriptor admission rejects null, incorrect version/size, a missing operation
  and retargeting; repeat attachment of the same pointer succeeds. Development
  and installed dylibs match the five-symbol inventory, relocatable install name
  and system-only linkage. Collection's sole export change adds the adapter's
  dependency declaration. IdApi v7, object layouts and image encoding are unchanged.
- Existing Map construction (five modes), 4,096-entry HAMT and method traces match
  81AG, as do Vector storage/methods, Range storage/native concatenation and the
  1,267-case value/hash oracle. Installed String checks pass 410 cases and 18
  numeric boundaries; Range passes 648 cases. All 145 stdlib assertions pass.
- ABI tests (four), host tests (86, one ignored), installer tests (four), and
  ABI/adapter Clippy with warnings denied pass. The focused adapter and Map
  scripts pass in debug. No release validation was run.
- A fresh external debug prefix contains 31 native libraries and 28 compiled
  modules. All 1,580 source hashes remained unchanged during installation; this
  validation record was added afterward.
- Both the previous installed build and the new prefix pass dynamic startup,
  a source-free compiled module, a saved v8 native Code image and a baked
  executable. They preserve existing/new split results, persistent Vector
  versions, aliases, UTF8 data, empty/invalid separators, nonoverlapping matches,
  native Vector construction despite public overrides, live split replacement
  and GC. The new adapter is observed loaded during startup, image and baked runs.

Receipts are `.cargo-target/81ah-{check,adapter,map,compare,unit,clippy,prefix,
prefix-native,image-baseline,image,stdlib}.log`, plus source hashes and per-fixture
baseline, development and installed traces.

## Next boundary

Review the remaining Collection coordinator as a separate unit: shared parent
construction/adoption, selector roots, retained execution-table handoff and the
ordered family initialization/publication sequence. All String/Vector/Range/Map
method and storage bodies have now left Collection. Further extraction must
preserve that interleaving and its reentry behavior, rather than merely relocate
the coordinator or discard its still-observable state.
