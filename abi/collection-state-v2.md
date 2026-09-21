# Collection state v2

`libbirnam_collect.dylib:birnam_native_module_v1` constructs the production mark
records, allocation budget, collection phase, native stack anchor and GC settings
before Runtime exists. It takes six Words: completed Native Stack factory v3,
GC Settings factory v1, Allocation Budget factory v1, Trace graph, Lifetime and
Object Model descriptors. Those three
modules now publish their existing factories through zero-argument native entries.
Collect supplies its own existing phase implementation.

The initializer enumerates the actual Id allocations through `allocationsDo:`.
It records each exact payload address as initially unmarked and collectable, and
charges the existing allocation budget with its header plus padded payload bytes.
It copies no object, changes no allocation identity and invokes no finalizer.
The mark records are weak metadata, not roots. Creating these records does not
activate collection or install construction wrappers.

The Word result points to the [128-byte owner](fixtures/collection-state-v2/collection_state_v2.h),
mirrored by [`collection_state::OwnedV2`](../rust/birnam-abi/src/collection_state.rs).
It contains the records table, completed stack/settings tables, canonical
factories and budget/phase contexts, a complete admission-policy replacement,
native destruction, the completed stateless collection coordinator, and the
[collector capability](collector-v1.md). Runtime
adopts that coordinator immediately; later legacy mounting is an idempotent check.
The owner destroys each control exactly once. Runtime's
consumers share that ownership and borrow the existing controls; none creates a
second production table or resets the early settings, marks or accumulated budget.

The 80-byte records table supports exact-address insertion, flag queries,
permanent retention, first marking, mark clearing, snapshots and removal of dead
collectable records. Flags are `MARKED = 1` and `PERMANENT = 2`; an unknown address
returns `UINT32_MAX`. Null/duplicate insertion, invalid flag bits and removal of
marked/permanent records fail without changing an existing record. Interior and
unknown addresses do not mark. Snapshots contain 16-byte records (Object, flags,
reserved zero); copy the view before any further operation on that owner.
Record removal releases only metadata, never the Id allocation or finalizer.

All operations require the constructing thread and live native code. Record and
canonical control operations do not call application code. Public stack/settings
mutations initially enforce owner-thread access. After Runtime construction,
`set_access` replaces that working policy with its complete observation/admission
policy. It does not fill a missing provider. No Execution or Runtime pointer is
needed to construct or use the early owner.

`InitialFloor::initialize_collection` builds this owner after the Trace graph.
The temporary `birnam_runtime_init_v8` bridge transfers it to Runtime, alongside
the existing completed owners and graph. Adoption retains its existing records
and budget, then registers and charges only Id allocations made since the early
snapshot. Subsequent production allocation, deferred settlement, marking,
finalization and reclamation all update these same records. Settings and stack
providers return the exact original capabilities. Normal startup skips the four
legacy factory preparations inside Id; legacy `id_init` clients retain them.

Native register capture is also available through the completed stack factory;
Runtime uses it for collection and source probes.

This completes the state ownership portion of Design 83 step 9. The explicit
collector activation transition and construction wrappers still need to move out
of aggregate Runtime. The collector now owns tracing,
Lifetime/Object Model/Id retirement, detachment, finalization and release;
Execution supplies only its scoped roots and retirement operations. Until
activation is implemented, early native allocations are non-collecting and their
record registrations are reconciled at tracing and Runtime adoption. The coordinator takes complete
scoped operations per call; the early owner advertises no heap-collection operation depending on a future service.

`scripts/id-native-loading-smoke.py PREFIX` exercises the installed production
owner with only its twenty-two predecessor libraries available. Independent C
checks the initial inventory/budget, mark and permanence rules, phase/settings
and stack operations, preservation through adoption, later allocation accounting,
and removal before actual finalizers run. Releasing a retained root permits
reclamation through that same owner.
