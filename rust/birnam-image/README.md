# Image persistence over Id

Bootstrap loads Image after Source Loader. System `saveImage:` writes image-v8
through a same-directory temporary file and atomic rename. `loadImage:`,
`loadImage:rebuild:` and `loadImageBytes:rebuild:` restore through current owner
messages. `loadImageMapped:` requires saved native code and rejects a source-only
image before compiler entry; Host uses it for the installed bootstrap.
Successful mapped loading answers true; rebuilding answers false.
Image validation/file failures answer nil with `imageSaveReport` status/detail;
dependency native failures propagate unchanged. Failed saves preserve the existing
destination. `imageCompositionIdentity` is nil before loading and the existing
64-character composition digest after a successful load.

The graph retains identity, shared and cyclic references, typed ordinary slots,
boxed numbers, Strings and Bytes, persistent collections, Store Cells, CodeObjects
and synchronous/resumable Blocks. CodeObjects retain canonical meaning and source
provenance; Blocks retain their captures. Facts preserve the Store basis, retention
policy and history. Source registration preserves original source objects and the
closed-registration flag. Loading does not evaluate source or rerun initializers.

Image reads ObjectModel-owned slots using `imageSizeOf:` and `imageSlotAt:in:`;
the owner distinguishes raw words from object edges. Restoration uses
`restoreImageSlotAt:in:put:`. Private ordinary Object layouts are reconstructed
from those typed fields. Store owns Cell projection/restoration and its fact
state. Execution owns Block shells and capture restoration; unfinished shells
cannot execute. Map rebuilds address-dependent hashes through `rehashImage:`
after method publication, preserving each restored Map's identity.

Unsupported native state, including Machines and noncanonical capability objects,
is rejected. Authored process-local handles declare a Store fact with
`imageTransientObject = true`; Image rejects any such object reached by the saved
graph. TaskControl marks its handles, including completed tasks. The marker's
attribute is itself `imageTransient`, so unreferenced handles do not enter the
saved graph through that declaration. Detached outcomes remain ordinary data.
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
| `preparePortableImageSources:` / `swapPortableImageSources` | Prepare and exchange the source registry, retaining its predecessor. |
| `finishPortableImageUnit` | Admit complete binding preparation. |
| `installPortableImageBinding:on:as:source:` | Publish through Store's ordinary method protocol. |
| `cancelPortableImageUnit` | Release staging roots. Published Blocks/Methods retain their Units. |

Every generated object is owned by the existing TCC Unit. The private bundle
binds its object count and contents' ordering to the three inventories' digest;
image-v8 additionally checksums the bundle. Compatible loading uses TCC
`loadObject:`. Explicit rebuilding uses `compile:`. Saving uses `compileObject:`.
These are trusted native artifacts, with the same ABI and code-ownership contract
as generated in-memory code.

Image builds facts in an independent Store, roots old callable Methods, then
exchanges Store state using `swapImageStateWith:`. Method publication retains
Store's authorization and observation hooks, including removal of later source
bindings on resident exports that are absent from the snapshot. Failure restores attempted bindings,
the prior source registry and the complete previous Store state. This compensation
does not consume additional fact capacity or reevaluate source. Ordinary external
effects performed by observer callbacks are outside that state compensation.

Alien wrapper recipes retain relocatable objects and target/selector identities.
Image maps and publishes those wrappers inside the same recovery extent, binding
current foreign addresses without compilation. Failed recovery restores their
previous dictionary bindings with the rest of the snapshot transaction.

Map identity keys are rehashed for the admitted Map representation family,
including shared and inherited private behaviors. ObjectModel resolves their
image layouts through registered behavior ancestry.

Managed Image saves validate the generation ledger before snapshotting and again
before file publication. Drift rejects the save and preserves the existing file.

Retirement of saved generation ancestry preserves the first Store error; a
retirement failure rejects and compensates recovery rather than publishing a
partly retired graph.

Generation recovery participates in the same publication transaction. Codegen
suspends old transient candidate capabilities before dispatch changes; the authored
CompiledUnit policy creates composed components and retires saved rollback ancestry
after restoration. Only successful adoption releases the old capability roots.
Returned Errors and native failures compensate facts, sources, bindings and the old
ledger together. Native-free rebuilds derive composition identity from the validated
program, binding and selection inventories.

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
post-write publication failure exercise cleanup and exact Store/source/binding
compensation. Final removal of references returns the restored Unit count to zero.
Codec tests retain all frozen fixtures and reject malformed references/bundles.

Source-defined methods on exported behaviors such as Collection are included in
binding inventories even though those behaviors have no ordinary owner. Restoration
and compensation address their own method dictionaries, preserving inherited methods.

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
