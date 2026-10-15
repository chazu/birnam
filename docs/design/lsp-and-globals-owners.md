# LSP ownership and the Globals publication boundary

2026-10-14. Follows the owned Module registry (`777df40a`).

Historical slice report: the subsequent [owned Globals implementation](owned-globals.md)
completes the storage transition described here as remaining work.

## LSP: ordinary state

All thirteen LSP fields now live in ordinary reference slots. `Lsp new` creates a
fresh instance through the current `initialState` method and current behavior.
`clone` makes a shallow snapshot: immutable Maps initially share their values,
then writes replace the selected server's slot. Aliases of one server continue
to observe its edits. This deliberately replaces the old fact-backed clone's
absent fields with ordinary object cloning.

`Reflect slotsOf: server` names every field; `Reflect slot: name in: server` and
`Reflect slot: name in: server put: value` inspect and edit the actual state.
The fields are docs, versions, projectRoots, projectCache, projectFailures,
documentPaths, projectIndexes, runtimeCompletionCache,
runtimeGlobalCompletionCache, runtimeGlobalCompletionBuildCount,
completionResponseCache, inputBuffer and workspaceUri. Explicit facts about a
server remain independent Store data. No automatic facts or history accompany
these fields. The optional Doc catalog still works through its existing hooks.

Protocol algorithms are unchanged: stale versions, UTF-16 positions, byte-based
framing, parse-only project inspection, document close and diagnostic publication
retain their existing behavior. In particular, completion responses remain
bounded by URI and require matching text and position before reuse; an edit does
not eagerly discard that Map. Close drops document payloads but retains nil-valued
URI keys, as before. This slice does not claim to fix all existing LSP retention
policies. Export and receiver completion snapshots retain their existing lifetime
and explicit reset policy.

There are no frozen sends, monkeypatch checks, new native storage tables or ABI
changes. Constructors and other authored methods remain resumable. Old images
keep their saved fact-backed LSP code and state. New images save the actual slots
and captures; loading does not reinterpret old facts as slots.

## Globals: transaction-owner protocol, before storage cutover

Generation previously named Store's `#__globals__` entity directly when capturing,
validating, publishing and compensating bindings. Its journal now distinguishes
ordinary Store facts from global binding writes and calls the binding owner:

| Message | Contract |
| --- | --- |
| `Globals bindingAt: name` | Snapshot: `#[]` for absence, `#[value]` for presence, including `#[nil]`. Values retain identity. |
| `Globals bindingBasis` | Nonnegative revision observation used to reject stale prepared publication, independently of Store's basis and Id's method epoch. The current adapter returns Store's basis. |
| `Globals restoreBinding: name expected: before previous: after reservation: owner` | Conditional per-name replacement/removal using the existing reservation; returns Globals on success, propagates failure otherwise. Both values use the snapshot encoding; matching uses presence and object identity. |

Generation consults these through current sends. Compensation compares the
current binding with its journal entry and changes only that name; unrelated
callback edits survive. No whole-world Map rollback or replay of callbacks is
introduced. The existing method epoch, quiescence, ledger and reservation checks
remain publication correctness checks, not guards around ordinary language sends.

**Globals storage has not moved yet.** The initial adapters deliberately retain
Store's authority, history, direct fact aliases, monotone observations and
reserved inverse capacity. Ordinary source lookup remains Globals-first with
nil-triggered Namespace fallback; Reflect's existing Namespace-first lookup and
separate owner inventories remain unchanged. Store/Relational bootstrap,
compilation recipes and image formats are unchanged. This adds no new foreign
binding registry and is not the final Store-independent implementation.

## Why the cutover is a separate slice

A local prototype successfully saved and restored authored Globals getters and
writers capturing an ordinary state owner. That establishes feasibility of the
capture transport, not transactional image recovery or production performance.
It is not shipped or enabled. A getter-only migration would still be incorrect:

- Generation reserves inverse capacity before publication. Merely rebuilding a
  persistent Map during rollback would abandon the existing guarantee under the
  Store's bounded fact budget unless that obligation moves to the binding owner.
- A restored value is not sufficient to validate a prepared transaction; binding
  revision observations must still detect intervening changes.
