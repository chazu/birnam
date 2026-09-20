# Environment installation v3

Design [81BE](../docs/design/81be-production-namespace-import.md) adds the
execution API's explicit [import capability](namespace-import-v1.md) to the
[v2 installer](environment-v2.md). The method bodies, registration positions,
receiver/argument handling, callback order, roots and image encoding retain
those contracts. Error Value uses its [v3 ABI](error-value-v3.md).

Obtain `birnam_environment_provider_v3` and attach it through
`birnam_environment_dependency_v3`. The immutable descriptor remains 24 bytes:
version 3 at offset 0, byte size at 8, install at 16. All fields are required;
malformed descriptors and retargeting reject, and repeated attachment of the
same descriptor succeeds. V2 is historical, with no production adapter.
The [independent C mirror](fixtures/environment-v3/environment_v3.h) defines the signature.

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

See the [native export inventory](rust-environment-dylib-v1.json). Use
`mise run environment` and `scripts/environment-image-smoke.sh PREFIX` for focused debug
and startup/source-free/saved-image/baked execution checks.
