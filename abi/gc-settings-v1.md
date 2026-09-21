# Native GC settings v1

Design [81BK](../docs/design/81bk-production-gc-settings.md) moves the two GC
flags and the existing automatic-collection predicate into `birnam-gc-settings`.
The [independent C mirror](fixtures/gc-settings-v1/gc_settings_v1.h) fixes three
arm64 layouts: 40-byte public capability, 32-byte copied floor descriptor and
64-byte immutable factory.

Id loads `libbirnam_gc_settings.dylib` through its common native entry, with no
initializer operands, and receives its completed factory as a Word. The early
[collection-state owner](collection-state-v2.md) creates and owns the controls;
Runtime adopts that same context. Host selection uses the completed `gcSettings`
[Runtime service](runtime-construction-v15.md#completed-service-selection).
Instances start with automatic collection enabled and precise mode disabled.
There is no pre-Id preparation, factory/provider export or dependency setter.

The public setters copy their selected operation/context on every call. The
implementation copies its floor descriptor, performs ordinary owner/observation
admission, then assigns the flag. No settings borrow crosses admission; reentrant
assignments finish before the outer assignment. Every nonzero C integer enables
the flag. Setters do not collect immediately, reset accounting, impose a phase
guard, count nested suppression or restore previous settings.

The factory's internal setters, due predicate, precise query and destruction use
the original owned address. Public descriptor instrumentation cannot redirect
them. These operations invoke no callbacks. Runtime performs existing admission before
its internal setters and keeps its native stack anchor and allocation accounting.
The due predicate returns 1 exactly when automatic mode is enabled, supplied
stack readiness is nonzero and supplied allocated bytes are at least 16 MiB.
Queries return canonical 0/1 C integers. Runtime preserves the existing read points,
including every-object precise selection and restricted storage observation.

Descriptors, contexts and implementation code outlive every retaining consumer.
Native clients receive this capability as an explicit constructor input;
[System GC](system-gc-v2.md) exposes language controls above it.

Run `scripts/gc-settings-test.sh PREFIX`, or
`scripts/runtime-services-test.sh PREFIX` for the combined service tests.
The C oracle uses the production startup path and checks independent factory
state, precise-mode conversion, automatic collection thresholds, suppression,
stack readiness and exact owner/observation rejection diagnostics.
`scripts/gc-settings-image-smoke.sh PREFIX` is the separate image coverage gate;
its legacy fixture migration remains tracked by Design 83.
