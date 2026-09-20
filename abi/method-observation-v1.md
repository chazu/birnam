# Native method observation v1

[81BI](../docs/design/81bi-production-method-observation.md) moves the single
nullable native pre-overwrite callback from Id into Method Observation. This
service owns replacement, snapshot selection and invocation. Id retains exact
direct lookup, dictionary mutation, executable reference accounting, Block-cache
invalidation and staged publication journaling.

The [independent C mirror](fixtures/method-observation-v1/method_observation_v1.h)
fixes the arm64 layouts. The immutable factory is 40 bytes: version 1, extent,
create, destroy and notify (at offset 32). The copied Floor is 32 bytes:
version 1, extent, context and admission. The mutable public capability is
32 bytes: version 1, extent, context and replace (at offset 24).

The host loads and pins `libbirnam_method_observation.dylib` and attaches
`birnam_method_observation_factory_v1` through Id's
`birnam_method_observation_dependency_v1` before `id_init`. Factory validation
and service creation invoke no floor callbacks. Invalid versions, extents or
missing operations reject; same-pointer attachment repeats and retargeting
rejects. Each Runtime owns an independent service that retains a copied Floor
and no pointer to the movable Runtime. Id exposes that Runtime's public
capability through `birnam_method_observation_provider_v1` after initialization.
The ordinary native module mounts after Object Model and publishes no objects
or methods. Production Id has no Rust dependency on this upper implementation.

Replacement selects the public context/operation at the call point. The service
copies its floor selection, performs the original owner/observation admission,
then replaces its hook. Null clears; repeated or different hooks are accepted.
The hook has C signature `void (Oop vtable, Oop selector)` and owns neither its
arguments nor native code. Existing callback code must remain live; the service
adds no GC roots or native lifetime ownership.

Only Runtime add/remove sites notify. Id checks direct dictionary membership
before notification even with no installed hook. A new or inherited-only key
and missing direct removal do not invoke the callback. A direct overwrite,
including the same address, or removal invokes it before mutation; the callback
sees the old dictionary and holds no RuntimeState borrow. Existing pure Store-log
mutation/read-only floor constraints remain. Seed and staged rollback raw
mutations do not notify. Factory notification and destruction use the original
owned service address regardless of public capability replacement.

IdApi v14 removes `setMethodOverwriteHook` and its obsolete hook layout entry:
26 fields, 208 bytes. Native initialization takes seven arguments, adding the
matching observation capability after vtable enumeration. Store registers with
the incoming capability, after root publication/root registration and after
setting its once flag, even when execution retains the first API. Prelude and
[Native Loader v7](native-loader-v7.md) retain and forward their first execution
selection. All native consumers require rebuilding; image encoding is unchanged.
