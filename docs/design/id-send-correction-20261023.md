# Restore the Id send contract

Implemented in **`683c9fc2`** on the isolated `codex/oop-only-id-slice` branch.
The [complete work summary](logue-runtime-work-summary.md) connects this correction
to the original Logue investigation, prior decisions and remaining migration.

The `apply` protocol was an implementation mistake, not an approved design choice.
It has been removed from Id and from the migrated native owners. Historical logs
that described it as architectural compliance are retracted in the migration log.

## Message path

Before: send, receiver method lookup, another lookup on the selected Method,
application dispatch, then the selected implementation.

Now: send, current receiver method lookup, then entry into the selected Method.
The bootstrap Method owner reads its declaration, context and code owner and keeps
them alive through the call. Generic dispatch does not classify arbitrary payloads.
The existing explicit captured-Method reflection operation consumes the bootstrap
owner's own argument Sequence; it is not a universal callable protocol.

Removed the `apply`, `apply:to:` and `apply:to:with:` bindings and implementations,
the dependent `appendTo:` forwarding operations, the extra native-Method metavtable,
application checks in Method comparisons, and private callback/evaluator Methods
created solely to support that protocol. Byte-consuming operations and evaluators
are local functions again. Byte copying uses ordinary `sizeWord`/`atWord:` data
messages. Their owning implementations provide the values; consumers never read
foreign payload layouts. The I/O owners register their existing dependency
Sequences as root providers for scoped temporary references.

This restores correctness without claiming an application speedup. Indexed byte
copying performs work proportional to byte count, including sends. Its costs must
be measured during the planned performance work; the rejected callback protocol
must not be restored or renamed to avoid that work.

## Enforcement and evidence

The cold-process dispatch test confirms that Id has no `apply` binding. It then
makes all lookup on the selected Method fail: an ordinary send still succeeds.
This checks behavior independently of the name of an attempted second protocol.

`bash scripts/oop-id-slice-test.sh` passes. It covers Id unit tests, independent C
and TinyCC callers, live/global/metacircular lookup, DNU, replacement, entered and
captured Method lifetime, GC/finalization/retirement failures, allocation policy,
value/collection integration, binary I/O, PEG and artifact inspection. Test fixture
delegation uses ordinary sends; no argument allocation occurs inside the
allocation-accounting callback. A source audit finds application selectors only
in the negative contract test, with no production implementation or forwarding
family remaining.

[Qualification receipts](../measurements/id-send-correction-20261023.json) record
these checks. Full compiler/execution/image/worker migration and Logue qualification
remain unfinished. This correction is in the isolated worktree, not installed.
