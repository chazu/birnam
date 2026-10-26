# Project guidance

Consult [glossary.md](glossary.md) before choosing terminology in explanations,
documentation, code comments, reviews, or commit messages. Use its preferred terms
and avoid the jargon listed there. In particular, distinguish vtable lookup from
method lookup, and describe the actual work being done. Preserve exact identifiers
and historical measurement keys when necessary, while explaining their meaning.

Read [Current Invariants](docs/misc/DESIGN.md#current-invariants) before changing
runtime architecture. Malleability is primary, and object payloads are opaque to
minimal Id. Do not introduce or rename a fixed payload-type catalog in allocation,
dispatch, or GC. Record how representation-specific work stays with its owner.

Before changing a core runtime path, apply the architectural-change rules in
[Current Invariants](docs/misc/DESIGN.md#current-invariants): state the contract,
show the before/after operation sequence, and identify new required mechanisms.
Preserve all invariants together. Oop representation and payload opacity are not
permission to add an application protocol. `apply` and its forwarding helpers are
not part of Id; do not introduce them or a renamed equivalent.

Treat user decisions and the authoritative invariants as the design authority.
An implementation report, existing code, a passing test suite, or an earlier
agent's interpretation is not authorization to change them. If the implementation
needs a new contract, stop the affected work and surface that specific conflict;
do not infer approval from a general instruction to continue.

For a core-path change, add or retain behavioral tests for forbidden alternatives
as well as expected results. Review the code against the contract separately from
reviewing whether its tests pass. An open invariant violation blocks further
migration that depends on that mechanism. Documentation of the violation and a
failing contract test may be committed; they must not be reported as a repaired or
qualified runtime.
