# Native loading through System

System wraps the common [Id descriptor protocol](fixtures/id-protocol-v1/id_protocol.h).
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

The Native Loader constructor receives one completed Namespace Object. It resolves
and retains its dependencies through messages, registers both methods through Id,
and returns System. Bootstrap loads it after the System and value prerequisites.
Its only exported symbol remains the common descriptor. The old Runtime, root,
Integer, Error and Text View service-table inputs are removed. See the
[Native Loader contract and tests](../rust/birnam-native-loader/README.md).

The aggregate `module_init` entry and exported dependency setters are retired.
Every native module is constructed through `birnam_native_module_v1` with its
explicit completed inputs. The installed native gate checks every library for
absence of those obsolete exports, plus retired preparers and factory/provider
aliases, in addition to the exact module inventories. The current production
prefix is checked by `mise run native-messages`; full installed Host/CLI and image
loader validation awaits those migrations.


Source files use the separate `[System load: path]` protocol. Its initializer also
receives Namespace, installs its method through Id and returns Unit. See the
[Source Loader contract](../rust/birnam-source-loader/README.md) for Codegen
registration/evaluation and the current embedding and Host/Image boundaries.
