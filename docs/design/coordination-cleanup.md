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
