# Coordination cleanup

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Follow-up to [ordinary mutation](minimal-mutation-and-coordination.md).
Each implementation slice was committed separately.

**Superseded live failure policy (2026-10-16):** section 4 records the historical
conditional-compensation implementation. The user subsequently selected
[basic live loading without rollback](basic-live-image-loading.md): save first if
recovery is needed, retain applied changes on failure, report partial application,
and abandon stale capabilities. Sections 1–3's ordinary-edit/startup boundaries
remain; maximal selective rollback is not pending work for basic loading.
The unused conditional Store-exchange API described in that historical section
was removed in the 2026-10-20 optional-Store cleanup. On 2026-10-28 the user also
retired the old explicit Id atomic method-undo/global-epoch ABA contract in favor
of authored value-checked candidate rollback. Store snapshot revision observations
remain separate; historical atomic-undo descriptions below are not current
requirements.

## 1. Explicit dictionary-description invalidation

The user retired the universal `methodChangesTo:` subscription on 2026-10-26,
keeping the corrected Id unchanged. Ordinary dictionary edits do not notify Store.
Store's existing `methodChanged:on:` remains an explicit, replaceable operation
for invalidating its owned description rows. Neither loading Store, `backfillMirror`
nor an explicit Store method edit installs a subscription. Descriptions are
snapshots, not promises of live lookup. Store's own publication authorizer and
post-edit observer remain part of its explicitly requested operations.

Default ordinary method descriptions were already disabled. Mirror rows are
nondurable and are not imported by Image. Store/Relational self-description and
user facts are retained. Mirror cleanup now checks the owned `mirror` flag before
retiring a row, including when an explicit fact has an identical triple.

Current regression coverage checks explicit invalidation, preservation of user
facts, replacement of the handler and absence of the retired Id operation. The
former universal-observer replacement/reentry tests describe a retired feature.

## 2. Direct Generation writes

Generation adoption no longer registers a Store observer. The expected-notification
slot and callback are removed. Explicit transactions install/remove Methods through
their current vtable owner messages, check their ordinary results, and verify that
Id observed exactly one dictionary mutation. Additional writes, including a
callback's remove/re-add ABA, make the coordinated operation stale. This coarse
condition deliberately admits fewer prepared transactions than a selective owner
version could; ordinary edits remain unrestricted. No method identity guard or
callback replay is introduced.

Final ledger observations and conditional compensation still protect explicit
activation/rollback. Store-backed lifecycle facts and their reserved inverse
capacity remain until the separate owner migration; deleting those would weaken
the requested transaction contract. Ordinary external edits are reconciled when
an explicit consumer validates its managed revision.

Validation for slice 2: candidate mapping/retirement, generation projection, all
six ledger modes, and native transaction tests passed. The final transaction test
covers failure before a dictionary write as well as after removal, fact publication
and reservation release. Console/TCP activation and exact rollback also passed.
The final error-before-write correction was rebuilt into the native test prefix;
the CLI run used the preceding full build (same successful-write path). The next
full installation rebuild includes that correction. The formal gate checked 309
Lean theorems, rejected 83 mutations and passed all 193 expected TLC outcomes.

## 3. Fresh startup versus live restoration

System `startImage:rebuild:`, `startImageMapped:` and
`startImageBytes:rebuild:` are explicit startup admission operations. The caller
must discard the startup world on failure, without entering application code or
falling back to source in that world. They retain validation, staging, native
signatures, source-free restoration and ownership, but skip prior-method retention,
the Globals checkpoint and live publication inverses. Store/source installation is
still necessary to publish the restored world; startup does not perform a second
swap to recover a previous application.

HostEntry has matching `startImage:rebuild:compiled:` and `startImageBytes:`
messages. Normal bootstrap, standalone image/baked startup and worker startup select
them. Existing `loadImage:` variants retain the advanced live contract. The Rust
Host facade also exposes live `load_image` / `load_image_mode`; fresh constructors
use separate startup dispatch. A flag inferred from Generation is not involved.

This slice separates the contracts. The previously reproduced live compensation
bug is addressed in the following commit, not claimed fixed by startup bypass.

Startup qualification: the new no-compensation failure probe passed in mapped and
rebuilt modes. Installed CLI checks passed with saved main, deleted source/assets,
missing/invalid main and load diagnostics. Worker tests passed for copied values,
errors, asynchronous results, timeout/crash, orphan reaping and authored service
cancellation. Isolated Host tests passed for bootstrap without compilation, saved
code/rooting and mapped generation resave/rollback. Broader rebuilt-generation
qualification continues alongside the following recovery slice.

## 4. Conflict-safe live recovery

The original dictionary/Global/fact erasure regression now passes in both image
modes. The following owner conditions replace unconditional checkpoint restoration:

