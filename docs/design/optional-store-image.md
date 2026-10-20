# Optional Store participation in Image

2026-10-20. The original optional-data component is qualified below. The production
provider cutover and declared-provider extension are implemented and qualified
in the completion plan. Clean release and immutable-archive acceptance passed
for [v0.18.0](https://github.com/chazu/birnam/releases/tag/v0.18.0).

Core Image asks current `System imageStore` once per save. The default answers
nil. `Module load: #store` explicitly activates the native provider and authored
image/fact-reflection integration; `#relational` depends on it. The former
`storeSupport` initializer has been folded into the public Store entry.

Native behavior requirements are separate from fact participation. Module's
ordinary registry marks activated native providers with `imageProvider`; Image
records their dependency-ordered names in v8 record kind 12. Loading sends that
declared closure to current `Module loadImageProviders:` before restoring the
heap. Already-loaded providers are not reinitialized, and missing providers
reject the load. Core images declare an empty closure. No ambient Store probe,
parallel native registry, package download or automatic repair is introduced.

Nil means no Store metadata reads, fact traversal, fact section, staging Store,
or state exchange. A selected owner receives the existing public Store messages.
Its native data revision participates in save consistency alongside the existing
Globals/source/dictionary observations. The current Module owner and registry
root also participate, so changing provider declarations rejects a stale save
just as fact writes or retention edits do. This observation occurs during snapshots, not ordinary
message execution. An arbitrary replacement of `imageStore` remains an ordinary
user edit; no automatic repair or method-identity guard is introduced.

Use the existing Sources heap-root payload with the new ordinary envelope
`#[#imageOwnersV2 sources foreignOwnerOrNil factOwnerOrNil]`. The fact owner must
have a resident Namespace export, as required by the existing native Store
representation. Its export reference preserves the data-owner identity; record
12 separately declares the required native behavior modules.
A missing export rejects loading before method/fact application. A selected owner
has exactly one retained-facts section, even when its fact count is zero; nil has
none. The retained-facts codec is unchanged. New executable images require the
provider record, including when it is empty; older executable images must be
rebuilt. Pure format readers can still inspect earlier framing fixtures.

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
opts out of fact snapshots. It does not unload already-selected native code or
remove that code from the provider declaration. This is not an implicit fallback
on lookup failure.

Acceptance: actual compiled core owners and Image on a floor with Store/Relational
physically absent; mapped/rebuilt/resaved source-free images; explicit facts,
history, retention and Cell aliases; missing-provider and mismatched-Cell refusal;
current provider replacement/errors and collecting callbacks; mutation during
snapshot; existing partial-load and code-lifetime checks. Measure startup and
snapshot costs separately. The [completion plan](store-independent-core-completion.md)
records the subsequent production Host/worker/bootstrap absence qualification.


Original component qualification passed: actual Store-absent mapped/rebuilt/resaved images, positive
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
