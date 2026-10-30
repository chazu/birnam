# Source registry: Id-only ownership design check

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

**Candidate follow-up (2026-10-18):** Native preparation/transaction files referenced
in this historical analysis have been removed. Links now point to the
[authored candidate policy](candidate-policy-migration.md); automatic compensation,
reservation and setter-postcheck promises below are superseded by explicit undo.

2026-10-18. Source inspection of `b893ccee`. This is a design check, not an
implemented migration or a runtime qualification receipt.

Implementation follow-up: [authored source registry](../../design/authored-source-registry.md)
records the owner cutover, candidate send/compensation boundary, early bootstrap
and direct image restoration. The findings below remain the original design check.

Follow-up implementation: [one-way image application](one-way-image-application.md)
removes recoverable capability backups and image source rollback before the owner
migration. At that intermediate stage the candidate callback seam and early
authored-owner bootstrap gates remained open; the authored-owner report now
records their implementation. The inventory below describes the reviewed baseline.

## Decision and readiness

Move the source registry to one ordinary, discoverable Birnam owner. All
inter-module interaction must use current messages through Id. No requirement
found in this audit calls for shared native state, a private ABI, a direct call
into another module's implementation, or a permanent native registry service.

The ownership boundary passes this design check. A mechanical translation does
not: candidate publication currently relies on a callback-free final registry
write, and bootstrap needs registry lookup before activating named modules.
Resolve and qualify these two seams before deleting the native owner. No code
change, bootstrap success, formal proof or performance result is claimed here.

This continues the [optional Store decision](../../design/optional-store-and-relational.md)
and follows the resolved [generation review](generation-post-simplification-review.md).
Registry data already lives outside Store; this slice moves native authority
into the language rather than merely removing another fact lookup.

## Current ownership and consumers

