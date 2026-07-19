# Digla

Digla is the terminal programming environment for Birnam. Its source module is
composed from [`modules/digla/`](../../modules/digla/); the project manifest
also builds optional precompiled Termbox and reflection wrappers through
`[[wrapped]]`. UI startup does not compile Alien trampolines at runtime, and
full-image method reflection uses a project-local reverse-name index rather
than repeatedly scanning every global.

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
highlighting, cached parser-backed diagnostics, and terminal-cell-aware transcript
wrapping. Shift with the arrow, Home, or End keys extends a source selection;
typing, Backspace, or Delete replaces/removes it. Long multiline buffers keep
the cursor in a visual-row viewport, and long logical lines soft-wrap without
changing source byte positions. Consecutive terminal character events are inserted
as one edit, so large pastes do not rebuild the persistent buffer once per byte.
Tab opens completion for scoped source
names, Birnam keywords, and bounded live-image globals; Tab/Down and Up move
through the menu, Enter accepts, and Escape dismisses it. F1 opens source/runtime
hover, F2 shows the full selector signature, and F3 first jumps within the buffer,
then lazily opens authored source from the active project or its dependencies.
F5 restores the previous buffer and cursor; navigation preserves unsaved buffers
in memory and labels loaded source paths in the status row. F4 explicitly refreshes
diagnostics. A `*` after the diagnostic count means the cache is stale after editing;
typing never reparses the whole buffer. Enter
evaluates the current Birnam form, for example `[1 + 2]`. Page Up and Page Down
move through wrapped transcript rows; a new evaluation resumes following the tail.

The ordinary `birnam run examples/digla` path reuses its already-resolved project
snapshot, so source navigation needs no workspace flag and does not repeat manifest
or dependency resolution. `--workspace PATH` (or `DIGLA_WORKSPACE`) selects a
different authored project when Digla is deliberately used as an external client.

Runtime operations use a transport-neutral session rather than calling the
evaluator from the terminal model. The current commands are:

```text
:inspect NAME   inspect a named global
:inspect        inspect the last successful result
:globals        list user-visible live-image globals
:session        show image identity and capabilities
:connect TARGET switch to the local or configured remote image
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
`birnam/inspect` can inspect the last successful evaluation. `System print:` and
`System println:` calls made by evaluated code are captured into bounded, ordered
`birnam/output` notifications before the evaluation response and are also kept in
that response's `events` array for simple request/reply clients. The current
Digla application adapter advertises `streamedOutput=true`: framed stdio/TCP
clients receive one correlated notification as each `System` write occurs, while
the bounded aggregate remains in the response as a fallback. The current terminal
submit path is still serial, so its screen incorporates those collected events
when the evaluation response returns; a headless or AI protocol client can consume
them immediately.

`birnam/evaluate` also accepts an optional nonnegative `timeoutMs`. In the Digla
project image, a positive timeout installs a cooperative deadline on Birnam's
canonical stack-flat loops (`Block whileTrue:/whileFalse:` and `Integer to:do:`).
An expired deadline returns a structured `phase="timeout"` error while keeping
the live session usable. The terminal uses `evaluation-timeout-ms` from Digla's
config (`--evaluation-timeout-ms=5000`, `DIGLA_EVALUATION_TIMEOUT_MS`, or TOML);
the default is `0`, meaning unlimited.

The TCP server also advertises `interrupt=true` and `cooperativeInterrupt=true`.
While an evaluation is active, a client may pipeline `birnam/interrupt` on the
same connection with an optional `params.requestId`. The server nonblockingly
checks for that frame at the same canonical Block/Integer loop boundaries, answers
the interrupt request immediately, and returns the evaluation with
`phase="interrupted"`; stale request IDs are rejected and the session remains
usable. Stdio and the current serial terminal submit path still advertise or
behave as non-interruptible. Recursive code, foreign calls, and native operations
without those safe boundaries cannot be preempted.

The same protocol is available to a long-lived loopback client over TCP:

```sh
zig-out/bin/birnam run examples/digla --build -- --session-listen --port=4400
```

In a second terminal, the ordinary Termbox editor can target that image instead
of evaluating in its own process:

```sh
zig-out/bin/birnam run examples/digla --build -- --ui --connect=remote --port=4400
```

The editor keeps its buffer, source analysis, navigation, history, layout, and
transcript locally. Evaluation, inspection, runtime completion, globals, and
session identity cross the serial framed connection. The connection opens lazily
on the first runtime request and is closed when the editor exits. `--bind` selects
another endpoint address; non-loopback servers still require the explicit
`--allow-remote` security opt-in described below.

The same editor can switch without restarting: `:connect remote` handshakes with
the configured `--bind`/`--port`, while `:connect local` returns runtime requests
to the client process's image. Buffer, transcript, history, navigation, and the
immutable startup configuration survive the switch.

Use `--session-listen-once` for a script or smoke test that should serve one
connection and exit. The server accepts one client at a time, caps frames at
1 MiB, and defaults to `127.0.0.1`. A non-loopback `--bind` is refused unless
`--allow-remote` is also present; that opt-in exposes arbitrary live-image
evaluation and should only be used behind an appropriate security boundary.
