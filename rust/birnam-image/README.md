# Image persistence over Id

Bootstrap loads [ImageFormat](../../modules/image-format.birnam), then Image, after
Source Loader. ImageFormat owns complete format encoding and structural validation;
Image consumes it through current Id messages and coordinates live restoration. System `saveImage:` writes image-v8
through a same-directory temporary file and atomic rename. `loadImage:`,
`loadImage:rebuild:` and `loadImageBytes:rebuild:` restore through current owner
messages. `loadImageMapped:` requires saved native code and rejects a source-only
image before compiler entry; Host uses it for the installed bootstrap.
Successful mapped loading answers true; rebuilding answers false.
Compiler-produced source metadata is restored through ordinary compilation and
SourceRegistry owners, independently of explicit Store facts. Both
mapped activation and canonical-program rebuild use it for generation recovery;
neither reparses module source. Images missing this metadata must be regenerated.
Image validation/file failures answer nil with `imageSaveReport` status/detail;
dependency native failures propagate unchanged. Failed saves preserve the existing
destination. `imageCompositionIdentity` is nil before loading and the existing
64-character composition digest after a successful load.

The graph retains identity, shared and cyclic references, typed ordinary slots,
boxed numbers, Strings and Bytes, persistent collections, Store Cells, CodeObjects
and synchronous/resumable Blocks. CodeObjects retain canonical meaning and source
provenance; Blocks retain their captures. Facts preserve the Store basis, retention
policy and history. Source registration preserves original source and compilation objects. Loading does not evaluate source or rerun initializers.

Image reads ObjectModel-owned slots using `imageSizeOf:` and `imageSlotAt:in:`;
the owner distinguishes raw words from object edges. Restoration uses
`restoreImageSlotAt:in:put:`. Private ordinary Object layouts are reconstructed
from those typed fields. Authored local Cells use those ordinary slots; Store
owns its explicit fact-backed CellVt projection/restoration and fact state. Execution owns Block shells and capture restoration; unfinished shells
cannot execute. Map rebuilds address-dependent hashes through `rehashImage:`
after method publication, preserving each restored Map's identity.

Unsupported native state, including Machines and noncanonical capability objects,
is rejected. Image sends current `imagePayload` to each object in the heap graph.
The Object default answers self. False rejects the save; a different Error result
propagates. A distinct payload is accepted only for an ordinary ObjectVt-family
receiver: its ObjectModel-readable typed slots supply the saved body, while the
original identity and behavior remain. Native families retain their existing
codecs and cannot receive arbitrary replacement layouts. Canonical values and
Namespace exports retain their existing reference encoding.

Globals uses this same protocol for its transient-binding projection. TaskControl
keeps a named `processLocal` reference slot: preparation sets it, completion leaves
it set, and its imagePayload rejects marked handles. Unused factory clones and
detached outcomes remain imageable. The per-object Store `imageTransientObject`
query and Image's Globals-specific projection state are removed. Fact participation is explicit through the current `System imageStore` message.
Core answers nil; the optional authored `storeImage` integration answers its
Store receiver. Nil creates no fact section and performs no Store metadata reads,
staging or exchange. The selected owner is retained in the image, and loading
requires that exported provider to be present. Store Cells must refer to that
owner; mismatches are rejected. A fact-free image leaves destination facts alone.
Private behaviors preserve their heap owners, shared instances and
acyclic parent chains rooted in supported native families. Ownerless private
behaviors and additional custom Block behavior layers remain unsupported.
Namespace exports resolve against the resident modules in the receiving process.

## Staging and native ownership

Codegen projects named implementations from current Id dictionaries through
Execution. `portableImageBindings` and `portableImageSources` supply the two
snapshots. Its image stage validates canonical program/binding/selection inventories
and prepares Methods before publication. The relevant operations are:

| Operation | Purpose |
|---|---|
| `compilePortableImageCode:bindings:selection:` | Produce a private bundle of relocatable native objects for saving. |
| `beginPortableImageMapped:programs:bindings:selection:` | Relocate the saved objects without C emission or compilation. |
| `beginPortableImageRebuild:bindings:selection:` | Compile canonical programs using the existing native backend. |
| `portableImageBlockMethod:synchronous:` | Obtain a staged Method with the saved Block disposition. |
| `preparePortableImageBinding:implementation:` | Initialize an owned adapter for the restored implementation Block. |
| `preparePortableImageSources:` / `finishPortableImageSources` | Stage the restored owner identity, then validate it through messages after Globals/Methods/maps are restored. |
| `finishPortableImageUnit` | Admit complete binding preparation. |
| `beginPortableImageApplication` | Invalidate old candidate/preparation/certification capabilities before application. |
| `installPortableImageBinding:on:as:source:` | Publish through the current vtable owner. |
| `cancelPortableImageUnit` | Release staging roots and end application; never restore old capabilities. Published Blocks/Methods retain their Units. |

Every generated object is owned by the existing TCC Unit. The private bundle
binds its object count and contents' ordering to the three inventories' digest;
image-v8 additionally checksums the bundle. Compatible loading uses TCC
`loadObject:`. Explicit rebuilding uses `compile:`. Saving uses `compileObject:`.
These are trusted native artifacts, with the same ABI and code-ownership contract
as generated in-memory code.

When facts participate, Image builds them in an independent Store; it stages executable code before
applying the image. Fresh startup requires discarding the world on failure.
Advanced live loading deliberately has no automatic rollback: application failure
leaves applied changes and callback writes in place. It retains the original
native failure or ordinary Error and reports `#partialLoad` through
`System imageSaveReport`. An application-phase startup failure instead reports
`#startupFailed`. Early admission failures use the ordinary rejection report;
replaceable callbacks are not promised side-effect-free.

