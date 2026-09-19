# 81AK — Shared text byte views

> **Status:** Implemented, following System GC controls in 81AJ.

Move Prelude's string_bytes and printable_string helpers into `birnam-text-view`.
They implement the shared exact-String/printable-value byte policy used by System
output, file/environment arguments, native module paths and source-load diagnostics.
Their other similarly named counterparts return copies or have different checks;
leave those separate. Prelude's callers use an explicit native dependency at
exactly the former helper calls, with no compatibility helper left in Prelude.

The provider loads immediately before Prelude, depends on prepared String v1 and
retains no IdApi, objects, selectors or byte buffers. Initialization only enables
the provider. Its immutable 32-byte descriptor offers exact and printable byte
views, each receiving the caller's selected IdApi and value. A 16-byte C result
contains a pointer and length: null means unavailable; a nonnull zero-length view
means a present empty extent. Encode/decode Rust Option slices without copying
bytes, allocating objects or adding callbacks. Keep code and descriptor pinned.

Copy both helper bodies and their tiny intern/send adapter unchanged. Exact
recognition rejects null without callbacks, calls the supplied live vtableOf,
then reads String's selected behavior and live bytes operation. Test zero length
before rejecting a null data pointer. Printable first tries exact bytes, then
for nonnull failures interns printString and performs one fresh ordinary send,
then tries exact bytes on the result. An exact String with unavailable bytes
still reaches that fallback. Do not add type checks, escaping, Unicode validation,
selector caching, additional lookup or copying of the borrowed extent.

Borrowing is part of observable behavior. In writeFile:contents:, path bytes
remain borrowed across the contents-byte callback and are interpreted afterward.
Source-load diagnostics retain the path view across evaluation callbacks. Preserve
these read points, pointer identity, underlying lifetime obligations and aliasing;
do not silently replace a borrowed view with an owned snapshot. Callers keep the
existing objects/storage alive and honor the selected String operation's borrow
contract. The new API does not extend a provider's lifetime guarantee.

Validate independent C byte/callback cases and all actual Prelude consumers
against installed 81AJ, including null/empty extents, inherited/exact recognition,
failed exact reads that fall back, live vtable/bytes/send changes, custom lookup,
reentry and GC. Check path mutation during the second file argument and diagnostics
after evaluation. Validate a fresh external debug prefix, source-free compiled
module, saved v8 native Code image and baked executable before committing. Keep
System output's actual methods and OS IO in Prelude for the next slice. No release
validation, IdApi/layout/image change or new language functionality.

## Validation

- Both policy bodies and the intern/send adapters match 81AJ after normalizing
  formatting. All other Prelude function bodies are unchanged. Every existing
  consumer calls the new native dependency at its former helper call; Prelude
  has no forwarding helper or duplicated conversion policy.
- The independent C oracle passes 682 binary output cases, including embedded
  NUL and invalid UTF-8 bytes. Its complete byte/callback traces match installed
  81AJ, development libraries and the fresh installed prefix. Null values,
  malformed carriers, exact/inherited String recognition, unavailable bytes,
  null empty extents and printable fallback retain their prior results and
  callback counts. Live vtable/bytes selection, ordinary method replacement,
  effectful custom lookup, nested calls and callback-triggered GC pass.
- All actual consumers are exercised: output, binary file round trips,
  environment access, native module loading and both source-error diagnostic
  paths. A contents callback mutates the previously borrowed path before file
  creation; evaluation mutates a borrowed path before diagnostic formatting.
  Both mutations remain observable at the same read points as 81AJ.
- Direct descriptor calls preserve nonempty pointer identity and distinguish
  absent from present empty bytes. Alternating supplied IdApi tables proves
  that the operation uses each caller's table. The operation resolves into
  Text View. Reinstalling Prelude retains its original execution table, and
  precise collection after return preserves explicitly rooted values.
- Admission rejects null, wrong version/size, either missing operation and
  retargeting; repeat attachment of the same pointer succeeds. Development and
  installed libraries match the four-symbol inventory, relocatable install name
  and system-only linkage. Prelude's only export change adds text-view attachment.
- Focused Text View, System GC (496 cases in each of three modes), Printing
  (1,104 cases in each of two modes), Boolean (34), Float (676 arithmetic cases)
  and Word (81 equality cases) checks pass, including installed native checks.
  All 145 stdlib assertions pass. ABI tests (four), Prelude tests (two), host
  tests (86, one ignored), builder tests (four), and ABI/Text View Clippy with
  warnings denied pass. No release validation was run.
- The fresh external debug prefix contains 34 native libraries and 28 compiled
  modules. All 1,613 source hashes stayed unchanged during installation; this
  validation record was added afterward. Installed package metadata and all 62
  required libraries match the updated install-smoke inventory. The full
  install-smoke script was not run.
- Both the previous build and the fresh prefix pass dynamic startup, a
  source-free compiled module, a saved v8 native Code image and a baked executable.
  They exercise output, file/environment arguments, retained aliases, GC and
  fresh printing lookup after public method replacement. Text View is observed
  loaded during new startup, image and baked runs.

Receipts are `.cargo-target/81ak-{check,build,compare,text-view,system-gc,printing,
boolean,float,word,unit,clippy,prefix,prefix-native,image-baseline,image,stdlib}.log`,
plus source hashes and baseline/development/installed consumer traces.

## Next boundary

Extract System print:/println: and their stdout writer into a System output
module. They can now use the shared Text View dependency without duplicating
conversion policy. Keep Prelude's selector/behavior snapshots and original
execution table, register at the same two slots, and preserve byte output,
newline/flush ordering, ignored IO errors and receiver/nil results. Move the
existing writer test with its implementation; extend focused checks only where
the new installation boundary needs evidence.
