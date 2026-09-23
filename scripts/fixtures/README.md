# Validation fixtures during Design 90

The current converted-prefix checks live in `rust/birnam-siphon/tests/` and run via
`mise run siphon` and `mise run native-messages`. The independent C message-carrier
fixture remains in `abi/fixtures/id-protocol-v1/entry.c` and runs in Id unit tests.

The retired `id-native-loading-smoke.py` driver used the former installed
construction/adoption graph. Native fixtures here that refer to removed module
headers or `IdApi` retain behavioral cases for later message-based setup; they
are not current passing checks. Consult their owner protocols and Design 90 rather
than restoring those headers or creating another construction graph.

Keep the execution, suspended-frame, recovery, deoptimization, image and concurrency
semantic probes. Their algorithms and acceptance obligations survive the interface
migration. Only the cache-specific `floor-lookup.c` fixture was deleted in this
cleanup; its assertions required retaining the removed service-call caches.
