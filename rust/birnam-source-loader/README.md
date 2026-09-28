# Source Loader over Id

Bootstrap loads Source Loader after Codegen. Its native initializer takes the
completed Namespace Object, installs `System load:` through Id and returns Unit.
All object/value/compiler/retention operations use current sends. Reinitialization
preserves replacements. The old IdApi, factory and service-table inputs are gone.

`System load:` keeps its existing order and results:

1. Require one exact String path, copy its bytes before further sends, validate
   UTF-8 and read at most one GiB. Wrong language arity/type answers nil; file
   failures answer `ioError` with `could not read source file`.
2. Resolve Codegen in Namespace after the read. An absent/nil binding answers
   `noCodegen`. Construct the source String and send `sourceRevisionOf:`.
3. For a non-Error/non-nil revision, read index 1 and send
   `recordModuleSource:named:`. An exact Error stops loading; nil does not.
4. Send `evalPersisting:from:` with the source and original path Object. A nil
   result consults `lastError`; a non-nil diagnostic produces `syntaxError`.
   Other Error results are returned unchanged. Diagnostics retain their existing
   stderr format; OS reads/writes remain private to this external-service owner.
5. For a non-nil, non-Error result with a declared name, look up Module in
   Namespace and, if present, send `markLoaded:`. Ignore its language result.
   Native dependency failures propagate unchanged throughout.

Registration precedes evaluation and is not rolled back on evaluation failure.
An identical registration succeeds; changed source under the same name answers
nil. Sealing registration also answers nil, so the loader still evaluates that
file. This preserves the existing distinction between source bookkeeping and
ordinary immediate execution.

The authored Module manager in `src/id/modules.birnam` runs unchanged through
this loader. It publishes Module in Globals; Source Loader's optional notification
still checks Namespace. Loading a source declaration therefore need not mark it
loaded. Host startup/dependency orchestration and Module's Artifact-backed loading
remain later migrations. Bootstrap does not implicitly install the authored
manager or merge Globals with Namespace.

## Validation and current boundary

`bash scripts/native-message-test.sh` builds/audits the actual module and runs
`birnam-siphon/tests/source_loader.rs`. It loads the complete authored manager,
checks self-registration, dependency ordering and Store state, then loads real
files. Coverage includes revision values and independent digest expectations,
duplicate/sealed registrations, retained source identity, registration-before-error,
nil/syntax/runtime/file results, live replacements, native failure propagation,
collecting dependencies, reentrant registration, filename mutation and reloads.
Replacement methods forward through sends; they never invoke captured entries.

The [embedding slice](../../docs/design/56-embedded-files.md) now resolves literal
`System embedFile:` calls relative to this source file and returns Bytes through
messages. Compiled Blocks retain their payload after the source asset is deleted.
Full Host/CLI startup, artifact activation and Image recovery are not accepted by
these tests. Codegen's existing registration and evaluation selectors are described
[in its contract](../birnam-codegen-runtime/README.md#source-loader-protocol).
