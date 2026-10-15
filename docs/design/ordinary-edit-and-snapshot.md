# Ordinary editing and snapshots of edited worlds

2026-10-15. Implements the first coordinated runtime slice of
[minimal mutation](minimal-mutation-and-coordination.md). The preceding investigation
was committed as `271eeec7`; this report describes the subsequent implementation.

## Delivered ordinary path

Reflect retains `defineMethod:on:as:` and `removeMethod:on:`. Construction still
uses current factory/template messages and supplied artifact code. The final edit
now goes directly through the actual vtable owner:

```text
Reflect definition
  -> current Method construction policy
  -> existing target/private-vtable resolution
  -> vtable addMethod:as:
```

Removal checks direct presence and sends that vtable `removeMethod:`. It does not
copy an inherited method into the dictionary. Missing direct bindings still return
`#missingDirectMethod`; a registered vtable no longer needs Store's exported-owner
restriction merely to remove its own entry. Reflect returns the original target
on success and forwards owner errors. It creates no method description facts.

The native Generation mutation-guard object, its bookkeeping bit and its two guard
selectors were removed. `enterRunning` retains explicit-operation phase checks,
but no longer installs a global authorizer or observer in Store. Ordinary method
editing is allowed during main, console and socket execution. No universal
`MethodPublication` owner was introduced.

Dictionary invalidation, current implementation sends, entered/captured Method
ownership, GC and deoptimization remain with Id/Execution. The primitive-floor test
now exercises the actual Reflect facade with Store, Relational and Generation
absent, as well as replacement during a call and subsequent reclamation.

## Explicit facilities remain explicit

Explicit Store `addMethod:on:with:` and `removeMethod:on:` retain their authorization,
description and managed-notification behavior. Compiler tests now verify both that
ordinary Reflect edits bypass those hooks and that explicit Store calls continue
to honor denial, native failures, notification ordering and collecting hooks.

Generation's explicit transactions still use their current Store-backed journal
and observation protocol. Adoption registers that optional managed observer;
ordinary Reflect edits are detected by reconciliation instead of being forbidden.
A callback's foreign edits are allowed even during a running program and can make
an explicit transaction stale. The candidate-retirement test now exercises that
same drift/compensation path in both bootstrap and running phases. Nested explicit
fulfillment and stale prepared operations remain rejected.

This slice does not remove every Store dependency. Store still installs its raw
mirror-invalidation observer, and compiler metadata, lifecycle records, Alien
recipes and Image staging still use Store. Removing that default observer and
remaining bootstrap edges remains separate work. The new default Reflect and image
binding paths create no automatic method rows, but this is not a Store-free core
bootstrap claim.

## General snapshots do not require an unchanged managed revision

Image binding installation also uses dictionary-owner messages and returns the
actual target, rather than Store identity. Image removal likewise edits its direct
binding. Native image callbacks and failure handling retain their existing call
boundaries; advanced live restoration is still available.

`Generation validateImageGeneration` remains strict for a caller explicitly asking
whether a managed revision is current. Image uses the new, ordinary replaceable
`Generation prepareImageSnapshot` message. It performs the existing lazy drift
reconciliation, but permits capturing the actual changed code. This observation
can mark opted-in revision/component records drifted and disable their rollback;
it does not replace the program's bindings or replay its source.

A managed binding replaced with an opaque native entry lacking an image-inventory
implementation remains inadmissible. The added native regression checks refusal
without replacing an existing destination file. This is a portability requirement,
not enforcement of equality with the old source revision. It is not a new complete
inventory of arbitrary untracked native edits.

On restore:

- Matching managed snapshots retain the existing composition/adoption path.
- Drifted revision records remain descriptive data. No old candidate capability or
  transient managed ledger is recreated from them.
- The saved Methods remain callable and editable. Explicit managed operations still
  reject stale/unadopted state.
- A restored drifted world can be saved and loaded again without manufacturing a
  current revision claim.

Source and recipe information remain available; a source revision is not silently
rewritten to pretend it produced an ordinary later edit. No new image-format flag
or backwards-compatibility adapter is introduced for this distinction.

## Snapshot consistency

