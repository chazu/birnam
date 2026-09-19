# 81AQ — System Input owns readLine

> **Status:** Implemented, following Integer's Error Value migration in 81AP.

Move Prelude's existing readLine method intact into a System Input module.
Preserve zero-argument admission, ignored receiver/argument pointer, stdin lock
and buffering, binary bytes, removal of only the final LF, EOF nil, and lazy
ioError construction. Keep the stdin lock and line buffer alive through String
or Error callbacks, including reentry. No input normalization or new behavior.

Use the System Output installation boundary: an immutable 24-byte descriptor
with install(registration, execution, System behavior, readLine selector). Prelude
keeps System creation/adoption, all twelve selector interns and the original
registration position. Pass its original execution table even if registration
reentered with another table before the first installation. Retain that pointer
before registering the method; behavior and selector are borrowed synchronously.

The new provider depends on prepared String and Error Value. Initialization only
enables the provider; create no objects, roots or input buffers. Load before
Prelude, attach named dependencies, and retain the existing module lifetime.
There is no IdApi, representation, image encoding or public selector change.

Compare independent native traces and byte results against installed 81AP for
registration reentry, live callbacks, binary/empty/partial/multiple lines, CR/LF,
EOF, read errors, nested reads, snapshots, replacement and GC. Verify dependency
admission and no eager registration/allocation. Check fresh external debug
startup, a source-free compiled module, saved v8 native Code image and baked
executable before committing. No release validation.

## Validation

- readLine is copied byte-for-byte. Every remaining Prelude function body is
  unchanged except the replacement of its one registration call with the Input
  installer. Prelude retains System identity/adoption, all selector interning,
  registration order and its first execution-table selection.
- The independent native oracle passes 680 byte/receiver cases in each of three
  modes: normal, first readLine-intern reentry and first print-binding reentry.
  Complete traces match installed 81AP, development libraries and the fresh
  installed prefix. The first installer may receive a different registration
  table while methods still use Prelude's original execution table.
- Cases cover empty/unterminated/LF-terminated input, CR/NUL/invalid UTF-8 bytes,
  null/arbitrary receivers, ignored zero-count argument pointers, EOF, current
  nil, null/alias String results, constructor/context replacement, multiple
  buffered lines, a 20,000-byte line and callback/precise GC. Invalid arity reads
  nothing and performs no String or Error callbacks.
- A sampled baseline stack confirms a recursive read from the String constructor
  waits on stdin's mutex while the outer readLine still holds its lock. Bounded
  child-process checks preserve this blocking on both success and error paths;
  they signal entry before the nested call and are terminated after observation.
  This is existing behavior, not a newly supported form of reentrant input.
- A directory stdin descriptor produces the existing ioError. Absent factories
  still construct kind/message, null imports return current nil, and Error
  callbacks preserve factory publication, imported snapshots, null/alias results,
  ordinary replacement and custom lookup. Copied Methods survive replacement;
  reload and direct installation retain the original execution table. Changes in
  earlier registration callbacks preserve the selected behavior/selector for the
  current installation, and the next installation adopts the new selection.
- Descriptor checks reject null, version/extent mismatch, a missing operation
  and retargeting; reattaching the same pointer succeeds. Provider initialization
  creates no Id objects, bindings or roots. Method ownership changes from Prelude
  to Input. Both development and installed Input libraries match the five-symbol
  inventory, relocatable install name and system-only linkage. Prelude adds only
  the Input dependency export; Integer, Float and Word exports remain unchanged.
- Focused Integer error/arithmetic/admission, Float and Word error/arithmetic,
  Error Value, System Output, Text View, System GC, Printing and Boolean checks
  pass, including installed native comparisons against 81AP. All 145 stdlib
  assertions pass. ABI tests (four), Prelude's identity test, host tests (86, one
  ignored), builder tests (four) and Input Clippy with warnings denied pass.
- The fresh external debug prefix contains 37 native libraries and 28 compiled
  modules; Input is its sole new library. Installed package metadata and all
  65 required libraries match the updated inventory. All 1,660 source hashes
  stayed unchanged during installation; this validation record was added
  afterward. No release validation or full install-smoke run was performed.
- Installed 81AP and the new prefix pass dynamic startup, a source-free compiled
  module, saved v8 native Code image and baked executable. Binary input, empty and
  partial lines, EOF, retained String/receiver aliases and precise GC survive.
  Saved and baked execution also run with failing stdin and preserve ioError,
  message bytes and fresh Error factory replacements. Receiver-specific readLine
  replacement remains separate from the System singleton's method.

Receipts are `.cargo-target/81aq-{check,compare,system-input,unit,clippy,
integer-error-value,float-error-value,word-error-value,float,word,error-value,
system-output,text-view,system-gc,printing,boolean,prefix,prefix-native,
image-baseline,image,stdlib}.log`, the sampled baseline lock stack, source hashes
and baseline/development/installed traces for all three registration modes.

## Next boundary

Extract System getEnv: into an Environment provider using the same installation
boundary. It already depends on Text View for exact String bytes, String for
successful results and Error Value for failures. Preserve live name lookup,
UTF-8 validation, missing-variable nil, non-Unicode value errors, the original
execution table and the existing registration slot. Use isolated test variables
and independent byte/callback comparisons; do not add setters, enumerate the
process environment or change the remaining System file/module loaders.
