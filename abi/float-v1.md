# Native Float dependency v1

`birnam-float` owns Float's canonical behavior, zero prototype and eight-byte
IEEE payload. It depends on IdApi v7 and explicitly acquires Integer for mixed
operations and conversions. Id initializes without Float; its still-combined
Machine and image/recovery consumers acquire the family through
`birnam_float_dependency_v1`. There is no Float compatibility facade in IdApi.

`FloatApiV1` is a 72-byte C table: version, byte size, stable native context,
behavior and prototype identities, constructor, reader, exact recognizer and
pointer to an immutable 48-byte `FloatStorageV1`. That codec declares payload
size/alignment and bit-preserving initialization/read callbacks. See the
[independent C declaration](fixtures/float-v1/float_v1.h).

Ordinary consumers copy the current operation table before calls. Construction
reads the current Id allocator. Recognition uses the current checked vtable
operation and accepts only the canonical behavior; native payload reading keeps
its unsafe live-payload contract even after behavior reassociation. Id's
internal reads and graph materialization instead use the admitted pure codec,
without callbacks into Id, allocation, GC, reentry or mutable module state.
Graph materialization retains its original allocation/root schedule. Float has
no native numeric optimization declaration or allocator-context guard.

## Mounting and lifetime

The host pins Float's library and calls `birnam_float_prepare_v1(IdApi*)` before
initializing Integer. Preparation registers partially initialized roots before
allocating, then returns a complete primitive table. The provider returns null
until preparation completes. Attach that table to the host, combined runtime
and Integer before Integer's `module_init`. After Integer publishes its table,
attach Integer to Float and call Float's ordinary `module_init`: this performs
owner association, exports `Float`/`FloatVt`, and installs the seventeen methods.
Other modules declare `birnam_float_dependency_v1` when they consume Float.

Both numeric tables are usable when Float's owner/export callbacks reenter.
Repeated initialization retains canonical identities and namespace bindings,
but reinstalls the seventeen methods as the method-only module did. A different
Id table, recursive initialization, invalid dependency or different mounted
family is rejected. Successful attachment of the same pointer is idempotent.
Library handles and native instances remain live for every published root,
method, codec and context, including after a startup failure. No unload rollback
is permitted after callback publication. Test runtimes own separate instances
with explicit local Id adapters instead of using image-global attachment slots.

Preparation runs after the floor/tracer/coordinator and before Integer; Float
activation retains its existing slot after Integer. As with Integer's extraction,
Float leaves the floor bootstrap enumeration and its names enter the namespace
when the loaded module publishes them. Canonical names, identities, first-alias
selection and binding replacement rules are unchanged; numeric enumeration
positions are a construction-order detail, not a promised ABI index.

All native modules must be rebuilt for the 34-field, 272-byte IdApi v7. Historical
v1-v5 manifests/fixtures remain unchanged. Method, Frame and image formats remain
unchanged. Run `mise run float` and `scripts/float-image-smoke.sh PREFIX` against a
debug prefix for the focused native and compiled-image checks.
