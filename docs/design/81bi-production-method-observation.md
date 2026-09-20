# 81BI — Native method-overwrite observation

> **Status:** Boundary audited against completed 81BH (`26ae3c02`);
> implemented and validated with debug native and compiled-image checks.
> Method Observation owns callback storage and invocation; Id retains mutation.

[81D](81d-id-api-partition.md) places higher method observation above Id while
retaining essential dictionary mutation and executable lifetime protection in
the seed. This slice extracts the state and notification behind
`setMethodOverwriteHook`. This extraction must preserve the current single,
nullable callback and its timing; a multiple-observer API would change behavior.

## Audited 81BH state and callers

`RuntimeState.method_overwrite_hook` starts as None. The public setter selects
the process-published Runtime, performs its normal owner/observation admission,
then replaces that slot without allocation or invocation. Null unregisters;
repeated or different callbacks are accepted. The slot is per Runtime, not an
Oop root or a source of native-code ownership.

Store is the only production setter caller. Its bring-up retains the first
execution Runtime, but hook registration uses the **incoming** API. It runs
after method/Cell/Globals installation, Store-root publication and root-provider
registration, and before seeding and namespace publication. The atomic once flag
is set before invoking the registration callback. Callback reentry can therefore
make a different incoming registration win; substituting Store's saved API here
would change behavior. Reload does not independently re-register the hook.

Store's callback reads the published Store root, finds the current mirror fact
and retracts it if present. It is separate from Store's method authorizer and
post-mutation observer. None of those services should be folded into this slice.

## Notification placement

`Runtime::add_method` and `Runtime::remove_method` each perform exact direct
lookup before copying the hook. Lookup occurs even when the hook is absent, so
an invalid vtable must still fail. A new or inherited-only binding does not
notify. A direct replacement notifies even when the method address is unchanged;
removing a missing direct key does not notify.

The callback receives the original vtable/selector identities and sees the old
dictionary. No RuntimeState borrow survives invocation. Dictionary mutation,
direct-binding reference accounting, Block-cache invalidation and staged
publication journaling happen afterward in their current order. Keep that
accounting in Id; an optional observer must not become its authority.

Raw dictionary mutation is not universally observed. In particular, staged
publication rollback removes/restores bindings directly while repairing its own
accounting. Moving notification into `Vtables::add_method` or `remove_method`
would add callbacks there and during seed construction. Preserve the current
two Runtime notification sites.

## Selected boundary

An independently owned Method Observation service owns callback replacement,
snapshot selection and invocation. Id supplies admission for the setter and the
existing direct-binding fact at the current notification point. The service
must not query the dictionary through a new send or reflection callback, retain
a RuntimeState borrow, create new roots, or change the method's native lifetime.
This must move state and behavior, not wrap the old setter under another name.

The registration capability receives explicit incoming selection through native
initialization and Native Loader, including Store reentry. Do not append a
setter to the unrelated enumeration capability or infer it from an API address.
The immutable factory creates and destroys an owned service and supplies its
internal notification operation. The mutable public capability exposes only
callback replacement; replacing its context or operation does not retarget the
owned notification state. The factory's notification receives the original owned
address and the direct-binding fact already checked by Id, snapshots the hook
and invokes it without holding any state borrow. The setter copies its floor
admission callback, invokes it, then replaces the slot. Destruction invokes no
hook and uses the owned address even after public capability mutation.

The host pins the new library and attaches its inert factory before `id_init`.
Each Runtime creates its service with a copied floor descriptor; no movable
Runtime address is captured. The ordinary native module mounts after Object
Model and before Store, publishing no language methods or objects. Normal Id
has no Rust dependency on the upper implementation; unit tests use a dev
dependency to exercise the same service against independent Runtime values.

Native initialization gains a seventh, explicit Method Observation capability.
Store registers through the incoming selection at the original call point.
Prelude/Native Loader retain it with their original execution selection for
direct loading and reentry; Native Loader's installer becomes v7. IdApi v14
removes its last setter field (26 fields / 208 bytes) and its obsolete hook-type
layout entry. The callback type now belongs to the observation contract. Image
encoding, numeric and Block-constructor declarations remain unchanged.

The v1 arm64 layouts are: Floor 32 bytes (context, admission); public capability
32 bytes (context, replace); immutable factory 40 bytes (create, destroy,
notify). Creation and factory admission are inert and reject wrong versions,
extents or missing operations. Same-pointer factory attachment repeats;
retargeting rejects. Public callback replacement retains the old unrestricted
replace/clear law rather than adopting that factory attachment law.