- Id `restoreBinding:of:in:expected:previous:atEpoch:` takes native entry/Unit
  operands and an observed native epoch. Target association, epoch and entry are
  checked together with the inverse inside Id, with no intervening send. It returns
  the committed epoch or Unit for conflict. Optional notification runs after commit;
  a subsequent inverse cannot overwrite a notification's later dictionary write.
- Store `imageStateRevision` observes process-local mutation, including policy and
  reservation changes that need not advance the durable fact basis. The conditional
  `swapImageStateWith:atRevision:stagedRevision:` checks both owners and exchanges
  state atomically. Revisions remain attached to their Store identities. A callback
  edit makes a later exchange fail instead of overwriting explicit facts.
- Generation's existing source stage records its owner revision. Publication and
  `rollbackSourceRegistry` compare it inside the registry; a later source registration
  survives recovery. Staging roots retain both sides until completion.
- Image obtains the planned Method before installation. It records only verified
  owner edits, detects extra dictionary changes including ABA, and never infers its
  expected write solely from the post-callback dictionary.
- Image does not reset resident Globals storage: it did not write that storage.
  Callback edits to it must survive. If the captured Globals owner changed during
  restoration, dictionary compensation is conservatively declined to preserve the
  current owner's access paths.

Independent source/Store inverses are attempted even when dictionary recovery
conflicts. Failure of an inverse also counts as conflict. The original native
failure or ordinary Error is preserved; `System imageSaveReport` answers status
`#compensationConflict` when complete recovery cannot be established. The composition
identity is cleared and old prepared/adoption capabilities are abandoned. No user
callback is replayed. This is scoped explicit-operation recovery, not a universal
transaction service or a guard around ordinary language sends.

The guarantee is preservation, not arbitrary-callback atomicity. A conflicted live
load can leave a partially loaded world. Foreign-wrapper publication still lacks
an exact inverse capability, so a failure after attempting it conservatively retains
the current dictionary set and reports conflict. Similarly, imported Globals-owner
replacement and unclassified lifecycle fact edits can prevent complete recovery.
This deliberately replaces the old unconditional promise with an honest result;
maximal selective recovery for those domains requires their own operation receipts.
Store/Relational remain optional facilities in the design; remaining compiler,
recipe, lifecycle and process owner migrations and full Store-absent bootstrap are
separate work.

### Qualification and limits

The three preceding commits are `b1a6f64f` (opt-in observation), `891f7c0a`
(direct Generation writes), and `14bd6d95` (startup/live separation).
The recovery implementation and its evidence are committed as the fourth slice.

Qualification passed:

- 826 language assertions, scratch/Reflect/code-inspector checks, then the remaining
  Cell/slot/registry/LSP/Globals postchecks on the final rebuilt runtime.
- Native conditional-owner tests: exact inverse, already-restored entry, absence,
  ABA, target reassociation, collecting reentry from inverse notification, and
  Store policy changes that leave the durable basis unchanged.
- Mapped/rebuilt callback preservation for dictionary edits, explicit facts,
  Globals (including bound nil), and introduced source registrations. Ordinary
  Error identity and native failure status/detail both survive recovery.
- Authored Globals with a late restored heap target in both image modes. The test
  retains the actual target passed to the callback; a partial load need not yet
  have switched the Global that previously named the resident object.
- Native image roundtrip and twenty repeated resaves with a fixed live code set.
  Final Host qualification additionally covers mapped/rebuilt/resaved managed
  images, successor activation/rollback, adoption/retirement failures, rejection
  of old authority after conflict, captured Method lifetime, and explicit recovery
  by reloading a known image.
- Ordinary mutation on a floor physically lacking Store, Relational and Generation:
  inheritance, warmed lookup, live implementation sends, captured/entered Methods,
  native removal, ABA and precise GC.
- 25 relevant Rust unit tests; 313 Lean theorems, 84 rejected mutations and 199
  expected TLC outcomes. Models prove their abstract owner conditions, not the
  whole compiler or runtime.

The original preservation regression failed before the fix. Qualification also
caught valid exported aliases denoting the same dictionary entry: the live loader
now compares subsequent writes against its own earlier publication. Legitimate
private-vtable installation remains accepted, with the actual table rooted; its
association is conservatively non-invertible if later recovery fails. These are
explicit-operation data checks, not method-identity guards on ordinary sends.

Broad native roundtrip/resave and authored-target tests preceded the final alias
correction. The final build reran the affected Host image/lifecycle paths, Error/
bound-nil/owner regressions, all remaining language postchecks and the absent-owner
floor. The receipt records this sequence rather than claiming every gate reran
against the final binary. No user installation, release or push was performed.

See the [qualification receipt](../../measurements/coordination-cleanup-20261015.json)
for source hashes, log hashes and build provenance.
