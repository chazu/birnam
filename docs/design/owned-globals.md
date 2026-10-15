# Globals owns its bindings

2026-10-14. Follows [LSP and the binding-owner protocol](lsp-and-globals-owners.md).
This slice changes the default binding backend to authored Birnam. Store and
Relational remain available and keep their self-description.

## Authority and interfaces

`modules/globals.birnam` installs ordinary methods on the canonical Globals
object. Its captured ordinary storage object holds one immutable state Vector:
`#[bindings, live insertion order, binding revision, reservation banks, transient policy]`.
`Globals bindingOwner` exposes that object; its named `state` slot is discoverable
and editable through Reflect. `bindingState`, `bindings`, `bindingNames`,
`bindingBasis`, `bindingAt:`, `liveBindingsDo:` and `nameOf:` expose the same
current authority. Saved Maps/Vectors are snapshots, not writable aliases.

`at:`, `at:put:`, `includes:` and `at:ifAbsent:` retain their source-visible
contracts. Bound nil is present and suppresses the fallback. Source resolution
still tries Globals first and falls back to Namespace when the result is nil;
Reflect's existing Namespace-first lookup remains separate. Reverse lookup picks
the first live identity alias. Authored `nameOf:` returns language nil for no
alias; native reflection also accepts the legacy adapter's native Unit.

Global keys retain identity semantics, including equal but distinct Strings.
The authored `bindingKey`/`bindingIdentity` protocol leaves Symbols unchanged and
wraps other keys in ordinary one-slot objects. Those wrappers compare their
original identities and deliberately share a constant hash. Unusual non-Symbol
keys therefore have collision-list costs; the normal Symbol path uses the
existing Symbol hash. No address index, foreign identity table, or new Map
representation/image family was introduced. Inventories report original keys.
The raw `bindings` Map exposes normalized keys; use the owner for ordinary access.

`remove:` removes a binding and its live-order entry. No permanent removed-name
index remains. Reservation snapshots retain the old name order only while needed
for an inverse. Restoring a removed name places it before its first surviving
old successor, preserving the order of unrelated current names.

Ordinary writes create no `#__globals__` facts and do not change Store's basis.
Explicit facts under that entity remain independent data, including deliberately
requested history. Default writes no longer update those descriptions, and direct
fact edits no longer change the bindings. This is the agreed public transition,
not a compatibility mirror. During cold startup the artifact imports the seed
bindings once, retracts their old automatic facts, and compacts under Store's
existing retention policy. Method publication and compiler metadata still have
their separate Store dependencies.

## Mutation and reservations

Writes run through the existing optional `TaskExecutor transition:` policy, then
conditionally replace the complete state root. A collecting or reentrant callback
that changes that root makes the attempted publication fail with
`#compensationConflict`; it neither overwrites the intervening state nor replays
callbacks. Reads have no revision checks or monkeypatch guards. All owner,
collection and key-policy sends remain replaceable.

`ObjectModel replaceSlot:in:expected:with:` supplies the generic conditional
reference-slot operation. It validates its own ordinary storage and roots the
operands before outgoing callbacks, then compares and publishes with no intervening
send. It knows nothing about Globals, Store, Generation or scheduler policy.
Id remains the floor and owns identity, dispatch, dictionaries and native loading.

Globals owns `reserveBindingCompensationFor:count:`,
`reservedBindingCompensationFor:` and `releaseBindingCompensationFor:`. Banks hold
finite credits and a name-order snapshot. Conditional inverses compare presence
and value identity, consume a credit only on successful publication, and change
only the selected binding. Release removes the bank and its references.

These credits are independent of Store's bounded fact quota. They do not preallocate
all future heap memory; general allocator exhaustion follows the existing fatal
policy of ordinary collections, just as the old Store reservations did not insure
against system allocation failure. Failed collection operations do not publish a
replacement root. Deliberately replacing the owner protocols can change policy.

Generation uses distinct reservation tokens for binding inverses and fact inverses.
Its Store bank covers publication metadata; its Globals bank covers bindings.
Failure, rollback and failed cleanup retain selective compensation and reacquire
both owners' credits where required. The same arrangement works with legacy
Globals because its adapter can use a separate token in the old Store bank.

