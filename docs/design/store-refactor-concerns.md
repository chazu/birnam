# Store refactor: concerns, experiments and proposed solutions

2026-10-15. Investigation against `70d9cf14` and the qualified supplied-template
prefix. **No production runtime or language semantics changed.** The accompanying
fixtures are experiments, not an installed publication owner. This report updates
the sequencing in [the Store decision](optional-store-and-relational.md).

The later [coordination cleanup](coordination-cleanup.md) fixes the reproduced
callback-write loss. Its conditional recovery reports conflicts and can leave a
partial live world; the conservative owner boundaries are documented there.
This document retains the original investigation and proposed alternatives.

## Current disposition (2026-10-16)

This is a historical investigation of `70d9cf14`, not the active implementation
plan. The callback-write loss is fixed by `bd8a532e`; the original negative probe
now has preservation regressions. The universal publication/result pipeline and
per-edit description hook proposed here were superseded by
[minimal mutation](minimal-mutation-and-coordination.md). Ordinary edits now use
actual dictionary owners; Generation explicitly coordinates its own operations.
Do not implement the old publisher or repeat the old defect expectation from this
report. Memory measurements below describe the recorded baseline, not today's RSS.
Use the [current plan and TODOs](optional-store-and-relational.md#actionable-migration-safeguards)
for remaining work. The later [basic live-loading decision](basic-live-image-loading.md)
selects no automatic compensation; stronger recovery is an optional future feature.

## Historical findings and proposed priorities

| Concern | Evidence | Recommended solution |
| --- | --- | --- |
| Image recovery erases intervening work | Reproduced in mapped and rebuilt loads: a failing publication callback's later method edit, unrelated Global and explicit fact all disappear. Also reproduced against installed 0.17.0. | Correct Image compensation before further publication/storage migration. Use conditional, owner-specific inverses and report conflicts; do not restore whole snapshots over callback work. |
| Publication depends on the native failure channel | Existing synchronous/image mismatch reproduced. An ordinary Birnam prototype passes 27 cases across fresh, mapped and rebuilt policy, including callback-local handling. | Specify publication in ordinary results. Required hooks return success or Error; check their results explicitly. Use the normal resumable construction path. Do not preserve two conventions or use ambient failure preservation. |
| Optional descriptions could remain a hidden dependency | Prototype's default path invokes no description action and leaves Store basis unchanged. Explicit action runs between dictionary commit and managed notification. | Keep snapshots first. Pass an optional description action per operation; no default Store resolution, implicit mirror or mandatory subscription. |
| Startup memory remains high | Sampled peak about 1.3 GiB; much lower after startup/GC. Images contain 200/292 MiB code bundles, mostly distinct body variants, not identical adapters. | Target large validation copies and buffer lifetimes first; then derive required body variants accurately. Exact bundle deduplication helps, but accounts for only about 7% of these images. |

## 1. Image compensation: a reproduced correctness defect

The new `image_recovery_conflict` probe saves a source-backed method, changes the
resident method, then intercepts the current
`Codegen installPortableImageBinding:on:as:source:` send. The callback:

1. Runs the actual installer, so the image's dictionary edit commits.
2. Replaces that same live dictionary binding with an independent native Method.
3. Writes an unrelated Global and asserts an explicit user fact.
4. Forces precise collection and returns native failure `(881, 73)`.

Image returns that failure, restores the pre-load method and loses both independent
writes. Both mapped and rebuilt modes reproduce it. The installed 0.17.0 runtime
also reproduces all three losses: this is **not introduced by the latest template
sharing or the recent Store-removal slices**.

The implementation explains the observation. Image's failure path unconditionally
restores saved entries, exchanges the whole Store state back, and restores a Globals
checkpoint. These operations recover an earlier snapshot, but cannot distinguish
image-owned writes from later callback writes. Existing successful roundtrip and
repeated-resave tests did not establish that distinction.

Generation already has a better pattern in `candidate_transaction.rs`: inverse
records include before/after values; it recognizes already-restored entries and
refuses to overwrite a third value. Its existing transaction and ledger tests pass.
That is a starting point, not proof that every Generation compare/write window is
safe under arbitrary reentry.

### Proposed recovery contract

A recovery participant must provide an inverse capability containing its exact
owner/target identity, prior presence/value, planned installed value and any needed
write identity/version. Capture this **before** calling the mutator. Reading the
current value after a callback returns does not establish which write was ours.

For an inverse:

- If the original state is already restored, finish without replaying a callback.
- If the state is still the operation's expected write, restore its predecessor.
- If another write intervened, preserve it and report `#compensationConflict`.
- Keep attempting independent inverses where their contracts permit it. Do not
  report a complete rollback when a participant remains conflicted.

The compare-and-restore must be one operation of the owner that actually holds
the data. A source-level read followed by an independently replaceable write has
an intervening-callback window. These are **data ownership checks**, not method
identity checks or restrictions on monkeypatching. Id must remain the dictionary
floor; it need not know Store, Generation or Error policy.

For methods, Image needs the prepared Method/entry identity before installation,
including removal's expected absence and the target's vtable association. The
current installation protocol does not supply a complete conditional inverse
capability. Expose that through the actual preparation/publication owner rather
than infer it from the dictionary after notification. A conditional Id dictionary
operation may be necessary; its exact selector is not implemented or selected here.

Keep inverse records in inspectable owner state; do not introduce a permanent
opaque Rust transaction registry.

For Store, Globals, source registry and foreign wrappers, replace unconditional
checkpoint restoration with owner-specific publication journals. Detached staging
is still useful; publishing its state must distinguish imported writes from later
ordinary writes. Preserve reservations for inverse storage, borrowed-value copying,
root lifetimes and process-local hook installation. A conflict may leave a partially
compensated world: expose that honestly and invalidate affected preparation/adoption
state rather than claiming that the original world was restored.

This is coordinated work, not a safe one-line condition around the existing
method restoration loop. The Global/fact reproductions demonstrate why fixing only
the dictionary loop would be incomplete.

### Acceptance before migration resumes

Turn the current negative probe into a positive regression. Add independent and
same-target writes, absence/bound-nil, removal, vtable reassociation, repeated writes,
source registry and foreign-wrapper cases, callback collection, failed inverses and
reserved-capacity exhaustion. Check both image modes and exact retention after
conflict. The characterization test currently asserts the observed defect;
`BIRNAM_EXPECT_IMAGE_CONFLICT_PRESERVED=1` deliberately makes it fail against today's
runtime. A passing characterization is not a fixed recovery guarantee.

## 2. Ordinary publication results: a viable authored solution

The known adapter mismatch remains: the explicit synchronous artifact returns a
native failure, while its mapped/rebuilt binding returns an Error. The earlier
native oracle describes that historical behavior; it need not dictate the new
publication API because backwards compatibility was explicitly waived.

Recommended publication rules:

| Stage | Ordinary result contract |
| --- | --- |
| Authorization | Exact true admits. Any other value, including Error, is returned before mutation. |
| Dictionary mutation | An Error is returned; never assume an error implies no edit committed. |
| Explicit description action | Runs only if supplied, after dictionary success and before managed observation. Error stops further work but does not undo the committed edit. |
| Required managed observation | An Error is a reported failure after commit. A callback that handles its own failure may return an ordinary successful acknowledgement. |
| Completion | Return the publication owner on success; Reflect can retain its target-returning facade contract. |

This deliberately changes the old rule that a loaded Error from a managed observer
is ignored. A required hook must be result-aware. Best-effort consumers can handle
and record their error and return nil; they should not rely on the caller silently
discarding an Error while treating a native status differently.

There is no proposed language-wide automatic unwind on Error, and no compiler
insertion of defensive checks around every send. The publisher checks results
because it is a sequencing protocol. The methods, `isError` sends, callback lookup
and constructors remain ordinary and replaceable.

The experimental `PublicationResultProposal` object has the selector
`publish:on:with:authorize:describe:notify:`. This is **a fixture API**, not a new
public runtime API. Its nine scenarios run in fresh, mapped and rebuilt policy:
plain success, explicit description, denial, authorizer Error/native failure,
description Error, observer Error/native failure, and an observer that handles its
own native failure. All 27 comparisons pass with collecting native hooks. They
check event order, Error identity, commit state, callable installed code and no
Store-basis change during the tested publication calls.

The prototype does not include Generation's required acknowledgements, removal,
raw notification policy, the bootstrap cutover, reservations or the Image recovery
fix. It uses current ordinary native-failure conversion, which still loses numeric
status/detail. It demonstrates the proposed sequencing contract, not a complete
language-wide error normalization implementation.

### Native transport and the remaining error work

Keep Word/Bytes/Object ABI adaptation. C ABI transport statuses and ordinary
language results are different layers; neither requires two ordinary language
semantics based on code representation. Normalize ordinary send results consistently
in both emitted execution strategies. Keep raw native transport explicit and local;
remove reliance on a dynamically scoped preservation flag that changes nested
callbacks' behavior.

Preserving native status/detail as inspectable Error data is desirable. A proposed
`Error nativeStatus:detail:` factory could do that through ordinary owned state;
that selector does not currently exist. Factory replacement and failure while
constructing an Error need defined terminal behavior, not recursive conversion.
Raw Id observers and deferred native requests need their own boundary tests; do not
claim the ordinary publication prototype has qualified those paths.

**Publication need not wait for a language-wide synchronous cleanup.** Its owner can
use the already-qualified normal resumable construction path and the explicit result
contract above. Update all native consumers to that contract together: Reflect,
Codegen/Image, Alien and Generation. No synchronous adapter or new permanent Rust
publisher is needed to preserve the rejected legacy error distinction. Broader
representation-independent error normalization remains separate required work.

## 3. Opt-in descriptions without a mandatory mirror

Keep the demonstrated snapshot approach as the initial integration. A snapshot
records selected owner identities and observations, their coverage and capture
basis/interval, and any deliberately retained history. It must not claim to be a
live or atomic whole-world mirror. Direct Reflect inspection remains authoritative
for current dictionaries, bindings and slots.

Use one publication implementation with an optional per-operation description
action. The default does not resolve Store, allocate a catalog or attach a global
observer. An explicit Store entry point supplies the action and chooses the Store.
The action runs between successful mutation and required managed observation;
failures remain visible through the result contract. Snapshot retention ends when
its owning Store/snapshot releases the data under its declared history policy.

The prototype proves absence of description calls and fact writes in its default
case, **not a Store-absent core**: today's Id raw observer and bootstrap still contain
Store integration. Remove those default edges at cutover. Do not hide them behind
a disabled frontend. A live projection can be added later with explicit subscription,
coverage, consistency, invalidation and retention rules. It is not a prerequisite
for optional Store.

Store/Relational's own self-description and explicitly asserted user facts remain
intended functionality. Do not identify cleanup candidates merely by their
vtable/selector-shaped data: only positively owned automatic mirror rows may be
retired. Preserve discovery and supported edits before removing a public fact family.

## 4. Startup memory: narrower diagnosis and targets

Two unchanged production-host processes were sampled with `ps` through startup,
then blocked in `System readLine`. The second performs one explicit collection
before that boundary. This is a diagnostic comparison, not a timing benchmark or
an exact allocation attribution; sampling can miss the instantaneous peak.

| Diagnostic | Normal | Explicit GC after startup |
| --- | ---: | ---: |
| Sampled peak RSS | 1,313.0 MiB | 1,328.2 MiB |
| RSS at input boundary | 743.9 MiB | 562.9 MiB |

The earlier seven-sample `/usr/bin/time` measurements remain the authoritative
before/after timing result. The new observations show that peak RSS is not the
retained Method-code footprint and that post-startup collection does not remove
the startup peak. They do not establish a leak or justify changing GC scaling.

The read-only `image-code-inventory.py` scans BIRNAMO1 records using mmap, validates
record bounds/ELF headers and confirms hash matches byte-for-byte. It is diagnostic,
not an alternative image-admission implementation. Body ordering follows the
current Image plan: resumable then synchronous for each portable program.

| Image inventory | Bootstrap | Tools |
| --- | ---: | ---: |
| Image size | 208.07 MiB | 303.58 MiB |
| Native object payloads | 200.12 MiB | 292.35 MiB |
| Portable programs | 1,794 | 2,654 |
| Resumable body payloads | 104.46 MiB | 151.89 MiB |
| Synchronous body payloads | 80.89 MiB | 118.50 MiB |
| Exact duplicate payload bytes | 14.63 MiB | 21.78 MiB |

Most bytes belong to distinct body variants. Removing identical adapter records
would reduce these files by only about 7%. Both variants are currently emitted and
mapped for every program; that does not mean either entire class can safely be
discarded. Heap Block locators still select synchronous execution, while the current
selection inventory labels all bodies resumable. Derive actual required variants
and their transitive closure before changing this representation.

The source also shows simultaneous large copies. Image retains the file Vec while
constructing the ImageFormat input Bytes. `record:` copies its large Code record;
`nativeCode:authority:` then copies the record's entire unit payload merely to hash
it. Native format extraction and Codegen loading make further owned snapshots.
The existing early drops help lifetime but cannot eliminate simultaneously live
copies. This is a grounded cause candidate, not a measured per-copy peak attribution.

Recommended memory sequence:

1. Add a generic owner-level range-hash operation, then avoid the full-payload copy
   in ImageFormat's checksum validation. Keep validation policy authored and the
   hash send late-bound. Qualify bounds, hashes, replacement and collection.
2. Investigate explicit owned immutable byte ranges and a phase-by-phase lifetime
   contract. Preserve `copyFrom:to:` semantics. Do not keep foreign borrows across
   sends or let one retained native Unit accidentally retain the entire image.
3. Derive and emit only genuinely required body variants, with source-free mapped
   admission and rebuild equivalence tests. Do not infer needs from today's coarse
   selection flags or silently discard explicit native entry behavior.
4. Deduplicate identical serialized object records with a versioned reference table;
   preserve payload identity, arity and independent Method state.

No memory optimization, threshold adjustment or image-format change is implemented
by this investigation. RSS savings for those proposals remain to be measured.

## Historical evidence and proposed next work

The [receipt](../measurements/store-refactor-concerns-20261015.json) records source
hashes, runtime provenance, observations and formal results. The existing Generation
transaction/ledger tests pass and the known error/image mismatch was rechecked.
The new abstract gate checks 301 Lean theorems, rejects 77 mutations and obtains all
182 expected TLC outcomes. New checks cover result identity/diagnostic preservation,
conditional inverse ownership, opt-in description and no callback replay. Atomic
owner compare/restore is a model assumption; these are not proofs of the current
Image implementation or its future replacement.

At the time, the recommended next implementation was **Image compensation that
preserves intervening work**, followed by an authored publication/result-contract
cutover. Compensation has since been implemented; the universal publisher was
subsequently rejected. The dictionary/Global/fact counterexample is now a passing
preservation regression. Keep memory work separate and measurable; preserve these
regressions while migrating coordinated state.


Reproduce the focused investigation against a production prefix:

```sh
BIRNAM_GC_PRECISE=1 BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib \
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/native-message-tests \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test image_recovery_conflict --test publication_result_proposal -- --include-ignored --nocapture
mise exec -- python3 scripts/image-code-inventory.py PREFIX/lib/birnam-bootstrap.image PREFIX/lib/birnam-tools.image
mise exec -- python3 scripts/image-startup-memory-probe.py PREFIX
mise exec -- python3 scripts/check-rooting-model.py
```

The memory sampler requires macOS (`ps` and `vmmap`). None of these commands
installs a runtime or changes the supplied prefix.
