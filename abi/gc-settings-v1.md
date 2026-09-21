# Native GC settings v1

Design [81BK](../docs/design/81bk-production-gc-settings.md) moves the two GC
flags and the existing automatic-collection predicate into `birnam-gc-settings`.
The [independent C mirror](fixtures/gc-settings-v1/gc_settings_v1.h) fixes three
arm64 layouts: 40-byte public capability, 32-byte copied floor descriptor and
64-byte immutable factory.

The host pins `libbirnam_gc_settings.dylib` and attaches
`birnam_gc_settings_factory_v1` through Id's `birnam_gc_settings_dependency_v1`
before `id_init`. Validation and creation invoke no floor operations. Invalid
descriptors reject; attaching the same pointer repeats and retargeting rejects.
Each Runtime owns an independent instance, initially automatic on and precise
off. Id publishes its public capability through `birnam_gc_settings_provider_v1`.
Ordinary module mounting creates no objects or methods. Production Id does not
link the settings implementation.

The public setters copy their selected operation/context on every call. The
implementation copies its floor descriptor, performs ordinary owner/observation
admission, then assigns the flag. No settings borrow crosses admission; reentrant
assignments finish before the outer assignment. Every nonzero C integer enables
the flag. Setters do not collect immediately, reset accounting, impose a phase
guard, count nested suppression or restore previous settings.

The factory's internal setters, due predicate, precise query and destruction use
the original owned address. Public descriptor instrumentation cannot redirect
them. These operations invoke no callbacks. Id performs existing admission before
its internal setters and keeps its native stack anchor and allocation accounting.
The due predicate returns 1 exactly when automatic mode is enabled, supplied
stack readiness is nonzero and supplied allocated bytes are at least 16 MiB.
Queries return canonical 0/1 C integers. Id preserves the existing read points,
including every-object precise selection and restricted storage observation.

Native initialization takes nine arguments, ending with the settings capability.
Descriptors, contexts and implementation code outlive every retaining consumer
and the owning Runtime. Prelude, Codegen and Image retain their first matching
execution selection; [Native Loader v9](native-loader-v9.md) forwards it and
[System GC v2](system-gc-v2.md) installs its controls with it. IdApi v16 removes
the two old setters. Early value preparation and image encoding are unchanged.

Run `mise run gc-settings` and `scripts/gc-settings-image-smoke.sh PREFIX` for
focused debug checks and installed startup/compiled-image coverage.

Current production constructs and owns these controls in the early
[collection-state owner](collection-state-v1.md), then Runtime adopts the same
context. Normal startup no longer prepares this dependency inside Id. The
existing factory ABI remains available for legacy clients.
