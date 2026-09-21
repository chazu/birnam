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
the caller; Unit returns System. Other native result kinds produce `typeError`.
Mapping failure produces `ioError`; a missing/incompatible descriptor produces
`notAModule`; other initialization failures produce `moduleError`. Invalid path
argument types/arity retain the existing nil result convention.

Id performs the loading message. Entered native modules remain mapped for the
process lifetime, even after initialization failure, because their effects may
already have escaped. There is no fallback to `module_init`. Generated code uses
the separate artifact/Execution lifetime rules.

The Native Loader constructor receives the completed Runtime, roots, System,
namespace presence/import, Integer, Error helpers and Text View dependencies.
It returns System after registering both methods. Its only exported symbol is
the common descriptor. This replaces Native Loader v12's provider/installer ABI.

The aggregate `module_init` entry and exported dependency setters are retired.
Every native module is constructed through `birnam_native_module_v1` with its
explicit completed inputs. The installed native gate checks every library for
absence of those obsolete exports, in addition to the exact module inventories.
