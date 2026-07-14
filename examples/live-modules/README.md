# Live-module examples

This example exports an exact file-backed revision, stages and inspects an edit,
activates it through the L1a intent, saves a full image baseline, boots that
baseline through both mapped and rebuilt recovery, starts an isolate from it,
then performs an L2 worker-local rollout and rollback between requests.

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

`rollout.input` is fed to a coordinator booted from the v1 baseline. It stages v2
locally only to produce the fixed copied-value request, then calls
`worker evolve:request timeout:`. The worker independently compares its current
base, opens the checksum-addressed artifact, prepares and commits under
quiescence, and returns a copied `EvolutionReceipt`. The next ordinary request
observes v2. A rollback request names the accepted current digest and the next
ordinary request observes v1 again. Rollback capability is process-local and is
not preserved by saving and booting another image.

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