`s.registry` is a field of Generation's private Rust `State`. Its
[implementation](https://github.com/chazu/birnam/blob/b893ccee/rust/birnam-generation/src/registry.rs) contains ordered
`(module-name Symbol, compilation-record)` pairs, two counters and one image
stage. The records are already ordinary Vectors carrying original source and
retained metadata. Native GC visitation roots both current and staged entries.

Direct access within Generation is not itself cross-module interoperability.
It becomes a boundary problem if consumers retain that access after the registry
is moved to a separate owner. A Birnam facade over the existing table would not
complete the migration.

| Consumer | Current access and responsibility | Required destination |
| --- | --- | --- |
| Codegen registration, replacement and source lookup | Id sends to Generation via `publication.rs`; String-to-record lookup also resolves Generation directly in `compiled_source.rs` | Resolve the ordinary owner and send its protocol; preserve Codegen's useful compiler-facing operations without a duplicate registry |
| Candidate compile and `activeFor:` | Direct `registry.record(name)` in `candidate.rs` | Message-based lookup of the current compilation record; native candidate handles remain Generation-owned |
| Candidate preparation and observation | Direct record checks in `candidate_preparation.rs` | Observe only the selected module through the owner; keep unrelated module edits admissible |
| Candidate activation and undo | Direct `replace_record(name, expected, next)` at the end of `candidate_transaction.rs` | Include conditional source replacement in the callback-capable explicit operation and its failure handling |
| Explicit certification and image-save admission | Copies of entries and content-revision checks in `candidate_ledger.rs` | Owned observation values obtained by messages; no mandatory certification of ordinary sends |
| Image source restore | Image sends Codegen prepare/swap messages, which send Generation registry messages | Validate staged ordinary data, then explicitly install it through its owner; preserve partial-application reporting |
| Recovery cleanup and GC | Generation clears the stage directly and visits native registry roots | Operation-local staging and ordinary object references; each native consumer roots only values it retains |

The cross-module routes above already use Id. Generation's use of shared pure
codec/analysis Rust code is separate from access to a live runtime owner; this
check does not claim all compiler code has been ported or audit every module.

## Proposed public boundary

Use an ordinary object, provisionally `SourceRegistry`, held in Globals. Give it
discoverable slots and ordinary inspection and mutation methods, following the
[Module owner precedent](module-owned-registry.md). Keep it separate from Module:
module loading bookkeeping and retained compilation records have different
lifetimes and responsibilities. This is one authoritative owner, not a mirror.

Native clients currently use Namespace-only `named` helpers. A new Globals-held
owner requires explicit `Globals at:` resolution through Id. Do not add a second
Namespace binding, a hidden pointer cache or a native singleton containing its
state. Resolve the current binding for a new operation. An explicit multi-step
operation can retain the receiver it selected; its compensation must address
that receiver rather than accidentally modifying a replacement owner. This is
operation-local ownership, not freezing method implementations or banning rebinding.

The protocol needs these capabilities; exact selector spelling remains an
implementation detail to settle with its tests:

- Inspect ordered entries, find a record by module name, and find a compilation
  by original source identity. Return ordinary objects, never native references.
- Register a prepared record and explicitly replace a selected record. Keep
  source validation with Codegen's retained-metadata protocol; do not reparse
  during compiled startup or add a registry-owned compiler.
- Perform conditional replacement for an explicitly requested candidate edit:
  replace this module's expected record with the supplied record, or report a
  conflict. An unconditional reflective edit remains available separately.
- Describe/export the registry for images and explicitly install validated
  incoming entries. No implicit certification, code installation or fact writes.

Persistent ordered values preserve iteration order and source/record references.
Equal source text must not merge distinct source objects. Current registration
also distinguishes duplicate equal bytes from conflicting text; preserve and test
that behavior separately from source-identity lookup. Do not casually substitute
a value-keyed String Map. A clone may share the persistent entries initially but
has its own owner slot; inspectable snapshots are not deep-frozen records.

All methods, collection operations and accessors remain replaceable. Native
clients may validate message results needed to decode data safely. They may not
inspect the owner's slots directly, compare method identities to admit an
operation, invoke saved native entries, or bypass current lookup after an error.
No registry-specific addition to Id is proposed. Existing ObjectModel slot and
conditional-slot messages are available to the authored owner through Id.

## Gate 1: candidate commit has a newly fallible boundary

The [current commit](../../../modules/candidate-policy.birnam)
finishes callback-capable writes, checks and compensation handling, then calls
`registry.replace_record` and installs native candidate ownership without further
outgoing messages. Turning only that call into an Id send is insufficient: the
new send can collect, reenter, mutate other state, or return an Error after a write.

Treat source replacement as one of the explicit operation's attempted writes.
Retain the selected registry receiver, old/new records, Methods and inverse state
before calling out. Define selected-state postconditions and conditional inverse
handling for the source write as well as the existing binding/field writes.
Keep unrelated callback changes. On an affected intervening edit, report failure
or failed compensation honestly; do not restore a whole-registry snapshot.

The default owner should publish an edited persistent value using the existing
conditional-slot protocol and appropriate existing task-transition policy. A
task transition does not prevent synchronous callback reentry, and conditional
slot replacement does not make a whole cross-owner operation atomic. Do not add
a global lock, replay arbitrary callbacks, or manufacture a privileged no-callback
entry point to recover the old Rust shortcut.

Before implementation is called ready, specify the exact write/postcheck/inverse
ordering and test: failure before publication; write then Error; selected-record
conflict; unrelated registration during a callback; owner rebinding; collection;
and failure while compensating. Existing conditional-compensation limits remain.
An arbitrary replacement can violate its own operation contract; the language
does not promise to make such replacement effects transactional.

Ordinary method edits must remain independent of this protocol. Future sends
use current dispatch; entered Frames, Blocks and other legitimate owners retain
their executable implementations through existing Execution/TCC ownership.

## Gate 2: install the owner before named artifact admission

[Bootstrap](../../../rust/birnam-bootstrap/src/lib.rs) loads native Codegen and
Generation before authored Error, Cell and Module. The
[artifact loader](../../../rust/birnam-codegen-runtime/src/artifact.rs) looks up a
named module's old source **before** running its initializer and registers its
compilation afterward. Therefore a normally declared registry module cannot
solve its own first-load dependency merely by creating the owner in its initializer.

The recommended experiment is an early authored installation unit, like Cell,
without a named-module declaration requiring prior registry admission. Install it
before `modules` and before the first named artifact lookup. Use existing early
ObjectModel/Globals/Reflect/collection protocols; do not depend on stdlib or the
task executor being installed. Keep discovery of the owner independent of whether
its installation source has a named-module declaration.

The clean build has an additional path: the `lang-seed` bootstrap skips the normal
authored module loop, and the installer compiles early artifacts in that mode.
Wire the owner into both paths. First establish whether compiling/loading its
installation unit needs anything beyond an explicit absent-owner lookup during
seed compilation. Prefer a bounded build-only treatment of that absence over
another native registry. Missing ownership during normal operation must fail,
not silently select a native fallback or drop registration.

This is a source-based route, not proof that all early sends are available. Prove
it with an empty-prefix build from checkout and an installed production build
with the displaced registry implementation absent. If a seed remains necessary,
record its exact early consumers and exclusion from production. Do not retain a
second production implementation under a bootstrap label.

## Simplifications to pursue, with limits

1. **Remove unused image rollback.** Repository search finds the exported
   `rollbackPortableImageSources` forwarding to `rollbackSourceRegistry`, but no
   loader caller. The current loader explicitly permits partial application.
   Replace prepare/swap/predecessor retention with validated staging and explicit
   installation; retain pre-application validation and temporary roots. Remove
   obsolete rollback selectors with their displaced implementation. No backwards
   compatibility adapter is required. Keep basic live loading free of automatic undo.
2. **Reassess both counters independently.** `revision` protects registry-local
   operations and staged swaps; `contents_revision` supports certification and
   image observations. Ordinary candidates already use the selected record's
   identity. Removing swap/rollback may remove the first counter's need. Do not
   assume comparing a snapshot is equivalent to a content-change stamp: changing
   A to B and back to A can be observably different today. Specify that case
   before eliminating the second counter; an ordinary owner-held stamp is an
   acceptable interim representation, not a guard on method replacement.
3. **Remove native ownership, not just its storage.** Delete the registry field,
   private mutation/read helpers and registry GC visitation after all consumers
   use messages. A small stateless Id-message helper inside a consumer is fine;
   shared backing tables, mandatory mirrors and new generic transaction services
   are not substitutes.

Image installation must also avoid two competing restorations: a Globals-held
owner can be part of the restored heap while the image carries a source inventory.
Define which restored object receives that inventory and when Globals makes it
current. Use the inventory as transport/admission data, not a second live authority.
Preserve shared identities, retained descriptions, source-free mapping and the
explicit rebuild path. Version/rebuild a changed format rather than silently
interpreting old data. Existing image-stage cleanup sends and their failure
behavior must be included when changing that path.

## Qualification and next slice

First implement a narrow protocol/consumer seam with focused candidate failure
coverage; it must not add a new native runtime module or permanent fallback.
Then install the authored owner in both bootstrap paths and delete the displaced
table, adjusting image staging in a separate reviewable slice where possible.
These are proposed slices, not completed work.

Acceptance must include:

- Id-only runtime boundaries and no native knowledge of the owner's slot layout;
  replacement lookup/accessor/mutation methods are actually exercised.
- Source identity, order, duplicate registration, absent/nil/error results,
  reflective edits, clone sharing, reentry and GC; explicit Store facts untouched.
- Candidate activation/undo/discard and certification release, including A/B/C
  chains and entered, captured and suspended/resumed executable ownership.
- Mapped/rebuilt/resaved images and partial-load failure; startup with parser and
  compiler disabled; malformed retained metadata rejected without repair.
- Empty-prefix cold bootstrap and the affected installed language/Host/native
  gates. Do not infer Store/Relational-absent core acceptance from this slice.
- Lean laws and TLA+ cases for selected source replacement, conditional inverse,
  callback interference and stage release, with negative controls. Those models
  supplement production rooting/lifetime tests; an Id-only interface alone is
  not a proof of implementation correctness.

No performance improvement is presumed. Measure registration/lookup and affected
startup/candidate paths, especially if ordered lookup adds sends. Any later
optimization must preserve current dispatch and deoptimization. This work does
not resume the Relational port or make the current Rust floor permanent.