- Namespace-exported Globals is encoded as an export reference, so placing its
  state in a slot does not by itself make Image serialize that slot.
- Image installation swaps Store state transactionally and restores resident
  behavior on failure. Captured binding state must participate in that recovery,
  including loading old images whose native Globals methods still read facts.

The next storage slice must implement an authored owner with bounded reservation
ownership, selective inverses and atomic image installation/compensation, then
switch the default. It must remove the old automatic fact authority deliberately;
an indefinite compatibility mirror or a permanent Rust registry is not a solution.
The new protocol makes the remaining obligations explicit without changing their
semantics prematurely. Reservation acquisition/release and image transport still
need their owner transitions; this protocol alone does not establish Store-free
Globals or a Store-free core.

## Verification scope

The LSP fixture checks independent instances, snapshots, aliasing, all slot names,
reflected edits, independent explicit facts, stale changes, completion admission,
partial/multiple frames, cache resets, replaceable transport, constructor suspension,
precise GC and reclamation. The driver checks source-free new and old images in
mapped and rebuilt modes; the stdio smoke test exercises actual project traffic.

The binding fixture checks absence/nil, snapshots, conditional conflict,
reservation use, removal, foreign writes and legacy fact edits. Generation's
transaction integration test replaces all three owner methods, collects during
calls, and changes only the owner revision to prove stale rejection independently
of Store, both between preparation and commit and during adoption/reconciliation. Existing publication failure, cleanup failure and inverse tests still
exercise both synchronous and resumable candidate code.

Lean checks snapshot encoding and selective conditional replacement, including
negative controls. Existing TLC models retain their rooting, live dispatch and
scheduling checks; publication and rollback are covered by the executable gates. These are abstract contracts, not a proof of the entire VM or
arbitrary replacement methods. Final acceptance evidence is recorded below.


## Acceptance

[Source-bound evidence](../measurements/lsp-and-globals-owners-20261014.json)
records the exact test prefixes and hashes. The final candidate passed all 826
language assertions plus scratch, reflection, inspector, Cell, Module, LSP and
binding-owner fixtures; native candidate/Generation/ledger/transaction tests;
console and TCP lifecycle tests; and native image owner/recovery acceptance.

The LSP artifact and Store/Codegen runtime libraries are byte-identical to the
preceding combined candidate, which also passed the stdio project smoke test,
new and legacy LSP images in mapped/rebuilt modes, all 26 REPL goldens, artifact
preflight/order/failure checks, Host relocation, compiler/Store/Relational native
checks and the 20-epoch image resave/liveness gate. After adding independent owner
revision checks to adoption/reconciliation, Generation and image recovery were
requalified against the final library. Native image/Store lifetime tests used a
separate TCC metrics copy; production prefixes were not instrumented.

Across batches of 0, 1, 30 and 100 discarded LSP instances, fact growth is zero
and the post-GC object delta remains 20, identical to the zero-call control.
No CPU speedup or complete Store-free activation is claimed.

The formal gate checks 252 Lean theorems, rejects 57 mutations and obtains all
127 expected TLC outcomes. A separate executable negative control fails against
the intermediate implementation missing the ledger's independent owner revision
checks; the complete transaction test passes with those checks in place.

Builds regenerated bootstrap artifacts and startup images using the development
Cargo cache. This is not clean-cache release qualification. The installed user
version was not changed, and these changes have not been committed.

Reproduction (use the project-managed Python for the socket harness):

```sh
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
python3 scripts/lsp-owner-test.py PREFIX --legacy-prefix PREVIOUS_PREFIX
BIRNAM_BIN=PREFIX/bin/birnam bash scripts/lsp-project-smoke.sh
mise exec -- python3 scripts/generation-cli-test.py PREFIX
mise exec -- python3 scripts/check-rooting-model.py
CARGO_TARGET_DIR=.cargo-target/native-message-tests \
BIRNAM_TEST_MODULE_DIRECTORY=PREFIX/lib CARGO_PROFILE_DEV_PANIC=unwind \
  mise exec -- cargo test --offline --locked -p birnam-siphon \
  --test candidate --test generation --test generation_ledger \
  --test generation_transaction -- --include-ignored
```
