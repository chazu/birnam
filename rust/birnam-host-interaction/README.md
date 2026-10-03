# HostInteraction

`libbirnam_host_interaction.dylib` owns console/socket session turns and authored
tool invocation. Evaluation returns before intent fulfillment; turn roots are
released at their existing boundaries and Isolate checkpoints remain in order.

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
| `repl:` | Console entry; Boolean selects caller configuration. |
| `socket:bind:` | TCP accept/session loops; Integer port and String bind address. |
| `cli:` | Invoke authored Cli with a Vector of String arguments. LSP queries HostProject directly. |
| `loadTool:` | Load a named authored tool, restoring the previous Module library directory. String name bytes. |

Project/image entry uses `repl: false`, preserving published context and avoiding
another main invocation. Socket EOF, quit and client errors close that connection
before the next accept. Console teardown runs after initialization even on failure.

Installed checks include the REPL golden transcripts, project LSP smoke test,
published project tty/socket entry and console/TCP generation activation/rollback.
