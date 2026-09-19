# 81L — Production Integer extraction

> **Status:** In progress. Follows the owner/parent protocol checkpoint
> `203238a8`. Integer is first; environment, other primitive values,
> reflection and storage/collection follow in separate committed extractions.

## Scope and sequence

The isolated 81G module establishes a boundary, but its raw operands and native
error statuses are not the production language contract. Extract the existing
production implementation, preserving its observable behavior.

The first stage moves Integer's thirteen native methods and its optional numeric
implementation declaration from Prelude to `libbirnam_integer.dylib`, loaded
immediately after Id. The module depends only on `birnam-abi`. Prelude no longer
installs or supplies those methods. Id's current representation, constructors and
four Integer API fields remain in place at this intermediate checkpoint.
**This stage alone does not complete the Integer extraction.**

The second stage must move representation, construction and primitive access
above Id, migrate their native consumers and reduce IdApi. Audit bootstrap and
pure representation consumers before selecting that boundary. In particular,
Machine operands, recovery preparation, deferred allocations and native numeric
guards currently consume Integer inside the still-combined runtime. Do not
replace a callback-free read under a state borrow with an allocating send, and
do not bypass replaceable construction in an optimization.

## Invariants and review

- Ordinary lookup selects the current Integer method. Moving code must not
  seal arithmetic or cause Prelude initialization to overwrite its replacement.
- Equal results retain fresh identities. Preserve operand evaluation, Error
  propagation, mixed Float operations, overflow and division behavior, and the
  existing comparison implementation; numerical behavior changes are separate.
- Native numeric declarations move with their actual method image. Existing
  image pinning and copied/wrapped API checks remain authoritative. No fallback
  declaration remains in Prelude.
- The existing native-module lifetime, collector, roots, finalizers and code
  retirement contracts remain in force. No new ownership/unloading system is
  a prerequisite for this extraction.
- Stage zero, installed stage one, native fixtures and recovery builds must all
  load the same required library. A source-file move without installed loading
  is incomplete.
- The first stage leaves IdApi v3, Method, Frame and image bytes unchanged.
  Later changes to those boundaries need their own explicit migration and
  independent fixtures.

The initial adversarial review identifies three likely false positives: testing
only after Prelude has loaded; leaving the numeric declaration in the old image;
and calling the method-only split a completed extraction of Integer from Id.
The independent C fixture tests Integer before Prelude, uses an `__int128`
arithmetic oracle, and verifies that later Prelude initialization preserves a
replacement method. The copied-API fixture targets the actual Integer image.

## Acceptance

Run the workspace checks, installed native fixture, numeric and recovery
matrices, full release plan and relocated installation smoke. Required module
inventory is 17 native libraries plus the existing 28 authored modules. Source
and artifact receipts belong under `.cargo-target/validation-81l/`.

The method-library checkpoint passed all 610 workspace tests (28 ignored), the
independent 405-case C oracle, copied-API checks, the full installed native gate,
and relocated installation smoke. The native gate passed in a serial rerun;
its worker ready deadline had timed out under concurrent matrix load, including
when reproduced with the previously accepted 81K release.

Completed matrix receipts remain under `.cargo-target/validation-81l/`. The
intermediate exhaustive run was superseded so the complete release gate can run
against the final Integer boundary. This checkpoint does not claim full release
acceptance or completion of the representation extraction. No performance claim
is made.