Users should save their current image and check success before loading another
when they want a recovery snapshot. No automatic checkpoint, retry or restart is
performed. Recovering from a saved image in a fresh runtime is the user's choice.
External effects and unsaved work cannot be restored by that snapshot.

No prior-method journal, Globals checkpoint, inverse Store/source exchange or
dictionary epoch admission is required by live loading. Failed application clears
composition identity and abandons old preparation/adoption capabilities; staging
cleanup releases temporary roots. Entered/captured code remains owned by Execution.
Explicit candidate undo retains its separate selected-write contract.
Future stronger loading can be an explicit facility without changing this basic
operation or introducing a mandatory coordinator around ordinary edits.

Alien wrapper recipes retain relocatable objects and target/selector identities;
Image maps them using current foreign addresses without compilation. Map identity
keys are rehashed for the admitted representation family, including private
behaviors. ObjectModel resolves image layouts through registered ancestry.

Snapshots record current executable code. `prepareImageSnapshot` checks
imageability of entries at registered authored selectors without reconciling or
mutating generation records. Strict `validateImageGeneration` remains an explicit
certification operation. Restoring code never adopts a generation automatically:
retained records and aliases stay descriptive, while process-local preparation
and rollback capabilities are discarded. `DispatchGeneration adoptCurrent` is a
separate explicit baseline operation. Image still computes its composition
identity from validated program, binding and selection inventories.

No service table, foreign payload access, global GC suppression or candidate
framework is introduced. Roots survive collecting owner calls; entered, suspended
and captured code continues to use Execution/TCC's existing lifetime mechanism.

## Format and validation

Frozen v8 fixtures retain their exact bytes. Additive heap locator 17 stores boxed
Bytes; locator 18 identifies a synchronous Block with the existing CodeObject and
capture representation. Optional record 10, before heap/fact records, references
the ordinary source-registration snapshot. Optional record 11 follows it and
precedes heap/fact records: ordered object/owner heap indices preserve private
behavior sharing, and owner rows name a native family or parent owner index.
Validation rejects missing owners, incompatible layouts and parent cycles before
native mapping. No process pointers are persisted. The runtime ABI marker distinguishes
message-based native artifacts from the retired shim ABI. The private `BIRNAMO1`
bundle belongs to Codegen; Image transports its bytes without reading native code.

`mise run native-messages` includes the production `image` test. It saves once,
loads mapped and rebuilt in fresh processes, then saves the rebuilt state and
loads that file in another fresh process. Mapped tests disable both compilation
entry points. Checks cover shared/cyclic objects, captures, Cells, history, named
methods, shared and nested prototypes, inherited-method replacement, identity-keyed
Maps, and binary embedding after deleting the source asset.
Collection runs after every restored ordinary slot. Partial mapping failure and
post-write publication failure exercise cleanup, retained applied changes and
partial-load diagnostics. Final removal of references returns the restored Unit count to zero.
Codec tests retain all frozen fixtures and reject malformed references/bundles.

Source-defined methods on exported behaviors such as Collection are included in
binding inventories even though those behaviors have no ordinary owner. Restoration addresses their own method dictionaries, preserving inherited methods.

Host/CLI image entry now restores global main; see the [Host contract](../birnam-host/README.md).
Installed entry and baking checks live with Host; optimizer work remains separate.

Numeric Image coverage includes nonzero Words through the unsigned maximum,
Integer conversion boundaries and overflow Errors, negative zero, a NaN payload,
infinity and the largest finite Float. Save/restore uses Word's `bits` and
`fromBits:` messages. The native Image test checks exact payload bits, shared
identity and Error kind/message after mapped, rebuilt and resaved loading.

The Image group also chains twenty fresh-process mapped/rebuilt resaves. It
checks stable retained facts, native Unit counts and image sizes for each load
mode, then releases every native Unit after the last roots disappear.

## Startup admission

`System startImage:rebuild:`, `startImageMapped:` and
`startImageBytes:rebuild:` require their caller to discard the startup world on
failure. They validate/stage normally and never run publication compensation. CLI bootstrap, standalone/baked image entry and workers
use this path and do not enter application code after failed admission.

`System loadImage:` and its existing variants remain explicit live restoration.
A caller that may inspect the current world after failure must use that contract
and accept partial application. See
[basic live loading](../../docs/design/basic-live-image-loading.md).

Generation keeps only an application-in-progress flag, not a recoverable copy of
old capabilities or a composition-digest recovery handshake. A staging failure
before application leaves existing capabilities intact. Once beginning application
is attempted, failures are conservatively reported as partial application, including
a replaced begin method that invalidates capabilities and then returns an Error.
The image format still validates its code/inventory checksums independently.

The owner-root payload in the existing Sources section is
`#[#imageOwnersV2 #[#sourceRegistryV2 sourceOwner] foreignOwnerOrNil factOwnerOrNil]`.
It references the same ordinary owners restored through Globals and user aliases;
there is no second registry inventory or hashed-name rediscovery during application.
Previous owner envelopes are rejected before application and must be rebuilt.
The outer v8 framing and retained-facts codec are unchanged. See the
[optional participation contract](../../docs/design/optional-store-image.md).

After ordinary bindings are installed, Image asks the foreign owner for its projected
recipe rows using captured-receiver preparation, installs foreign bindings through
Alien's public Id protocol, repairs Map hashes, and finally invokes authored indexing.
This order supports hashes based on destination identities or foreign functions.
New Methods are temporary operation roots until indexing; persistent registration
metadata contains only Id stamps. Foundational Error predicates capture canonical
boolean values so the default checks do not resolve Globals during this interval.

Snapshot admission still observes the source owner and its content revision, so
registration during saving cannot silently publish mixed source state. No ordinary
invocation is guarded by recipe or Method identity checks.