## Images and old worlds

The canonical Globals object is a Namespace export, so its captured storage owner
is transported through the ordinary portable method captures. There is no new
image section or native Globals heap layout. New images preserve binding values,
identity keys, aliases, nil presence, ordering and the owner methods themselves.

Image asks for `imageBindingCheckpoint` before publication. On failure it restores
method dictionaries, sources and Store, then calls `restoreImageBindings:` to
restore the resident binding-state root. This covers callbacks that edit the old
owner before the saved methods have been installed. Restoring methods alone would
not undo those edits to a captured mutable object.

The owner also provides `transientBinding:put:`, `bindingIsTransient:` and
`imageBindingProjection`. JSON and TOML use this explicit policy for process-local
PEG caches. Their previous Store `imageTransient` declarations cannot govern an
owned binding Map. Save reads a projected owner payload which omits those bindings
and live reservation banks, while preserving the original storage object's image
identity and aliases. The live root is unchanged. Transient policy persists and
parsers rebuild their caches lazily. A transient value deliberately retained
elsewhere still has to satisfy that other path's image contract.

After successful method installation and Map rehashing, `releaseImageReservations`
drops saved banks before Generation recovery. A restored image has no live native
activation journal to use those reservations. Failed loading retains the resident
banks through the checkpoint.

The authored methods occupy a private behavior above the original GlobalsVt.
Loading an old image removes the newer source methods and exposes the native
fact-backed parent again. Old images keep their fact aliases and history; a new
cold startup uses the authored owner. The remaining native methods are seed/legacy
adapters, not a second default binding authority or a new permanent Rust registry.

## Runtime corrections exposed by authored lookup

Ordinary Block construction used to create a fresh behavior and then mutate it
with `addMethod`. That needlessly changed the binding epoch during a read-only
owner observation. `Vtable delegatedWith:as:` now constructs the fresh behavior
with its initial NativeEntry atomically. The new identity cannot have been
observed before the storage primitive returns. Its normal GC wrapper retains the
operands/result across callbacks, enforces collection-phase restrictions and charges
the actual allocation. Id itself remains independent of GC policy. Later edits, including remove-and-restore, still
advance the epochs. Execution uses this current Id message; no dependency method
is captured for future calls.

Optional optimization queries must distinguish absence from an entered call's
failure. Execution now resolves those queries through Id once and invokes the
selected entry under its normal input/code roots. A missing hook declines without
running `doesNotUnderstand`; an entered native failure is propagated. With authored
Globals, constructing an error for a missing numeric/admission hook could recurse
back into the same missing hook.

Private recovery authority now identifies the enclosing drive delimiter together
with the existing caller frame, PC, descriptors and live acquisition. Balanced
synchronous reentry preserves that logical pending call. Public Actions still use
the changing drive epoch and remain stale after reentry. A different delimiter,
reused frame, changed pending call or missing acquisition still rejects recovery.
This permits authored getters inside callbacks without weakening control-transfer
or stale-action checks.

## Verification and scope

The owner fixture covers nil/absence, aliases, identity keys and reservation
owners, selective inverses, callback conflicts without replay, one-step task
budgets, and reclamation. The native owner fixture injects GC and mutation during
conditional slot replacement, rejects foreign payloads, and injects failure after
an image callback edits resident Globals. It checks the exact restored root,
method restoration, bank preservation, and mapped/rebuilt success.

The Store-absent floor exports actual production CodeObjects, capture identities
and native signatures, and installs them through the existing bootstrap binding
mechanism. It exercises reads, writes, identity keys, errors, reservations,
inverses, owner checkpointing and GC with Store, Relational, Lang, Bootstrap and
Generation libraries unavailable. This establishes operational independence, not
a complete Store-free application startup.

The new Id and Execution unit checks cover initial dictionary construction versus
later mutation, balanced reentry, stale public epochs, changed delimiters/frames
and released acquisitions. Lean and bounded TLC models cover identity, conditional
publication, selective compensation, banks, image recovery and recovery scope.
These abstract models do not prove the entire VM or arbitrary replacement code.

