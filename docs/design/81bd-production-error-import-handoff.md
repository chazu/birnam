# 81BD — Error Value borrows the caller's import capability

> **Status:** Implemented and debug-validated, following publication in 81BC.

Move Error Value's two import consumers onto an explicit per-call Namespace
import descriptor. Recognition reads it only after the null-value short circuit
and successful live ErrorVt presence check. Construction reads it after kind
interning, String construction and successful Error presence. Import's returned
behavior/factory remains a snapshot across the subsequent vtableOf or selector
intern/send callbacks. No descriptor, context, result or byte buffer is retained.

Namespace import v1 is a 32-byte live function/context descriptor. Snapshot its
function and context together at invocation; neither descriptor validation nor
handoff invokes it. Earlier callbacks can change its selection, and nested calls
can use different API, presence and import capabilities. Error Value v3 adds its
pointer after presence in both operations, retaining a 32-byte immutable provider.
All nine consumers, host discovery, manifests and current fixtures migrate to v3.

As with 81BA's first consumer handoff, callers still own mutable IdApi slots at
this checkpoint. Their synchronous adapter lends the address of the one import
slot; it does not copy the function early, accept an aggregate table in the
recipient, or register API addresses. The loan is valid only through the helper
call and is not saved. It selects the exact table already chosen at the call site,
including incoming registration versus first-saved execution tables.

This is the actual shared-consumer migration, not removal of IdApi.import or
canonical import's C wrapper. Native initialization and installer/preparation
transport are later work. Do not add a global import dependency or aggregate
compatibility table. No new language functionality, objects, roots, safepoints,
methods, numeric declaration operands or image encoding are introduced.

Preserve negative presence flags, raw-null behavior/factory handling, current nil
selection, kind/message evaluation, callback order, reentry, GC, identity and
reload. Check independent old/current native transcripts for the nine consumers,
v3 admission and live slots. A separate native oracle must supply import while
IdApi.import aborts, change function/context during earlier callbacks, reenter
with another selection and prove both short circuits and result snapshots.
Verify dynamic startup, a source-free compiled caller, saved native Code image
and baked execution in a fresh debug prefix, then commit. No release validation.

## Validation

- Workspace all-target check and fresh debug installation passed. ABI and host
  library tests passed (95 total, one intentionally ignored); Error Value has
  no Rust unit tests. Strict ABI/Error Value Clippy and strict syntax checks of
  54 current C fixtures passed.
- All nine real consumers reject null, v2, malformed and incomplete v3
  descriptors and retargeting, and accept repeated attachment. Old v2 provider
  and dependency symbols are absent. Current host discovery selects v3.
- Sixteen complete old/current native transcripts match exactly, both for
  development and installed builds. Independent baseline clients are compiled
  from `ebc3b2c7` with their original Error Value v2 header and libraries; current
  clients use v3. Coverage includes Error Value's 114 cases, Integer's 395,
  Float's 373, Word's 24, Environment/File IO/Input's 680 each, Native Loader's
  1,020, Source Loader's 524 and Block Loop's 980, with three registration/reentry
  modes for each loader/loop. No old/new descriptor cast is used.
- The actual File IO error path preserves replacement of its saved execution
  API's import slot during kind interning, String construction and presence.
  Those same three additions also pass through the original v2 baseline and
  preserve its complete trace. This tests the production Rust slot adapter,
  independently of the direct descriptor fixture.
- The new independent native import oracle passes while IdApi.import aborts.
  It checks lazy null/absent short circuits, null factory/behavior results,
  negative presence, function/context replacement at all three earlier callbacks,
  nested independent API/capability selections, current nil after callback
  replacement, and imported factory/behavior snapshots after later mutation.
  Both provider operation addresses resolve to the loaded Error Value dylib.
  `scripts/error-value-test.sh` runs this probe twice as part of the durable gate.
- `error-import-image-smoke.sh`, `error-presence-image-smoke.sh` and
  `error-value-image-smoke.sh` pass dynamic startup, source-free compiled callers,
  saved v8 native Code images and baked executables. They cover explicit import
  and presence, real Error factory/method replacement, aliases, precise GC,
  repeated initialization and saved-versus-incoming Lang/Tcc API selection.
- Numeric declaration context/copy/mutation/restoration checks still pass.
  All 145 installed standard-library assertions pass.
- Fresh debug prefix:
  `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81bd-prefix.xiblctdx`.
  All 1,810 source hashes stayed unchanged through installation. Only the ABI
  README consumer summary and this validation record changed afterward.
- All 71 old/new native/authored library inventories match except Error Value's
  provider and nine consumer dependency v2-to-v3 renames. The ten affected
  manifests, declared install names and system-only linkage pass. Integer keeps
  its existing absolute debug install name. No release validation was run.

Receipts are `.cargo-target/81bd-{check,unit,clippy,c-check,final-error-value,
native-audit,installed-native,slot-baseline,import-oracle,install-driver,inventory,
import-image,presence-image,error-image,stdlib}.log` and `81bd-source-hashes.json`.

## Next boundary

Move canonical C-name import into Namespace and carry each caller's live import
selection through native initialization, saved execution state and installers.
Account for direct consumers, ForeignAddress, compiler bridges, copied APIs and
callback-free numeric/Block declaration operands before removing IdApi.import.
The synchronous slot loan is transitional and must disappear with that field.
IdApi remains v9 (32 fields, 256 bytes), and Namespace factory remains v6.
