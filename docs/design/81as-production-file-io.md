# 81AS — File IO owns System file reads and writes

> **Status:** Implemented, following Environment in 81AR.

Move readFile: and writeFile:contents: unchanged from Prelude into File IO.
Preserve exact borrowed String arguments, binary contents, the one-GiB read
limit, existing ioError messages and receiver results. Keep the path borrowed
across the contents callback before UTF-8 conversion; callbacks may change it.
Successful reads own their bytes through the current String constructor.

Use a 24-byte installation descriptor with registration/execution tables and the
already selected System behavior and both selectors. Prelude retains System
identity/adoption, all twelve interns and the original two adjacent bindings.
Record its first execution pointer before registration callbacks, including
reentry with a different registration table. Keep selector/behavior arguments
synchronous and unretained. Reload retains the original execution table.

File IO depends on Text View, String and Error Value and loads before Prelude.
Initialization only enables its provider: no Id objects, roots, methods or open
files. Copy the tiny path helper; Prelude still needs it for source loading.
No IdApi, representation, method ABI, image format or public protocol changes.
No compatibility facade or new file operations.

Compare independent native traces against installed 81AR. Cover binary bytes,
empty and failed IO, exact/inherited arguments, live String/Error callbacks,
borrowed-path mutation, nested reads/writes, GC, registration reentry, snapshots
and reload. Verify a fresh external debug startup, source-free compiled module,
saved v8 native Code image and baked executable, then commit. No release
validation.

## Validation

- Both methods and their argument/path helpers are copied verbatim. Every other
  Prelude function body is unchanged except the two binding calls replaced by
  File IO installation. System identity/adoption, twelve selector interns,
  registration order and original execution selection remain intact.
- The independent C oracle passes 680 binary read/write/receiver cases in each
  of three modes: normal, first readFile:-intern reentry and first print-binding
  reentry. Complete traces match installed 81AR, development libraries and the
  fresh installed prefix. It uses only files in its own temporary directory.
- Sparse files exercise the real one-GiB boundary: exactly one GiB reaches the
  String constructor with the full extent; one additional byte produces the
  existing error. The mocked large-result constructor avoids a second one-GiB
  copy. Empty files, byte-for-byte writes, truncation, null/arbitrary receivers,
  invalid UTF-8, interior NUL, empty paths, directories and missing files retain
  their results. Wrong arity, null carriers and inherited arguments skip later
  callbacks. An unavailable byte view and a zero-length view remain distinct.
- Vtable callbacks select the current String byte operation; authoritative
  recognition and externally borrowed extents remain effective. Path callbacks
  can replace the result constructor/context. Contents callbacks can redirect
  the earlier borrowed path, invalidate its UTF-8 or make it valid before path
  conversion. Invalid contents still return nil before path conversion/errors.
- Null and alias String constructor results are preserved. Nested writes and
  reads during construction leave the outer owned read buffer intact, including
  callback GC. Missing/null/replaced Error factories, current nil, null/alias
  results, publication during message construction, imported factory snapshots
  and custom lookup preserve lazy errors and both exact messages.
- Both methods run during registration. Reentry with a different registration
  table still uses Prelude's original execution table; copied Methods and direct
  installation retain it after reload. Mutation at the first binding preserves
  the second selector and behavior snapshot; the next installation adopts the
  new selections. Precise GC preserves retained values.
- Admission rejects null, wrong version/extent, missing install and retargeting;
  repeat attachment of the same pointer succeeds. Provider initialization adds
  no Id allocations, methods or roots. Development and installed libraries match
  the six-symbol inventory, relocatable install name and system-only linkage.
  Prelude adds only File IO attachment; checked earlier providers retain their
  export inventories.
- Focused Environment, Input, Integer arithmetic/error/admission, Float and Word
  numeric/error, Error Value, Output, Text View, System GC, Printing and Boolean
  checks pass, including installed comparisons against 81AR. All 145 stdlib
  assertions pass. ABI tests (four), Prelude identity (one), host tests (86, one
  ignored), builder tests (four) and File IO Clippy with warnings denied pass.
- The fresh external debug prefix contains 39 native libraries and 28 compiled
  modules; File IO is its sole added library. Installed metadata and all 67
  required libraries match the updated inventory. All 1,682 source hashes stayed
  unchanged during installation; this validation record was added afterward.
  No release validation or full install-smoke run was performed.
- Installed 81AR and the new prefix pass dynamic startup, a source-free compiled
  module, saved v8 native Code image and baked executable. Binary payloads and
  aliases survive image restore and write back byte-for-byte. Reads see changed
  files in the current process while saved bytes remain intact. Empty files,
  read/write errors, precise GC and fresh method/factory replacements pass.

Receipts are `.cargo-target/81as-{check,compare,file-io,unit,clippy,environment,
system-input,integer-error-value,float-error-value,word-error-value,float,word,
error-value,system-output,text-view,system-gc,printing,boolean,prefix,prefix-native,
image-baseline,image,stdlib}.log`, source hashes and baseline/development/installed
traces for each reentry mode.

## Next boundary

Extract Block whileTrue: and whileFalse: together, with their shared loop body.
Preserve fresh value sends on every iteration, exact Boolean identity checks,
Error recognition/propagation and evaluation order. Prelude should retain Block
identity, capture layout and creation/adoption, selecting both selectors before
registration and supplying its original execution table. This is independent of
source evaluation and native-module loading, which remain separate boundaries.
