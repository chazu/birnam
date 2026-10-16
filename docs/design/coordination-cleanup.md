# Coordination cleanup

Follow-up to [ordinary mutation](minimal-mutation-and-coordination.md).
Each implementation slice is committed separately.

## 1. Opt-in dictionary observation

Store construction no longer sends Id `methodChangesTo:`. Ordinary dictionary
edits therefore have no Store observer by default. The existing Id registration
message remains available for an explicit live consumer; Store's
`methodChanged:on:` can be selected explicitly to invalidate its mirror rows.
Neither `backfillMirror` nor an explicit Store method edit subscribes implicitly.
Descriptions without a subscription are snapshots, not promises of live lookup.

Default ordinary method descriptions were already disabled. Mirror rows are
nondurable and are not imported by Image. Store/Relational self-description and
user facts are retained. Mirror cleanup now checks the owned `mirror` flag before
retiring a row, including when an explicit fact has an identical triple.

Validation: Store unit regression for identical explicit/mirror facts; native
Store and publication tests explicitly opt into observation; the Id observation
test checks the empty bootstrap slot, replacement, reentry, collection and failure.

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
