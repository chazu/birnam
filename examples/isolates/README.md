# Isolate examples

These examples build explicit worker images and exercise blocking process
isolates plus I1b's health-checked single-worker replacement.

From the repository root:

```sh
zig build
bash examples/isolates/run.sh
```

The examples cover:

- `copied-values.birnam` — nested maps/vectors and scalar values crossing the
  wire boundary;
- `two-workers.birnam` — two independent, single-flight handles;
- `failures.birnam` — structured application failures, continued use after an
  application error, and a zero-budget deadline expiring before transmission
  without consuming the worker;
- `replacement.birnam` — select a v1 worker, boot and health-check v2, switch
  only between calls, and continue through the returned `currentWorker`.

`build-image.birnam` loads `worker.birnam` before initializing the current
generation, then saves `/tmp/birnam-isolate-example.image`. Coordinator examples
start clean processes from that explicit image. The runner removes it on exit.

The replacement example builds v1 and v2 with `LiveModules saveBaseline:`. Its
spec makes every boundary explicit: worker receiver/selector/image, the trusted
health request, and ready/health/stop deadlines. `replace:using:` returns an
`IsolateReplacement`; inspect `status`, `currentWorker`, `priorWorker`,
`pendingCleanup`, and `failure` instead of assuming success. This is a blocking
one-worker handoff—there is no pool, busy draining, retry, or concurrent routing.
