# 81AR — Environment owns System getEnv:

> **Status:** Implemented, following System Input in 81AQ.

Move Prelude's getEnv: method and its argument helper unchanged into Environment.
Keep exact borrowed String-name access through Text View, UTF-8 validation, live
environment lookup, missing-variable nil, successful String construction and the
existing ioError policy for invalid Unicode. Preserve the owned value bytes
through String callbacks, including nested reads and environment mutation.

Use the existing 24-byte installation descriptor with registration/execution
tables and the already selected System behavior/getEnv: selector. Prelude keeps
System identity/adoption, all twelve selector interns and the original binding
position. Pass its first execution table even when registration reenters with
another table before installation. Retain that pointer before registering the
method; keep behavior/selector arguments synchronous and unretained.

Environment depends on Text View, String and Error Value, loads before Prelude,
and initializes only its provider. Add no setters, enumeration, objects, roots,
new lookup semantics or compatibility facade. No IdApi, representation, method
ABI or image encoding changes.

Compare independent native traces against installed 81AQ using isolated test
variables. Cover binary/Unicode names and values, missing/empty results, exact
versus inherited Strings, live callbacks, constructor reentry, factory dispatch,
registration snapshots and GC. Verify admission, fresh external debug startup,
source-free compiled module, saved v8 native Code image and baked executable,
then commit. No release validation.

## Validation

- getEnv: and its argument helper are copied verbatim. Every other Prelude
  function body is unchanged apart from replacing the getter's registration
  with the Environment installer. System identity/adoption, all selector interns,
  registration order and Prelude's original execution selection remain intact.
- The independent oracle passes 680 name/value/receiver cases in each of three
  modes: normal, first getEnv:-intern reentry and first print-binding reentry.
  Complete traces match installed 81AQ, development libraries and the fresh
  installed prefix. Tests use process-specific synthetic keys, not environment
  enumeration or unrelated variables.
- Cases preserve empty/missing values, binary name rejection, non-Unicode value
  errors, Unicode names/values, null/arbitrary receivers and exact versus inherited
  String recognition. The macOS baseline's KEY=... lookup behavior remains
  delegated unchanged, including earlier UTF-8 and interior-NUL handling.
  Wrong arity, null carriers and unavailable views retain their original nil
  results and skip subsequent policy callbacks.
- Name-byte callbacks can change the environment before lookup. Vtable callbacks
  select the current byte operation; exact recognition and a borrowed external
  name extent remain authoritative. A name callback can replace the String
  constructor/context before result construction. Null and alias constructor
  results remain unchanged.
- Nested construction can mutate the variable and perform another lookup while
  preserving the outer owned value bytes. Nested error construction and callback
  GC also match the baseline. Missing/null/replaced Error factories, current nil,
  null/alias results, factory publication during message construction, imported
  snapshots and custom lookup preserve the existing lazy policy.
- Methods run during registration. Reentry with another registration table still
  uses Prelude's first execution table; copied Methods and direct installation
  retain it after reload. Mutation during the preceding writeFile:contents:
  registration preserves the current behavior/selector snapshot, while the next
  installation adopts the new selection. Precise GC preserves retained objects.
- Admission rejects null, wrong version/extent, missing install and retargeting;
  repeated attachment of the same pointer succeeds. Provider initialization
  creates no Id objects, methods or roots. Development and installed libraries
  match the six-symbol inventory, relocatable install name and system-only
  linkage. Prelude adds only Environment attachment; the checked numeric,
  Input/Output, Text View and Error Value export inventories are unchanged.
- Focused Input, Integer error/arithmetic/admission, Float and Word error/numeric,
  Error Value, Output, Text View, System GC, Printing and Boolean checks pass,
  including installed comparisons against 81AQ. All 145 stdlib assertions pass.
  ABI tests (four), Prelude's identity test, host tests (86, one ignored), builder
  tests (four) and Environment Clippy with warnings denied pass.
- The fresh external debug prefix contains 38 native libraries and 28 compiled
  modules; Environment is its sole added library. Installed metadata and all
  66 required libraries match the updated inventory. All 1,671 source hashes
  stayed unchanged during installation; this validation record was added
  afterward. No release validation or full install-smoke run was performed.
- Installed 81AQ and the new prefix pass dynamic startup, a source-free compiled
  module, saved v8 native Code image and baked executable. The fixture changes
  the controlled value between source execution, save, restore and baked launch.
  Retained Strings and aliases survive while lookups see the current process's
  value. Empty/missing results, non-Unicode errors, precise GC and fresh method
  and Error factory replacements are preserved.

Receipts are `.cargo-target/81ar-{check,compare,environment,unit,clippy,system-input,
integer-error-value,float-error-value,word-error-value,float,word,error-value,
system-output,text-view,system-gc,printing,boolean,prefix,prefix-native,
image-baseline,image,stdlib}.log`, source hashes and baseline/development/installed
traces for all three registration modes.

## Next boundary

Extract System readFile: and writeFile:contents: together into File IO. Preserve
raw bytes, the existing one-GiB read limit and failure messages, exact String
arguments, receiver results, and live borrowed paths across the contents
callback. Keep both selectors and the selected System behavior fixed before
registration, using Prelude's original execution table. String, Text View and
Error Value are existing dependencies. Leave source evaluation and native-module
loading for separate boundaries, including their remaining path helper consumer.
