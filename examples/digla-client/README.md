# Digla client

This project is the Termbox editor/browser client for the headless Digla image
service in [`../digla`](../digla/). Keeping it as a separate load root ensures
that stdio and TCP servers do not acquire Termbox, editor, or browser state.

Run a local client:

```sh
zig-out/bin/birnam run examples/digla-client --build
```

Or connect it to a headless image server:

```sh
zig-out/bin/birnam run examples/digla --build -- --session-listen --port=4400
zig-out/bin/birnam run examples/digla-client --build -- --connect=remote --port=4400
```