Before graph capture and after native code generation/admission, Image observes
Store identity/basis, Globals identity/binding basis and Id's binding epoch. A
detected change rejects the snapshot before atomic file publication. The caller's
existing file remains intact; callback edits remain live and callbacks are not
replayed. These are data observations at a snapshot boundary, not checks for method
replacement around ordinary sends.

The native `image_snapshot_stamp` regression changes a dictionary during code
emission, forces GC, and checks rejection, one callback, the surviving edit and
byte-for-byte preservation of the previous file. The existing Generation save-time
drift test likewise now expects snapshot rejection rather than treating source
revision equality as the reason for refusal.

The observations are conservative and depend on the owners' revision protocols.
They do not promise an atomic point-in-time snapshot of every arbitrary object or
callback side effect. The coarse Id epoch can advance for a removal attempt even
when the direct binding is absent.

## Qualification

`ordinary-edit-image-test.py` starts both unadopted and explicitly managed worlds,
removes the writer sources, edits while main is running, checks no method facts,
checks a captured Method's old result, and saves the changed world. Fresh mapped
and rebuilt processes execute the changed code and can edit it again. The managed
case retains drifted metadata and rejects a current-generation claim. A resave/load
case covers a restored drifted world with no native ledger.

The former running-veto assertions in console/TCP and CompiledUnit tests now check
allowed ordinary edits to unrelated bindings. The tests for explicit activation,
exact rollback, nested fulfillment, stale preparation and foreign callback drift
remain. This is an intentional contract change, not removal of those checks.

The first language run passed all 826 assertions but stopped at the local-Cell
language object-count allowance. Diagnostic batches on baseline and candidate both
showed the same fixed 29-object measurement delta. A new native harness runs the
same operations in the production tools world and measures Id's counter after the
language frame returns: 0, 1, 50 and repeated 200-call batches leave exactly zero
objects and zero facts. A deliberate retention control fails at the first one-call
batch (+2 objects). Language telemetry is now diagnostic; the native exact-zero
check is the reclamation gate. No runtime leak fix or widened allowance is claimed.

Final qualification passed 826 language assertions and all postchecks, 22 native
integration binaries plus the isolated Store/Generation-free Reflect floor, host
receipt/relocation acceptance, console/TCP and 59 CompiledUnit policy checks, edited
mapped/rebuilt images and a drifted resave/load. Native image tests include twenty
successive resaves and mapping reclamation. The relevant Rust unit suites passed
40 tests. The native run was resumed after replacing old running-veto expectations;
its earlier successful cases and the complete resumption are recorded separately.

Final review found that removal's lifetime query incorrectly required an owned
Method even for a raw native entry. The new floor regression fails on that candidate.
The final correction accepts the legitimate Unit reply and retains owned Methods
when present. After rebuilding, the floor (including raw native removal), compiler,
snapshot-change and Method-binding tests, and managed mapped/rebuilt/resaved image
checks passed. The broad language/host/transaction checks above precede this narrow
correction; the receipt distinguishes their prefixes and the focused final reruns.

Formal checks distinguish actual saved code from managed authority, reject adoption
of drifted/descriptive snapshots, and model preservation of the old file and callback
writes when a snapshot observation changes. These are abstract contract checks,
not a proof of the entire implementation. The gate checked 309 Lean theorems,
rejected 83 mutations and obtained all 193 expected TLC outcomes. Final commands, source hashes and results
are in the [receipt](../measurements/ordinary-edit-and-snapshot-20261015.json).

## Remaining boundaries

- Remove Store's default raw mirror registration and finish the remaining owner
  migrations before claiming optional Store/Relational bootstrap.
- Replace the explicit Generation observer handshake/journal coupling only in its
  own coordinated slice; it is no longer a prerequisite of ordinary Reflect edits.
- Fix the known advanced live-load compensation defect. Snapshot-save rejection
  does not repair live-load rollback overwriting callback writes.
- Keep the explicit synchronous failure/image mismatch and general native error
  normalization work separate. This change does not silently alter synchronous
  artifact invocation semantics.
- No release, user installation or performance improvement is claimed here.