The full language suite passed 826 assertions and its auxiliary owner/retention
checks. The REPL suite passed all 26 cases, including saving after TOML cache
creation. New and legacy Globals/LSP images passed mapped and rebuilt loading
with source removed. Installed-prefix relocation, artifact publication, LSP
stdio, and console/TCP Generation activation and rollback also passed. The
formal gate checked 263 Lean theorems, rejected 60 deliberately broken proof
variants, and obtained all 140 expected TLC outcomes (including negative
controls). Both Globals and Cell passed their separate Store-absent floors.
All 69 native test binaries requested by the native-message gate passed across
the qualification and corrected reruns; the additional borrowed-read test also
passed. The source-bound receipt records which run supplies each result rather
than treating earlier failed fixture runs as successful.

The image lifetime tests explicitly clear bindings through Globals before clearing
Store. Their teardown reloads resident Globals policy from its compiled artifact;
it does not pin an obsolete captured binding snapshot across image loads. Such a
pin would retain a whole earlier world and include it in later saves, invalidating
the fixed-live-set experiment. The exact reclamation and resave-size assertions
remain in place. The materialization test empties the cache before measuring its
fixture because bootstrap and authored error construction can populate it too.
The Store capacity test now retains history from its current available basis and
checks that request succeeds, instead of silently requesting basis zero after
seed compaction. It still reaches the fact quota and proves reserved compensation
works while preserving unrelated facts.

The repeated-save test passed all 20 restore/resave epochs, alternating mapped
and rebuilt loading. After the first load, its fixed fixture held 311 facts,
960 fact-storage bytes, 623 live native units, and a 112,978,440-byte image across
the observed plateau; exact per-mode equality and final fixture reclamation
passed through the last epoch. These figures describe this test world, not a
minimal application image.

## Cost and remaining work

This is an ownership migration, with a measurable CPU regression. A paired
three-sample benchmark against the immediately preceding native-backed Globals
prefix measured the following medians (normal GC, Darwin thread CPU clock;
startup and setup excluded):

| Loop | Native-backed baseline | Authored owner | Ratio |
| --- | ---: | ---: | ---: |
| 2,000 explicit binding reads | 1.842 s | 3.582 s | 1.94x |
| 2,000 source-name reads | 1.823 s | 3.528 s | 1.94x |
| 500 binding writes | 0.402 s | 1.350 s | 3.36x |

These are complete benchmark loops, including ordinary loop/arithmetic work,
not isolated dispatch latency. Both prefixes produced the same answers and zero
retained-fact growth in these cases; the baseline's overwrite compaction already
avoided growing its retained-fact count. The candidate additionally separates the
binding authority from Store. Captured immutable slot indices avoid repeated
Integer construction but do not eliminate the cost of the authored Map/Vector
accesses and mutation protocol.

A final sequential Logue comparison, after the other qualification processes
finished, used the same fixtures with normal GC and three samples. Both prefixes
passed the 34-check regression and the checkpoint oracle, including live method
replacement:

| Application phase | Native-backed baseline | Authored owner | Increase |
| --- | ---: | ---: | ---: |
| Sparse arena batch | 141.983 ms | 179.795 ms | 26.6% |
| Dungeon generation | 2,034.401 ms | 2,841.977 ms | 39.7% |
| Replacement scenario batch | 73.025 ms | 96.302 ms | 31.9% |

The replacement case uses its fixed single-sample configuration; the other cases
use three samples. These phase timers exclude loading, setup, inter-batch GC,
snapshots and JSON. Earlier application timings taken during parallel qualification
are not used for this comparison. Neither these results nor the microbenchmarks
justify calling this migration a speed improvement.

The next performance investigation should measure generic authored access and
call overhead in the real application. Any acceleration must preserve current
method lookup and deoptimization; a privileged native Globals registry or cached
dependency Method would undermine this migration. General Map removal is also a
future opportunity: this implementation rebuilds Maps when removing bindings or
banks, so removal scales with their current size.

This does not yet make the full bootstrap Store-free. The seed/legacy adapter,
method publication, compiler metadata and task integration remain separate work.
Store and Relational keep their explicit APIs and self-description.

The [qualification receipt](../measurements/owned-globals-20261014.json) records
source/runtime hashes, native-test coverage, formal counts, logs and benchmark
observations. Qualification ran against the working tree before commit; the
installed release was not replaced.
