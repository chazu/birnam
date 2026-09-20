# 81BI — Native method-overwrite observation

> **Status:** Initial boundary audit against completed 81BH (`26ae3c02`).
> No implementation or new ABI has been selected yet.

[81D](81d-id-api-partition.md) places higher method observation above Id while
retaining essential dictionary mutation and executable lifetime protection in
the seed. The next candidate is the state and notification behind
`setMethodOverwriteHook`. This extraction must preserve the current single,
nullable callback and its timing; a multiple-observer API would change behavior.

## Current state and callers

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

## Boundary still to resolve

An independently owned upper service could own callback replacement, snapshot
selection and invocation. Id would supply admission for the setter and the
existing direct-binding fact at the current notification point. The service
must not query the dictionary through a new send or reflection callback, retain
a RuntimeState borrow, create new roots, or change the method's native lifetime.
This must move state and behavior, not wrap the old setter under another name.

The registration capability needs explicit incoming selection through native
initialization and Native Loader, including Store reentry. Do not append a
setter to the unrelated enumeration capability or infer it from an API address.
Factory preparation, independent Runtime movement/teardown, canonical internal
notification versus a replaceable public registration operation, and the exact
native ABI remain to be specified before implementation.

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
executables. Commit only after the implemented extraction passes those checks.
No release validation is required.
