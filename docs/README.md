# Documentation

Start with [INVARIANTS.md](../INVARIANTS.md) for language requirements and
[ARCHITECTURE.md](../ARCHITECTURE.md) for the current implementation map.
Use the [glossary](../glossary.md) for terminology. Neither Rust terminology nor
historical implementation arrangements add language requirements.

## Current behavior and operation

- [Language](guide/01-language.md) and [idioms](guide/02-idioms.md).
- [Runtime behavior](contracts/runtime.md): Methods, edits, execution and images.
- [Bulk byte messages](contracts/bytes.md).
- [Images](guide/04-images.md), [compiled units](guide/05-compiled-units.md),
  [concurrency](guide/07-concurrency.md), and [shipping artifacts](guide/09-shipping-artifacts.md).
- [Validation](validation.md) and [benchmark operation](misc/BENCHMARKS.md).

Crate READMEs describe the current implementation of particular facilities.
They do not establish language-level ownership or borrowing rules.

## Detailed references and proposals

The [design index](design/README.md) distinguishes message/format references,
proposals and implementation notes. An accepted message or byte-format contract
can constrain compatibility without prescribing Rust types or service boundaries.
Proposals are not instructions to implement everything they describe.

## History and evidence

[Archived documents](archive/README.md) contain earlier designs, rejected
approaches, completed migrations and dated investigations. Their claims of
"current," "required," or "remaining" refer to the recorded period. They are not
current architectural authority or an active work queue.

[Measurements](measurements/README.md) retain their original revisions, results
and limitations. Passing tests, measured speedups and implementation reports do
not make the measured architecture an invariant.
