# 81AU — Native Loader owns System loadModule:

> **Status:** Implemented, following Block Loop in 81AT.

Move loadModule: unchanged from Prelude into Native Loader. Preserve exact String
recognition, borrowed bytes followed by an owned CString, interior-NUL rejection,
RTLD_LAZY, live dlopen/dlsym callbacks and the original execution pointer passed
to module_init with null state. Ignore the initializer result and return the
receiver. Preserve existing errors, handle lifetime, callback order and reentry.
Do not add dependency admission or unloading to this direct loader path.

Use a 24-byte installer with registration/execution tables, selected System
behavior and loadModule: selector. Prelude keeps System identity/adoption, all
selector interns and the original binding position. Retain its first execution
pointer before registration, including reentry with a different registration
table. Native Loader depends on Text View and Error Value and loads before
Prelude. Initialization only enables its provider: no Id objects, roots, handles
or methods. No IdApi, representation, public protocol or image encoding changes.

Compare independent native traces against installed 81AT, including byte paths,
errors, live loader/String/Error callbacks, owned path lifetime, nested loading,
API/state forwarding, registration reentry, snapshots and GC. Verify real native
loads through fresh external debug startup, source-free compiled module, saved
v8 native Code image and baked executable, then commit. No release validation.

## Validation

- loadModule: and its argument helper are copied verbatim, including the original
  Runtime API pointer passed to module_init. Every other Prelude function body
  is unchanged except replacing the method binding with installation. System
  identity/adoption, selector interns and registration order remain intact.
- The independent C oracle passes 1,020 path/failure/receiver cases in each of
  three modes: normal, first loadModule:-intern reentry and first print-binding
  reentry. Complete traces match installed 81AT, development libraries and the
  fresh installed prefix. Cases distinguish interior NUL, arbitrary non-UTF-8
  bytes, empty paths, open failure, missing initializer and success. Callback
  counts prove which stages run and which failures skip subsequent stages.
- Wrong arity, null carriers and unavailable views skip loading. Null/inherited
  paths have no printing fallback; a zero-length byte view still reaches dlopen
  with an empty CString. Vtable callbacks select the current byte operation,
  authoritative recognition and externally borrowed extents remain effective,
  and byte callbacks can select the current loader operation.
- dlopen receives RTLD_LAZY and dlsym receives the exact returned handle and only
  module_init. Open callbacks can replace the symbol operation. The CString
  remains independent of mutations to the original borrowed bytes and survives
  nested loading during open, symbol lookup and initialization, with callback GC.
- Initializers receive the exact original execution pointer and null state,
  including registration/reload with a different table. Null and nonnull
  initializer returns are ignored. Success returns the original receiver,
  including null; failure uses current nil or the existing lazy Error result.
- Both error kinds and messages match. Missing/null/replaced Error factories,
  null/alias messages and results, current String constructor/context, nested
  error construction, publication during message creation, imported snapshots
  and custom factory lookup preserve callback order and behavior.
- The method runs during registration. Copied Methods survive replacement,
  Prelude reload and direct installation while keeping the original execution
  table. Mutation during the preceding load: binding preserves the current
  System behavior/selector snapshot; the next installation adopts new values.
  Precise GC preserves retained objects.
- Admission rejects null, wrong version/extent, missing install and retargeting;
  repeat attachment succeeds. Initialization creates no Id objects, methods or
  roots. Development and installed libraries match the five-symbol inventory,
  relocatable install name and system-only linkage. Prelude adds only Native
  Loader attachment; checked prior providers retain their export inventories.
- Focused Block Loop, File IO, Environment, Input, Integer arithmetic/error/
  admission, Float and Word numeric/error, Error Value, Output, Text View, System
  GC, Printing and Boolean checks pass, including installed comparisons against
  81AT. All 145 stdlib assertions pass. ABI tests (four), Prelude identity (one),
  host tests (86, one ignored), builder tests (four) and Native Loader Clippy with
  warnings denied pass.
- The fresh external debug prefix contains 41 native libraries and 28 compiled
  modules; Native Loader is its sole added library. Installed metadata and all
  69 required libraries match the refreshed inventory. All 1,706 source hashes
  stayed unchanged during installation; this validation record was added
  afterward. No release validation or full install-smoke run was performed.
- Installed 81AT and the new prefix pass real loading through dynamic startup,
  source-free compiled module, saved v8 native Code image and baked executable.
  The target library verifies the loader Method's owner and original API/null
  state, deliberately rejects dependency attachment, returns a nonnull ignored
  result and installs a callable method reporting repeated initialization.
  Missing libraries, missing module_init, retained receiver/path aliases, precise
  GC and fresh method/factory replacement preserve results. The target's method
  remains callable after initialization; each explicit load runs its initializer.
- Block Loop's startup/compiled/image/baked fixture also passes against the new
  prefix, exercising real Prelude reinitialization through the extracted loader
  and verifying the resulting native loop Method owners.

Receipts are `.cargo-target/81au-{check,compare,native-loader,unit,clippy,block-loop,
file-io,environment,system-input,integer-error-value,float-error-value,
word-error-value,float,word,error-value,system-output,text-view,system-gc,
printing,boolean,prefix,prefix-native,image-baseline,image,block-loop-image,
stdlib}.log`, source hashes and baseline/development/installed traces for each
reentry mode.

## Next boundary

Extract System load: into Source Loader, including its source-file read, declared
module staging, evaluation, diagnostics and loaded-module notification. Preserve
the read limit, borrowed path through callbacks, owned source bytes, Codegen
selection, fresh sends, lazy errors and exact ordering. Keep System identity and
registration in Prelude. Move its actual Integer, String, Text View and Error
Value consumers; remove Prelude dependencies only when their final consumer has
moved. Leave the identity/layout coordinator intact.
