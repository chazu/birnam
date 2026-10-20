# Optional Store participation in Image

2026-10-20. Qualified component contract for completion-plan slice 5. Production
composition remains a later gate. The duplicate native local-Cell representation is already removed.

Core Image asks the current `System imageStore` once per save. The core definition
answers nil. The optional authored `storeImage` integration captures a Store
receiver and defines that message to answer it. Loading that integration is the
explicit opt-in after its native Store provider is loaded; no ambient Globals/Namespace search decides whether facts exist.
The current distribution will select this integration until the production
composition cutover. That transitional selection is not Store-free bootstrap.
The early initializer cannot require the later `#store` Module kernel record;
its native provider is supplied by the explicit bootstrap input closure. The
optional-loading slice must declare that dependency in its final loader composition.

Nil means no Store metadata reads, fact traversal, fact section, staging Store,
or state exchange. A selected owner receives the existing public Store messages.
Its native data revision participates in save consistency alongside the existing
Globals/source/dictionary observations, so retention edits as well as fact writes
can reject a stale save. This observation occurs during snapshots, not ordinary
message execution. An arbitrary replacement of `imageStore` remains an ordinary
user edit; no automatic repair or method-identity guard is introduced.

Use the existing Sources heap-root payload with the new ordinary envelope
`#[#imageOwnersV2 sources foreignOwnerOrNil factOwnerOrNil]`. The fact owner must
have a resident Namespace export, as required by the existing native Store
representation. Its export reference is the image's explicit provider declaration.
A missing export rejects loading before method/fact application. A selected owner
has exactly one retained-facts section, even when its fact count is zero; nil has
none. The existing framing and independent fact validator remain unchanged.
Images with the previous owner envelope must be rebuilt; no compatibility reader
or synthetic empty database is added.

Fact-backed Cells must name the selected owner. Save and load reject a mismatch
instead of silently reconstructing handles whose facts were omitted. Cell shells
are allocated through their recorded owner, never by resolving a default global
Store. Ordinary authored Cells continue to use ordinary object slots and need no
provider. Multiple independently snapshotted Stores are outside this slice and
were not supported by the prior single-Store image transport.

Only selected fact data is prepared in a temporary Store. Publication retains the
existing partial-application boundary: an application failure keeps applied effects
and callback writes; no rollback, retry or checkpoint is introduced. Core image
loading into a world with an optional Store leaves that Store's data untouched.
Merely having Store installed on disk or present in the destination does not enroll
it in a core image load. Deliberately overriding the participation method to nil
opts out of Store snapshots; this is not an implicit fallback on lookup failure.

Acceptance: actual compiled core owners and Image on a floor with Store/Relational
physically absent; mapped/rebuilt/resaved source-free images; explicit facts,
history, retention and Cell aliases; missing-provider and mismatched-Cell refusal;
current provider replacement/errors and collecting callbacks; mutation during
snapshot; existing partial-load and code-lifetime checks. Measure startup and
snapshot costs separately. Complete Host/worker/bootstrap absence remains with
the later production composition slice.


Qualification passed: actual Store-absent mapped/rebuilt/resaved images, positive
optional-Store controls and exact missing-Store refusal, Cell aliases/history,
current private Error propagation, intervening retention/dictionary edits,
foreign-backed hashing, reentrant restoration, partial loads, fixed-live-set
resaves, no-compilation startup, and all 847 language checks. Formal contracts
pass 424 theorems, 135 rejected mutations and 291 expected TLC outcomes.

Three interleaved normal-release pairs show save CPU 33.132 to 33.255 s (+0.4%)
and startup CPU 2.696 to 2.737 s (+1.5%); startup wall rises 1.6%. The benchmark
image grows 80,108 bytes (about 78 KiB, 0.03%). GC policy is unchanged. This is
an architectural separation, not recovery of the previously accepted Alien cost.
See the [receipt](../measurements/optional-store-image-20261020.json).
