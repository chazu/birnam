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
