# Native Loader

`libbirnam_native_loader` initializes with Namespace and installs the existing
System methods through Id sends. It retains Roots, String/StringVt, Integer, nil
and System; Error is resolved from Namespace when an error is constructed.
Initialization returns System. Reloading with the same Id, Namespace and owner
thread returns System without reinstalling methods.

| System message | Result |
|---|---|
| `loadModule:` | Load the exact String path with no initializer arguments. |
| `loadModule:arguments:` | Read the sequence's Integer `size`, then `at:` with Integer indices starting at zero; pass those Objects to the initializer. |

A successful native Unit returns the original receiver; an Object returns that
Object. Wrong language arity or a non-exact-String path returns nil. A non-Integer
size or negative count produces the existing `typeError`. Native argument tags
and failed dependency sends follow the current Id boundary contract.

The path bytes are copied before sequence sends, as before this migration.
Receiver, arguments, size, indices and extracted Objects remain rooted across
collecting sends and initialization. Integer recognition/construction uses
`wordOf:`/`fromWord:`. No dependency method entry is cached.

Id receives the existing load message and owns mapping, descriptor validation,
initializer entry and image pinning. Native Loader preserves its error mapping:

| Id outcome | Language error |
|---|---|
| Mapping failure | `ioError`: `could not load module dylib` |
| Missing/incompatible descriptor | `notAModule`: `missing or incompatible birnam_native_module_v1` |
| Other load failure | `moduleError`: `native module initialization failed` |
| Successful non-Unit/non-Object result | `typeError`: `native module returned a non-language value` |

`mise run native-messages` builds this module through production Bootstrap and
runs `native_loader` against independent C dylib fixtures. Coverage includes
actual admission/results, compiled source calls, path mutation, precise collection
inside dependencies and initializers, transient argument lifetime and cleanup,
replacement/failure propagation, lazy Error lookup, and reload preservation.
The installed Image group also runs `native-loader-image-smoke.sh`: descriptor
loading through source and source-free artifacts, cloned System receiver identity,
private receiver methods, mapped/rebuilt recovery and a baked executable. Image
main preserves the running-phase guard against direct method replacement.
