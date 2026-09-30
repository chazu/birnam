# Trace diagnostics

Bootstrap publishes `Trace`. Its native message `report: visitor acceptingGaps: mask`
(Object, Word) invokes the current `Storage observe:` method and traverses the
validated graph. It sends `reached:identity:` or `notReached:identity:` (Object,
Word) once per allocation, ordered by allocation identity, then `complete:` with
the unchanged Word coverage gaps. The completion result propagates to the caller.

A mask of zero is strict. Any unaccepted gap, including an unknown future bit,
fails with native INVALID_REQUEST detail `3` before report callbacks run. Current
Storage always reports unobserved native stack roots (bit `1`); callers must
explicitly accept that limitation. Missing reference owners add bit `2`.

All graph endpoints and allocation identities are validated, including edges from
unreached objects. Duplicate roots/edges, cycles and empty graphs are valid.
Enumeration does not make an allocation a root. `notReached` means only absent
from this declared graph; it never authorizes collection or finalization.

Reporting runs within Storage's rooted observation. Retained callbacks fail closed;
nested reports fail with detail `2`. Provider/report failures clean up the scoped
state and propagate. No borrowed state crosses an outgoing message.

`cargo test -p birnam-trace --offline --locked` checks all 4,096 three-vertex graph
and root combinations against transitive closure. `bash scripts/native-message-test.sh`
checks the actual dylibs, live method/owner replacement, coverage, callback lifetime,
GC during observation, failure cleanup and post-observation reclamation.
