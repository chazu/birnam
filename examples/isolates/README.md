# Isolate examples

These examples build explicit worker images and exercise blocking process
isolates.

From the repository root:

```sh
scripts/rust-install.sh install --root . --prefix "$HOME/.local" --profile debug
bash examples/isolates/run.sh
```

The examples cover:

- `copied-values.birnam` — nested maps/vectors and scalar values crossing the
  wire boundary;
- `two-workers.birnam` — two independent, single-flight handles;
- `failures.birnam` — structured application failures, continued use after an
  application error, and a zero-budget deadline expiring before transmission
  without consuming the worker;
`build-image.birnam` loads `worker.birnam` before initializing the current
generation, then saves `/tmp/birnam-isolate-example.image`. Coordinator examples
start clean processes from that explicit image. The runner removes it on exit.
