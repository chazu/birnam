# Project guidance

Read [INVARIANTS.md](INVARIANTS.md) before architectural work and consult
[glossary.md](glossary.md) for terminology. [ARCHITECTURE.md](ARCHITECTURE.md)
maps the current implementation; [docs/README.md](docs/README.md) distinguishes
current references, proposals and history.

Birnam objects interact through messages. Native implementation helpers are
ordinary algorithm code, not permission to bypass another object's behavior or
introduce a private object system. Do not import Rust ownership, borrowing or
borrow checking into the language. Keep objects and code valid through GC,
callbacks and suspended execution as an implementation responsibility.

Keep Id minimal and its payloads opaque. Whole buffers are whole values, not a
reason for one send per byte. Do not add an implicit application protocol after
ordinary method lookup. Do not infer new services, registries, wrappers or data
copies from crate boundaries or old implementations.

For a core change, state the required behavior, show the before/after operation
sequence, and justify each added mechanism and relevant cost. Prefer the direct
implementation. Test behavior and prohibited alternatives; review architectural
compliance separately from whether tests pass. Tests of incidental internal
steps do not become language requirements by existing.

Explicit user decisions and the invariants govern the design. Archived documents,
migration logs and measurement receipts are evidence about their recorded scope,
not current requirements or work queues. Do not restore retired machinery from
them or promote a successful workaround into an invariant.

Proceed with routine implementation choices within the agreed behavior. If a
real language requirement must change, surface that specific conflict before
building dependent work. Do not invent a conflict from an implementation
preference, or hide a violation by weakening its tests or changing its wording.
