# Id native protocol v1

`id_protocol.h` is the independent C declaration of Birnam's native message
boundary: opaque Objects, Words, Bytes, method entries, Protocols and outcomes.
Upper module tables and the former IdApi are retired under
[Design 90](../../../docs/design/90-functional-modules.md).

`entry.c` checks the C field layout and call/return convention. Id's unit test loads
it and installs/invokes it through production Id dispatch without upper modules.
It checks all six operand kinds and scalar failure details, without supplying an
alternate Id implementation.

Current production checks:

```sh
mise run siphon
mise run native-messages
```

The first exercises the real Siphon-to-Id handoff, interning/loading and invalid
paths. The second builds and audits the completed prefix's dylibs, then exercises
Bootstrap, Namespace, Roots, GC, object behavior, values and collections through
messages. It checks live replacement, nested loading, temporary lifetimes,
activation, retirement and finalization. It also covers Execution, the compiler
and Image; installed Host acceptance runs through `scripts/host-cli-test.sh`.

The installed native-adoption driver and cached floor-function lookup fixture are
deleted. Their former descriptions are available in this file at `8735496a`.
Other retained native fixtures with old construction/header assumptions are
migration evidence, not runnable acceptance tests. No factory table, saved service
entry or forward-filled dependency bundle is part of this protocol.

The independent C root enumerator in `entry.c` receives an Object and sends
`visit:` through the real Id carrier. The Id test replaces that visitor method,
checks exact failure propagation and rejects an old NativeEntry visitor operand.
