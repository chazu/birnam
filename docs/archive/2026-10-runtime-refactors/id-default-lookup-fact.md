# Id default-lookup fact — 2026-10-22

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Logue's [interactive profile](logue-latency.md) found substantial dispatch cost
under tile rendering. This change reduces the cost of ordinary Id dispatch
without changing the game, its symbol comparisons, collection policy or native
message protocol.

## Change and invalidation boundary

Previously each ordinary `bind` looked up the metaclass's `lookup` method,
compared its function and context with Id's built-in lookup, then looked up the
requested method. The first lookup repeated even when no binding had changed.

`Vtables::has_default_lookup` now remembers a positive fact that the specified
metaclass/selector resolves to **this registry's** actual built-in lookup.
The fact contains only three integer addresses: metaclass, selector and registry.
It stores no selected method, foreign context, parent link or rooted object.
The receiver's current metabehavior is checked on every dispatch, and the
requested method is still selected from the live dictionary on every send.
Custom lookup continues through the existing metacircular path.
No language sends are eliminated: this removes repeated private dictionary
reads inside Id, rather than skipping source-level or owner-query messages.

Invalidation is explicit; it does not depend on unsaturated epoch arithmetic:

| Mutation | Invalidation |
| --- | --- |
| Add, replace or remove a method | `note_mutation` |
| Change a parent or reassociate a behavior | `note_mutation` |
| Install an opaque weak binding or remove dead weak keys | `note_mutation` |
| Retire behavior records | Clear the fact in `remove_objects` |
| Register a fresh behavior with initial methods | No existing lookup changes |

Registry address is part of the key, so moving a registry cannot reuse a fact
about the old native context. Retirement clears the fact before address reuse.
False answers are not memoized. Dictionary reads and fact updates are inside Id,
on its owner thread, with no callback across the inspection interval.

`nativeBindingsMatch:` checks this fact once per callback-free batch. It still
checks each receiver/selector's liveness, current metabehavior, actual selected
method and exact context. Merely hoisting the original metaclass lookup out of
that loop was tested first and produced no useful application speedup.

## Paired interactive result

The [evidence record](../../measurements/id-default-lookup-20261022.json) retains
phase samples, exact harness, artifact/source hashes, output parity and checks.
The baseline and candidate performance prefixes differ only in `libbirnam_id`;
both Id libraries were built with Rust 1.95.0 in release mode. All other native
modules, images and Logue sources are identical. Two fresh processes per variant
run in baseline/candidate/candidate/baseline order. Each uses three floor-change
round trips and 80 ordinary turns in a real drained Termbox PTY. GC is enabled.

| Operation | Baseline median CPU | Candidate median CPU | Reduction |
| --- | ---: | ---: | ---: |
| Full static map | 2,695.34 ms | 2,584.64 ms | 4.11% |
| Descend redraw | 2,715.34 ms | 2,600.90 ms | 4.21% |
| Ascend redraw | 2,776.28 ms | 2,616.77 ms | 5.75% |
| Ordinary turn logic | 13.24 ms | 12.66 ms | 4.41% |
| Ordinary redraw | 38.40 ms | 36.46 ms | 5.04% |

All four runs produce byte-identical terminal output before timing reports.
This is a modest local improvement, not a whole-language or cross-machine
performance claim. Full redraw remains slow and automatic collections still
produce half-second pauses; this slice does not remove either remaining cost.

## Correctness checks

The Id regression checks exact native context, replacement/removal/restoration,
inherited lookup, parent changes, weak shadowing/removal, retirement/address
reuse, and changes after both epochs saturate. Existing dispatch tests exercise
actual sends through replaced lookup and custom metabehaviors.
Deleting the invalidation step in a disposable mutant makes the new regression
fail on a stale positive fact; the mutation is not present in the checkout.

The production binding-query test now includes a maximum-size mixed direct and
inherited batch, a mismatch in its last row, a non-live selector, a late-row
override, and a saved matcher invoked after replacing/restoring global lookup.
Both global and per-behavior custom lookup must be refused without executing
the callback. The evidence record records the completed broader checks and
installed artifact status separately.

The optimization relies on Id owning all dictionary/graph mutation paths. New
mutators must invalidate the fact before exposing their changes to callbacks.
This obligation is covered by targeted native tests, not a new formal proof of
all Rust mutation paths or a release qualification.

## Completed delivery

- Workspace: 349 tests passed; 130 native integration tests remained ignored in
  that invocation and are not counted as executed by the workspace gate.
- Targeted production validation: all 35 selected native test executables passed,
  with ignored tests explicitly enabled. Coverage includes compiled execution,
  replacement/reentry, GC, loader callbacks, image restoration and retention.
  Native export, dependency and install-name audits passed.
- Logue: all 34 game checks, 68 independent-oracle checkpoints and six Python
  oracle/checker tests passed. Acceptance timing under concurrent validation is
  not used in the paired speed comparison above.
- Installed a release-profile development build at
  `/Users/chazu/.local/opt/birnam`; the normal `~/.local/bin/birnam` symlink uses it.
  The exact installed manifest/receipt verifies, and its Id library is
  byte-identical to the measured candidate and tested build.
- The installed game passes its 34 checks again. A real controlling-PTY smoke
  drives class selection, six alternating movement keys, quit, and terminal-mode
  restoration through the original `logue.birnam` main loop.

The previous installation is retained at
`/tmp/logue-runtime-20261022/installed-before`. This measured slice is recorded before the subsequent simplification work.
No version bump or release publication is part of this slice.
