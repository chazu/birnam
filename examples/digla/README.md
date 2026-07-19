# Digla

Digla is the terminal programming environment for Birnam. Its source module is
composed from [`modules/digla/`](../../modules/digla/); the project manifest
also builds the optional Termbox native library through `[[wrapped]]`.

Build and run the first configuration-backed slice from the repository root:

```sh
zig-out/bin/birnam build examples/digla
zig-out/bin/birnam run examples/digla --build -- --port=5555
```

The default entrypoint prints the resolved AppConfig snapshot. To launch the
first local Termbox editor slice on a real terminal:

```sh
zig-out/bin/birnam run examples/digla --build -- --ui
```

The editor supports character insertion, UTF-8-aware backspace, cursor motion,
history, multiline input with Ctrl-J, local evaluation with Enter, lexical
highlighting, parser-backed diagnostics, and terminal-cell-aware transcript
wrapping. Tab opens completion for scoped source names, Birnam keywords, and
bounded live-image globals; Tab/Down and Up move through the menu, Enter accepts,
and Escape dismisses it. Enter evaluates the current Birnam form, for example
`[1 + 2]`.

Runtime operations use a transport-neutral session rather than calling the
evaluator from the terminal model. The current commands are:

```text
:inspect NAME   inspect a named global
:inspect        inspect the last successful result
:globals        list user-visible live-image globals
:session        show image identity and capabilities
:help           show command help
```

For a persistent headless client, including an AI agent, launch the same live
image with framed JSON-RPC on stdin/stdout:

```sh
zig-out/bin/birnam run examples/digla --build -- --session-stdio
```

Messages use `Content-Length` framing and the methods
`birnam/session/open`, `birnam/evaluate`, `birnam/inspect`, `birnam/globals`,
`birnam/runtimeCompletion`, `birnam/interrupt`, and `birnam/close`. Responses
contain only copied JSON-safe values; the local adapter retains raw runtime values only so a later
`birnam/inspect` can inspect the last successful evaluation. The next transport
slice is a loopback TCP adapter over this same contract.
