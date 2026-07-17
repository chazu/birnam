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
history, multiline input with Ctrl-J, and local evaluation with Enter. Enter
evaluates the current Birnam form, for example `[1 + 2]`. The remote session
protocol, syntax services, and full position maps are later slices.
