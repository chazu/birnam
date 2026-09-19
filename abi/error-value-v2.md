# Lazy Error value policy v2

Design [81BA](../docs/design/81ba-production-error-presence-handoff.md) separates
the caller's presence capability from the remaining IdApi access. Compared with
[v1](error-value-v1.md), both operations gain a borrowed
`const NamespacePresenceV1 *` immediately after `const IdApi *`.
The [C mirror](fixtures/error-value-v2/error_value_v2.h) specifies the signatures.

The immutable provider remains 32 bytes, now with version 2. Hosts obtain
`birnam_error_value_provider_v2` and attach
`birnam_error_value_dependency_v2`; all current native consumers use v2, with no
v1 export or fallback. Admission still rejects missing operations, wrong
version/extent, null descriptors and retargeting. Initialization remains
callback-free and creates no objects, roots, methods or layouts.

Presence is selected per invocation, with no cached global or API-address map.
Recognition first short-circuits null values without accessing the capability
(a null capability pointer is permitted on that path). It then checks ErrorVt
through the live capability, imports the current behavior through the selected
IdApi, and compares that snapshot with the subsequent vtableOf result.

Construction reconstructs the same borrowed kind/message slices, interns kind,
and invokes the current String constructor before reading presence. It then checks
Error, imports its factory, interns kind:message: and sends to that imported
factory snapshot. An absent/null factory returns the selected API's current nil.
All v1 callback order, reentry, GC, argument/result and borrowed-buffer contracts
remain unchanged. Neither operation reads IdApi.isExported or retains presence.

The Rust call-site adapters currently lend each caller's live isExported slot;
they do not snapshot its callback. IdApi v8, numeric admission identities, module
initializers, Error representation, language protocols and image encoding are
unchanged. The remaining direct consumers and slot removal are later work.
