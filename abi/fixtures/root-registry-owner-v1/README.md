# Initialized root registry v1

`root_registry_owner_v1.h` independently declares the 72-byte owner returned by
`libbirnam_root_registry.dylib` through `birnam_native_module_v1`. The descriptor
uses the common native protocol: initializer arguments are Object roots, and its
result is a Word containing the descriptor address. Invalid operand kinds fail
before allocation/publication. No Runtime, GC, language values or prepared Id
dependency is needed to initialize it.

The owner retains the existing root-provider API and factory v1 plus:

- `retain(context, object)`: retain a non-null Object and return a nonzero token;
  null returns zero. Register the root before publishing the object in module state.
- `release(context, token)`: remove that reference; return 1 once, otherwise 0.
  Tokens belong to this owner and are never reused; released records are removed
  so storage tracks live roots rather than the number of past retain calls.
- `set_admission(context, floor)`: replace the current, working mutation admission
  policy with a complete supplied policy; invalid descriptors return 0.
- `roots(context)`: return the current retained references, in unspecified order.
  This canonical snapshot invokes no policy callback and works during collection
  or observation. The view is borrowed until mutation; copy it before callbacks
  or nested calls.

Operations are owner-thread only. The initial mutation policy checks that thread.
After Runtime is complete, it installs its storage-observation admission policy;
root registration/retention never waits for that later policy to become usable. Transfer ownership once, retain the module's
code, and eventually call `factory->destroy(api)`; this destroys both native root
records and the descriptor. Destruction does not free Id objects or run their
finalizers. Native pointer arguments and callback lifetimes remain the caller's
responsibility.

The installed `scripts/id-native-loading-smoke.py PREFIX` exercises the actual
production prefix with only Id and this module available. Independent C retains
an otherwise unrooted object before Runtime exists; the same registry is adopted
by ordinary host construction, and collection observes retention and release.
This is root ownership only. Complete Lifetime payload/finalizer ownership and
collector activation are specified separately in Design 83.
