# Isolate process workers over Id

Bootstrap loads Isolate into the creating thread's Id domain. The wire-v1 codec,
process spawning, serial request IDs, blocking transport, bounded nonblocking
polling and cancellation envelopes retain their existing format and algorithms.
Only owned bytes cross processes; native transport state contains no Oops.

Language value conversion sends to Integer, Float, String, Vector, Map and Id's
Symbol owner. Every intermediate object is rooted, borrowed bytes are copied
before another send, and native dependency failures propagate. Transfer supports
nil, Booleans, Integers, Float bits, binary Strings, Symbols, Vectors and Maps.
Depth, node, container and frame bounds remain enforced. Capabilities, Blocks,
cycles, unsupported map keys and malformed input are refused.

`start:selector:image:` boots a fresh installed executable through `--worker`.
Host loads the saved image without evaluating stdlib or global main, rebases its
Module library directory, resolves the receiver and selector, and enters Isolate
through messages. Ready and boot failures
travel over the inherited socket, with 15 seconds allowed for the ready handshake.
Source/image stdout remains ordinary program
output. `request:` and `request:timeout:` preserve synchronous replies and failure
records. `submit:timeout:` admits one outstanding request; `pollRequest:` answers
nil while pending and a one-element Vector on success, including a nil result.
Timeout/disconnection never retries an operation whose execution is uncertain.

`startService:image:`, `cancelRequest:` and `pollServiceControl` preserve the
source-authored TaskService/TaskControl protocol. Cancellation admission is not a
remote acknowledgement; the eventual outcome records cancellation and cleanup.
No Rust task scheduler or cross-thread object execution is introduced.

Handle retirement only marks the process orphaned. `Isolate checkpoint` and
`idleFor:` reap orphans outside GC; Host shutdown terminates remaining workers.
Failure fields are traced by their owner. Handles and native process state remain
nontransferable and are rejected by Image's supported-family boundary.

Validation:

- `cargo test --offline --locked -p birnam-isolate` retains wire, bounds,
  fragmentation, nonblocking deadline and cancellation-control tests.
- `mise run native-messages` includes collecting value conversion, malformed
  input, Failure retention and exact native-failure propagation. A controlled
  wire peer also verifies that reply-construction failure terminates the handle.
- `python3 scripts/worker-test.py PREFIX` runs real installed image workers,
  including sync/async requests, structured errors, timeout/crash, Ready/Stopped,
  admission failure, orphan reaping and authored TaskService/Worker behavior.
  It is also run by `mise run host-cli`, which defaults to an optimized build for
  the large authored-service fixture; source compilation is slow in debug builds.

Host also loads the authored Concurrency helper at startup. It prepares a batch
through the resumable source compiler, adopts its initial generation, and captures
the worker image without attempting candidate activation inside a language call.
`scripts/concurrency-cli-test.py PREFIX` exercises this path, the retained 19-check
service fixture, source deletion before worker startup, and owned image cleanup.
It runs in the installed task acceptance group.
