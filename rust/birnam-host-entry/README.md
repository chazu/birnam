# HostEntry

`libbirnam_host_entry.dylib` owns authored bootstrap and program/image/worker
entry through Codegen, Module, System and Isolate messages.

Initializes with six Objects: the completed Namespace, followed by String
paths for the installation prefix, library directory, Rust workspace, guides,
and Id library. Every input is retained while path bytes are copied. No private
foreign object layout or dependency-specific service pointer crosses the boundary.
Initialization returns the owner Object and publishes it in Namespace. Repeating
initialization with the same Id, Namespace, paths and creating thread returns the
same Object without overwriting methods; mismatched construction is rejected.

The descriptor is `birnam_native_module_v1`. Native mappings, owner receivers,
private behaviors and callback contexts live for the process lifetime. Calls on
another thread are rejected. Each call roots its receiver and Object arguments;
intermediate results use local scopes backed by the existing Roots Object.
Ordinary failures return `Error kind: #hostError message: ...`; successful entry
operations return nil unless a result is specified. Wrong arity is an Error.

| Message | Result |
| --- | --- |
| `bootstrap:tools:` | Restore the console/tool image, or explicitly select source recovery. Two Booleans. |
| `bootstrapSource` | Explicit source recovery. |
| `evalScript:` | Evaluate one bounded source path. |
| `compile:output:` | Compile one source path to an artifact. |
| `loadArtifact:` | Activate through Module. |
| `loadImage:rebuild:compiled:` | Live restoration; rebase Module's library directory after success. |
| `startImage:rebuild:compiled:` | Startup admission; caller must discard the world on failure. |
| `loadImageBytes:` | Live restoration from an Image byte provider; HostEntry passes the Oop to Image without decoding or copying its bytes. |
| `startImageBytes:` | Startup admission of baked image bytes. |
| `imageHasMain` | Boolean presence of global main. |
| `imageMain` | Enter saved main; fulfill its returned intent after evaluation returns, then checkpoint Isolate. |
| `worker:receiver:selector:fd:` | Restore and enter a worker without main. Path and names are Strings; fd is Integer. |
| `shutdown` | Shut down child workers. The executable sends this on teardown. |

Compiled startup rejects absent/incompatible images and metadata; source recovery
is explicit. Nil/false program results remain successful. Runtime rebasing,
worker admission diagnostics and intent fulfillment keep their prior ordering.

Installed acceptance uses `scripts/host-cli-test.sh` and the executable facade's
rooting, parser-disabled startup and generation/Image tests.
