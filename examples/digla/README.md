# Digla

Digla is the terminal programming environment for Birnam. Its source module is
composed from [`modules/digla/`](../../modules/digla/); the project manifest
also builds the optional Termbox native library through `[[wrapped]]`.

Build and run the first configuration-backed slice from the repository root:

```sh
zig-out/bin/birnam build examples/digla
zig-out/bin/birnam run examples/digla --build -- --port=5555
```

The current entrypoint prints the resolved AppConfig snapshot. Terminal editing,
history, syntax highlighting, and image sessions will build on the same module.
