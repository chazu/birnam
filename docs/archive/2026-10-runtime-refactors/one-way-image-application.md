# One-way image application

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-18. Implements the next reduction identified by the
[source-registry boundary review](source-registry-id-boundary.md), on top of
`b893ccee`. Explicit candidate compensation and user-requested undo are unchanged.

## Contract and implementation

Validate and stage an incoming image, invalidate old process-local capabilities
when application begins, then apply it. On failure, retain applied effects and
report partial application. Recovery remains the user's saved image and an
explicit fresh-runtime reload, as selected in the
[basic live-load decision](../../design/optional-store-and-relational.md#selected-live-load-failure-contract-2026-10-16).

Generation no longer holds a recoverable copy of candidate handles, prepared
operations or certification observations. `beginImageApplication` drops those
roots and sets an application flag. Candidate operations remain unavailable while
that flag is set. `finishImageApplication` releases an unused source stage and
clears the flag; it never restores old capabilities. Before application, cleanup
does not clear existing candidates, prepared operations or certification.

The native begin operation resolves its reply and checks owner-local exclusion
before invalidating state. It performs no outgoing send after invalidation.
Execution/TCC still retain implementations owned by entered frames, captured
Methods/Blocks and other legitimate users. Dropping a candidate handle does not
directly unmap executable memory. No shared native structures or private calls
cross module boundaries; Codegen and Image use current Id messages.

The former composition-digest recovery handshake, success recovery, cancellation
restore and failure abandonment operations have been removed. Image still checks
its format, code/inventory consistency and checksums and records composition
identity on successful application. Removing the duplicate Generation handshake
does not remove artifact validation.

`prepareSourceRegistry:` validates and roots incoming entries.
`installSourceRegistry` consumes them and replaces the current registry. It keeps
no predecessor and has no inverse operation. Its Codegen facade is now
`installPortableImageSources`; obsolete swap/rollback selectors are removed,
without compatibility adapters. The serialized `sourceRegistryV1` representation
is unchanged. Build/install the cooperating native modules together.

An explicit installation may supersede a source edit made after staging, just as
other later image writes may supersede earlier values. It no longer promises
conditional exchange against a staging revision. Cleanup leaves installed sources
as they are. The registry's existing mutation/content counters still serve other
operations; their independent simplification is not part of this change.

## Failure boundary and malleability

The loader's partial-application boundary includes the
`beginPortableImageApplication` send itself. A replaced begin method can invoke
the default invalidation and then fail, including after GC. Reporting that case
as an untouched admission rejection would be misleading. Application-attempt
failure reports `#partialLoad`, or `#startupFailed` for fresh startup, and clears
the loader's composition identity. An early staging rejection retains the
ordinary rejection behavior. Replaceable staging callbacks are still not promised
side-effect-free.

This does not make arbitrary replacement methods transactional. Cleanup uses
ordinary sends and retains the existing failure limits of that protocol. There
are no method-identity guards, replayed callbacks, mandatory world certification,
new locks or automatic recovery snapshots. A user can deliberately replace these
methods; the default implementation provides the documented behavior.

## Qualification

Qualification passes against `/tmp/birnam-image-simplification/prefix`:

- Fresh-prefix installation build; 826 language assertions, zero failures.
- All ten checks in the Host generation group, including the new boundary test,
  mapped/rebuilt/resaved/divergent images, console/TCP and 62 compiled-unit checks.
- Native candidate retirement, ledger and transaction suites; image roundtrip,
  20 successive save/load cycles with stable retention, callback-failure cases,
  conditional owner operations and snapshot admission.
- Installed startup with parsing and compilation disabled, checked in an isolated
  copy so the test's temporary file removal does not affect concurrent tests.

The [receipt](../../measurements/one-way-image-application-20261018.json) records
commands, logs, source hashes and test-binary provenance. Production runtime
sources match the built prefix; the final Host fixture was rebuilt separately
after correcting its candidate-reopening assumption and a test predicate.

The new Host regression covers a collecting staging failure with a
still-usable prepared rollback, immediate invalidation, cleanup without revival,
retained Method execution, consumed source staging and one-way installation over
an intervening source write. The existing mapped/rebuilt/resaved generation-image
matrix now injects both ordinary Error and native failure after begin invalidates
capabilities; it checks partial-load reporting and explicit subsequent reload.

Lean and TLA+ models distinguish pre-application cleanup from invalidation,
independent code ownership, source installation and cleanup. Negative controls
cover late invalidation, retained recovery roots, capability revival, premature
code release and source undo. These are abstract contract checks, not proofs of
native implementation or arbitrary callback behavior.

Formal qualification passes 381 Lean theorems, 118 rejected semantic mutations and
255 expected TLC outcomes. Receipt:
`.cargo-target/formal/results/rooting-na3cc4h4/summary.json`; its input hashes match.
The affected native libraries retain their declared exported symbols and have no
new direct inter-module dynamic dependencies. Production Rust is reduced by 164
physical lines across Generation, Codegen runtime and Image; this is a scope
measurement, not a CPU-performance claim.

No source-registry owner port, Store-absent core bootstrap, release, user-prefix
installation or CPU performance improvement is claimed by this slice.
This slice and the preceding boundary-design documents are committed together.
No push accompanies the commit. Source-registry ownership and its callback/bootstrap
gates remain the next migration work; candidate compensation remains a separate
feature decision, not an image-loading requirement.
