# Native loading through System

System wraps the common [Id descriptor protocol](../../rust/birnam-abi/tests/fixtures/id-protocol-v1/id_protocol.h).
`[System loadModule: path]` supplies no initializer operands.
`[System loadModule: path arguments: sequence]` sends `size` and zero-based `at:`
to the sequence and supplies the resulting Objects in order. Size must be a
nonnegative Integer. No native addresses, runtime service bundle or inferred
dependencies are supplied. The initializer defines its own input contract.

The wrapper copies path bytes and retains argument Objects across effectful
sequence messages and initialization. It requires no collection implementation;
Vector or an application-defined sequence may supply the protocol. The explicit
argument form does not inspect argument Objects to guess their native types.

The native entry is `birnam_native_module_v1`. An Object result is returned to
the caller; Unit returns the original receiving Object. Other native result kinds produce `typeError`.
Mapping failure produces `ioError`; a missing/incompatible descriptor produces
`notAModule`; other initialization failures produce `moduleError`. Invalid path
argument types/arity retain the existing nil result convention.

Id performs the loading message. Entered native modules remain mapped for the
process lifetime, even after initialization failure, because their effects may
already have escaped. There is no fallback to `module_init`. Generated code uses
the separate artifact/Execution lifetime rules.

Native Loader is authored in `modules/native-loader.birnam` and loaded as a
compiled artifact after Lang. A typed Bootstrap operation exposes Id's load
outcome; path/argument checks and error mapping stay authored. The retired native
descriptor and service-table inputs are removed. See the
[Native Loader contract and tests](../design/authored-small-modules.md#native-loader).

The aggregate `module_init` entry and exported dependency setters are retired.
Every native module is constructed through `birnam_native_module_v1` with its
explicit completed inputs. The installed native gate checks every library for
absence of those obsolete exports, plus retired preparers and factory/provider
aliases, in addition to the exact module inventories. The current production
prefix is checked by `mise run native-messages`; installed Host/CLI and Image
entry are checked by `scripts/host-cli-test.sh`.


Source files use the separate `[System load: path]` protocol. Its initializer also
receives Namespace, installs its method through Id and returns Unit. See the
[Source Loader contract](native-loading.md#loading-source-files) for Codegen
registration/evaluation and the current embedding and Host/Image boundaries.


[Block Loop](../design/authored-small-modules.md#block-loops) is authored and loaded
as compiled artifacts after Lang. Its synchronous entries preserve the bootstrap
suspension barrier; Module reload preserves later method replacements.

## Loading source files

`[System load: "file.birnam"]` uses the authored
[SourceLoader](../../modules/source-loader.birnam). It reads a bounded file snapshot,
selects Codegen, registers declared source metadata, then evaluates persistently
with the original path. Registration is not rolled back after an evaluation error;
a duplicate/sealed registration returning nil still permits evaluation. Syntax and
runtime errors preserve their stderr diagnostic formats and Error results.

Wrong arity/type returns nil. File errors precede the missing-Codegen check.
Successful declared modules are optionally marked loaded through Namespace's
Module binding; Globals remains separate. The source loader itself is marked loaded
at bootstrap, so ordinary Module reload does not reset a replaced System `load:`.
