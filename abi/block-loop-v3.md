# Block Loop installation v3

Design [81BE](../docs/design/81be-production-namespace-import.md) adds the
execution API's explicit [import capability](namespace-import-v1.md) to the
[v2 installer](block-loop-v2.md). The method bodies, receiver/argument handling, callback order, roots and
image encoding retain those contracts. Design 83 moves installation from Prelude
to Block Loop initialization, after Runtime has published `Block` and `BlockVt`. Error Value uses its [v3 ABI](error-value-v3.md).

The production `module_init` selects the existing Block behavior and installs
`whileTrue:` and `whileFalse:` immediately, with Error Value already supplied.
Prelude neither creates Block nor reinstalls these methods. Native consumers can
still obtain `birnam_block_loop_provider_v3` and attach it through
`birnam_block_loop_dependency_v3`. The immutable descriptor remains 24 bytes:
version 3 at offset 0, byte size at 8, install at 16. All fields are required;
malformed descriptors and retargeting reject, and repeated attachment of the
same descriptor succeeds. V2 is historical, with no production adapter.
The [independent C mirror](fixtures/block-loop-v3/block_loop_v3.h) defines the signature.

Install receives registration API, execution API, execution presence, execution
imports, then the existing behavior and selector operands. Capture the execution
API and both live descriptors at the original first-install point before any
registration callback. Incoming registration can reenter with a different API;
subsequent installation retains the first execution selection. Registration uses
the supplied registration API and does not reintern supplied selectors.

Each import reads the current function/context together at its original call
point; earlier callbacks can change either, and the returned Oop remains a
snapshot. Caller-owned descriptors, context and code must outlive all retained
methods, alongside their matching API. No API-address registry or callback-slot
loan is used in production.

See the [native export inventory](rust-block-loop-dylib-v1.json). Use
`scripts/id-native-loading-smoke.py PREFIX` for the installed native value/control
prefix and `scripts/block-loop-image-smoke.sh PREFIX` for
startup/source-free/saved-image/baked execution. The older `mise run block-loop`
C driver still assumes aggregate `id_init` construction and Prelude installation;
it requires migration to the production Id/Runtime boundary.
