# 81J — Extract the existing private-behavior operation

> **Status:** Production refactor implemented and validated.
> Builds on `c5af8c59`, which checkpoints the isolated 81I experiment.

## Scope

This work splits an existing responsibility out of production Id. It introduces
no executable-ownership system, unloading policy, collector redesign, or new
lifetime tracking. The isolated 81F–81I proofs establish possible boundaries;
completing every limitation of those experiments is not a prerequisite for the
production refactor. In particular, automatic native-code retirement is outside
this slice.

The extracted operation is `ensurePrivateVtable`. Its implementation moves from
`birnam-id::Runtime` into [birnam-object-model](../../rust/birnam-object-model/src/lib.rs),
a separately loaded Rust dylib. Production Id has no normal dependency on that
crate. Its unit tests use a dev dependency to exercise the same extracted
algorithm against their independent Runtime instances.

## What moves, and what remains in Id

The object-model module owns the existing selection algorithm:

1. Obtain the current behavior after the existing allocation-phase/safepoint checks.
2. If the object is itself a default vtable, return the object.
3. If its current behavior already names it as owner, reuse that behavior.
4. Otherwise create a default behavior delegating to the current one, record the
   owner association, and attach the new behavior to the object.

The Rust `BehaviorAccess` trait is an adapter for these operations and the test
harness. It is not a new native table, registry, or runtime object representation.
The production adapter uses the declared IdApi operations and seed messages.

Id retains its existing heap, default-vtable representation, owner field,
parent relation, collection machinery and mutation epoch. Two seed messages on
Vtable make the previously internal boundary available:

- **`prepareBehaviorMutationOf:`** preserves the previous operation's owner-thread
  check, prohibition during collection, automatic-collection opportunity and
  checked behavior read. Keeping this preflight also preserves those checks when
  the convenience operation merely reuses an existing behavior.
- **`reassociateBehaviorOf:with:`** validates live allocation membership, writes
  the existing object's behavior header, and advances the existing vtable
  mutation epoch. It performs no behavior construction or owner bookkeeping.

The existing `proto`, `vtableOwner` and `setOwner` operations supply default
behavior construction and association access for this slice. Their further
partition is separate work. The core reassociation operation does not require
that every behavior use the default-vtable representation.

Production's owner edge remains **strong**. A reachable behavior keeps its owner
alive, and an unreachable owner/behavior cycle remains collectable. The weak
reuse cache in the isolated experiment is not substituted for this representation.
Method publication, compiled-method accounting, image handling and GC continue
through their existing implementations.

## Callers and loading

The upper module installs **`ensurePrivateVtableFor:`** on Vtable. Native callers
use a small message adapter in `birnam-abi`; it contains no copy of the private
behavior algorithm. Prelude, TCC and Codegen/Reflect migrate to that adapter.
The generated-code bridge `birnam_ensurePrivateVtable` remains a TCC entry and
now reaches the loaded service through the same message path.

Both the stage-0 host and the host used to build stage 1 load
`libbirnam_object_model.dylib` immediately after Id and before the other native
rungs. The installer publishes the new library with the stack. The library and
its API pointer follow the existing process-lifetime native-module contract.
The required seed-method bindings retain its two cached selector identities;
ordinary replacement of their implementations preserves those identities. These
seed services remain prerequisites of the layer. There is no new unload mechanism
or side registry.

This is a Rust layer extraction. It does not make the installed host a siphon,
replace this module with Birnam code, or remove the other responsibilities still
inside production libid.

## Deliberate IdApi break

`ensurePrivateVtable` is removed from the table; neither new seed message adds a
native field. The arm64 macOS table changes from 54 fields / 432 bytes to
**53 fields / 424 bytes**. Later fields shift down by eight bytes.

The current declaration is [idapi-v2.json](../../abi/idapi-v2.json), with an
independent [C header](../../abi/fixtures/idapi-v2/idapi_v2.h). The diagnostic
record still uses descriptor format v1, but its schema version is 2 and it
publishes the new layout fingerprint. The v2 fingerprint is SHA-256 of sorted,
compact UTF-8 JSON excluding the `fingerprint` member; a Rust test verifies the
fingerprint and all field offsets against the compiled descriptor.

All active source-built native fixtures migrate to v2. The old v1 manifest and
committed binary fixtures remain historical evidence and are not regenerated or
claimed compatible. Consumers compiled against the old table must be rebuilt;
there is no compatibility slot or forwarding field. Current build/install paths
rebuild the native stack and authored modules together.

## Acceptance

Validation covers the existing prototype identities, vtable self case, private
behavior reuse, sibling isolation, mutation visibility and dead owner/behavior
cycles. A focused test roots only the private behavior and checks preservation of
its strong owner reference.

The source-built v2 C client additionally verifies that bare Id has no
`ensurePrivateVtableFor:` implementation, mounts the object-model dylib, then
exercises that service and the migrated table. Other foreign clients explicitly
mount the same prerequisite before Prelude/TCC/Codegen. The native prefix gate
checks the new installed inventory along with startup, compilation, image,
stackless and concurrency behavior.

The recovery runner retains its historical checks on frame, registration,
emitter and image contracts. It checks the current IdApi against the independent
v2 manifest and records its hashes separately; it no longer claims the table is
unchanged from the 74T baseline. Archived v1 comparison experiments retain their
original contracts and are not part of current acceptance.

Installed inventory checks now expect 16 native libraries and the existing 28
authored modules, including `stacklessControl`. The combined native gate has a
600-second limit to accommodate its current image and concurrency coverage; no
cases are removed.

For local exhaustive validation, the existing CI matrix groups run in four
independent processes against the same recorded prerequisites. The first four
long debug sweeps exceeded their 900-second limits without assertion failures;
their reruns allow 3600 seconds per matrix. Commands, cases and the final coverage
oracle are unchanged. Initial timeout logs are retained under
`.cargo-target/validation-81j/timed-out-matrices/`.

Validation results:

- Workspace: **608 tests passed**, 28 ignored by the existing test configuration.
- Full release plan: **84 steps passed**, including the complete debug/release
  matrix oracle, native clients, image and concurrency checks, recovery,
  dylib export/dependency audit and two independent cold-source installations.
  The local matrix scheduling and extended deadlines are described above.
- The separate `scripts/install-smoke.sh` check passed against the validation
  prefix, including relocation and the installed source/library inventory.
- Production `cargo tree -p birnam-id --edges normal` contains no dependency on
  `birnam-object-model`; the upper algorithm is linked into Id only for tests.

The complete source/artifact-bound release receipt is
`.cargo-target/validation-81j/run.json`; matrix logs and individual receipts are
under the same directory. The separate install smoke log is
`.cargo-target/81j-install-smoke.log`. These are local validation artifacts, not
installed runtime dependencies. No performance qualification is claimed.
