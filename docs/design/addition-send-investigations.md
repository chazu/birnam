# Remaining addition-send investigations

The four investigations were executed in order from the accepted 177-send
addition baseline. Each has its own findings, targeted experiments, proof updates,
measurement receipt and commit. This is an investigation record, not a claim that
all remaining infrastructure has been eliminated or that the send floor is proven.

| Investigation | Result | Sends after |
| --- | --- | ---: |
| [Invocation/captures](invocation-capture-investigation.md) | Shared guard implementation; remove unused name-service dependencies. Each bulk proof checks six rows instead of eight. An invocation environment still needs a carrier/invalidation/lifetime design. Commit `1e6d2833`. | 177 |
| [Integer–Word conversion](integer-word-conversion-investigation.md) | Constructor/getter replacements retain temporary identities and change results. Add regression experiments and compiler-region obligations; do not remove boxes unconditionally. Commit `1bb4aa9f`. | 177 |
| [Result lifetime handoff](result-lifetime-handoff-investigation.md) | Reproduce and fix fresh-result collection during custom cleanup. Owner scope preserves coverage/order and batches small groups. Commit `401eef72`. | 175 |
| [Representation queries](representation-batch-investigation.md) | Batch same-segment operand metadata in Id, preserve ordered fallback and constructor validation. Committed with this summary. | 171 |

Previously validated uncommitted work was first recorded as baseline commit
`e35b6bd9`. All cold-bootstrap Rust implementations remain. Current method
replacement, Id dispatch, owner boundaries, aliases, Error precedence and active
code/input lifetimes continue to have focused regression coverage. New cleanup
and metadata providers must honor their explicit protocol contracts.

Final fresh-prefix acceptance passes 22 Rust/C checks, 26 native checks across
21 binaries, fourteen task assertions at quanta 128 and 1, all 146 Lean theorems
and all 94 TLA+ configurations including required unsafe counterexamples. The
saved-image startup gate passes. The bounded warmed census records 171 sends
and three Words plus one Integer allocated per addition. This is six fewer sends
(3.4%) than the starting point; no CPU speedup is inferred from counts. The
exhaustive workspace matrix was not rerun.

The larger remaining opportunities are admitted invocation environments and a
compiler region spanning Integer conversion/helper boundaries. Both require more
than local send deletion. Source-level nonescape does not prove that constructors,
getters, frames or callbacks cannot expose an Object. A future implementation
should use the conversion observability fixtures and existing recovery/permission/
owner models before enabling a fast path. Separate constructor checks also remain
until a current certified-allocation handoff can replace them safely and cheaply.

Existing images with machine code retain their embedded generated helpers and
are not recompiled on load. Fresh authored artifacts and newly emitted code use
the cleanup fix and current optimizations.