The adjacent `symbolName` field is currently a borrowed Id storage fact with
active-Runtime selection and a rooted-Symbol lifetime obligation. Symbol printing
already lives in Printing (81AI). Simply forwarding that raw borrow through
Object Model would not extract another implementation responsibility; it needs
its own representation/lifetime review rather than mechanical field removal.

## Required evidence

Preserve independent 81BH code/headers and its final debug prefix. Compare
registration, overwrite/same-address replacement, removal, inherited/missing
bindings, null unregister and invalid-target diagnostics. Cover Store's incoming
registration versus saved execution and once-flag reentry, old-dictionary reads
inside the callback, independent services and silent rollback. Existing
`overwrite_hook_runs_before_replacement_and_removal_without_state_borrows`
provides one focused oracle; it is not a complete caller or lifetime proof.

Then verify actual service ownership, current ABI/consumer migration, siphon
startup, source-free compiled callers, saved native Code images and baked
executables. Mark the extraction complete only after it passes those checks.
No release validation is required.

## Completion evidence

The initial checkpoint `fcd36b54` was explicitly unfinished. This completion
migrates current C headers/fixtures/runners, export manifests, build prerequisites,
Error Value's native initializer, and the matching Rust/host callers. Historical
IdApi v13 and Native Loader v6 headers/manifests remain unchanged.

- Workspace/all-targets check and affected-package all-feature checks passed.
  The affected unit suites pass 370 tests, with 18 existing opt-in tests ignored.
  Coverage includes inert factory admission, copied floor lifetime, independent
  services, replace/clear, public descriptor replacement, notification reentry,
  Runtime movement/destruction, and notification-free publication rollback.
- Fourteen exact old/new native comparisons use the archived 81BH source/header
  tree and final debug prefix as the independent oracle. They cover direct,
  same-address, inherited/new/missing bindings, replacement/clearing, old-dictionary
  reads, roots/GC, invalid targets with and without a hook, owner/observation
  admission, Store incoming versus saved execution, and both once-flag reentries.
- Sixteen existing native consumer transcripts also match 81BH byte for byte.
  Native Loader explicitly checks the seventh borrowed pointer through nested
  loads, reload, direct installation and initial registration reentry.
- Sixteen real host admission cases cover absent/malformed factory and capability
  providers, attachment rejection and a usable production capability. Nine floor
  discovery/cache cases pass. The independent v14 C client reports 26 fields and
  successful repeated initialization with all seven arguments.
- The debug prefix contains 44 native libraries and 28 compiled authored modules.
  All 72 Birnam library export inventories match the declared changes. The new
  module has its exact three exports, relocatable install name and system-only
  linkage. Both callback replacement and notification addresses resolve to it;
  normal Id has no dependency on its Rust implementation.
- `method-observation-image-smoke.sh` passes ordinary startup, a compiled caller
  after deleting its source, a saved v8 image with native Code, and a baked
  executable. It checks same-address and different-address mirror retraction,
  raw removal, stable Store basis, GC, aliases and reload. Loader traces identify
  the new module in startup and both image modes. Selector enumeration, vtable
  enumeration, Namespace, Native Loader and Block Loop image suites also pass.
- The installed stdlib suite executes 145 assertions with zero failures. Actual
  image-backed 81BH/current workers complete twelve exact wire round trips and
  orderly shutdown with identical transcripts. The external debug harness waits
  for readiness; it does not change the production handshake deadline.

The first image probe incorrectly compared freshly boxed basis Integers by
pointer identity. The corrected probe roots the saved value and compares it
numerically; all four execution modes pass. No runtime change was needed.
The initial stdlib invocation only registered its suite; the final receipt
explicitly runs the suite and verifies its allPassed exit status.

Receipts are `.cargo-target/81bi-{unit,abi-final,consumer-units,check-final,
features,native-check,native-audit,host-validation,inventory,idapi-client,
floor-discovery,stdlib,worker-wire}.log` and the six `81bi-*-image.log` files.
The final installed prefix is recorded in `.cargo-target/81bi-prefix-path`;
`81bi-install-driver.log` records 1,921 unchanged source hashes at installation.
Only the image test fixture changed afterward, for the rooted numerical
comparison above; installed runtime/build sources stayed unchanged.
No release validation, image-format change or performance claim is included.

## Next boundary

Audit the native root-provider registry in Id's `gc::MetadataShell`. The ordered
callback list and snapshot operation are a distinct state owner that can move
above Id without moving native stack capture, automatic collection policy,
object metadata retirement or physical storage. Preserve duplicates, the exact
32-provider limit/diagnostic, snapshot timing and provider code lifetime. Do not
turn this into a new root-unregistration or ownership API. Symbol bytes remain a
seed-owned representation/lifetime question, not a forwarding-only extraction.
