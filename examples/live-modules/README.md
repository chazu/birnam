# Live-module examples

This example exports an exact file-backed revision, stages and inspects an edit,
activates it through the L1a intent, saves a full image baseline, boots that
baseline through both mapped and rebuilt recovery, and starts an isolate from it.

From the repository root:

```sh
zig build stage1
bash examples/live-modules/run.sh
```

`worker-v1.birnam` and `worker-v2.birnam` intentionally contain only the L1b
candidate-safe shape: one canonical module declaration plus declarative method
operations over the already-established `LiveExampleWorker` receiver. L1b does
not run changed top-level effects, introduce globals, initialize state, or migrate
objects.

In the interactive console the equivalent host-boundary workflow is:

```text
:live stage /path/to/revision.birnam
:live activate /path/to/revision.birnam
:live rollback
:live save /tmp/worker.image
```

The first line of an exported revision is the base envelope. Everything after it
is the exact authored source:

```text
// birnam-live-base sha256:<64 lowercase hexadecimal digits>
```
