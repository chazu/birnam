# 81L — Production Integer extraction

> **Status:** Complete; focused validation passed.
> Follows owner/parent protocol checkpoint `203238a8` and method-library
> checkpoint `a5b4b90b`. The complete boundary is a separate commit.

## Ownership and scope

`libbirnam_integer.dylib` now owns the canonical Integer behavior and prototype,
the signed-64-bit raw payload, direct native construction, unchecked reading,
exact recognition, and thirteen arithmetic/comparison/conversion/iteration
methods. It loads immediately after Id and before object-model and Artifact.
Prelude no longer installs these methods. Integer depends only on `birnam-abi`.
The current installation contains 17 native libraries and 28 authored modules.

Id's bootstrap and Values no longer create or interpret Integers. Bare Id does
not export `Integer` or `IntegerVt`. Their enumeration position consequently
follows module mounting. The module's explicit root provider retains its two
identities even when both names are rebound, and roots partial initialization
before any allocating callback. Tests instantiate the same implementation with
per-Runtime Id adapters; Integer is only a **dev-dependency** of `birnam-id`.

IdApi v4 removes `smallint_vt`, `mkSmallInt`, `smallIntValue`, and `isSmallInt`:
45 fields, 360 bytes. All native clients migrate to the explicit
[Integer dependency](https://github.com/chazu/birnam/blob/cffba722/abi/integer-v1.md); there is no compatibility facade.
The host injects named dependencies before `module_init`, whose opaque state
contract remains unchanged. Generated code retains its existing TCC bridge names.
The still-combined Machine, recovery and optimizer components explicitly consume
Integer. They have not been extracted from the Rust runtime by this change.

## Interface alternatives and decision

Three independent designs considered ordinary service messages, a native Integer
capability, and caller-owned storage with a representation descriptor. Ordinary
messages would make formerly callback-free reads reentrant under existing state
borrows and introduce allocation-message effects into the native constructor.
Those are semantic changes beyond this extraction. A descriptor alone also does
not authorize an optimizer to omit constructor effects.

The selected boundary combines live native operations with an immutable pure
storage codec. Ordinary consumers copy the current operations table before use.
The codec only initializes/reads supplied payloads: it cannot allocate, collect,
call Id, reenter or consult mutable module state. Recovery keeps its existing
safepoint, re-admission/layout check, generic allocation, initialization and
immediate rooting sequence. Deferred allocations hold aligned untraced opaque
payload batches; Heap never interprets Integer or invokes the codec at settlement.
The existing counted-index ledger owns its encoded batch, with the same bounded
multiplicity accounting and retention/release points.

## Preserved behavior and admission

- Ordinary lookup still chooses the current arithmetic method; Prelude mounting
  does not overwrite a replacement. Equal results remain distinct identities.
- Preserve evaluation, Error propagation, mixed Float behavior, existing
  comparison precision, overflow, zero division, and MIN remainder -1.
- Native construction still allocates directly with a safepoint; it does not
  send `allocate`. Exact recognition uses heap membership and canonical vtable
  equality. The unchecked payload read remains valid after reassociation.
  Image's inherited-behavior rule remains separate and unchanged.
- Native numeric declaration v2 belongs to the actual Integer method image.
  Current contexts, identities, operations, codec and allocator must match the
  admitted contract before bypassing native calls. Wrappers withdraw admission;
  restoring them restores eligibility. Closure admission passes its actual
  Integer dependency through the private v2 hook too.
- Existing module lifetime, roots, finalizers, code pinning and retirement rules
  remain in force. No unloading or collector-policy change is introduced.
- Method ABI, Frame descriptors and durable image bytes remain unchanged. The
  independent image-authority comparison permits only exact counted dependency
  substitutions; negative controls reject changed operands or recognition rules.

The first checkpoint moved only methods/declarations. That checkpoint alone was
not a completed Integer extraction. The complete boundary additionally migrates
all production primitive consumers, storage authority, mounting and ABI evidence.

## Validation and next work

The independent C fixture begins with bare Id, checks 405 signed-128-bit oracle
cases, errors, mixed values, fresh identity, replacement before/after Prelude,
reassociation, foreign-address rejection, and roots after namespace rebinding.
A separate copied-API fixture checks every numeric context operand and a live
constructor wrapper. The independent v4 C header and manifest check the reduced
IdApi; the Integer header independently checks its two structures.

Workspace tests (610 passed), focused Integer/ABI lint checks and the independent
installed-module fixtures pass. Final changes are checked with focused numeric,
recovery and deferred-allocation tests. The user explicitly waived release
validation for this refactor on 2026-09-18; no release-validation gate remains.
No performance claim or Birnam-authored replacement is made.

After committing the completed Integer extraction, proceed separately through
raw environment/export namespaces, remaining primitive values, default-vtable
reflection, storage/representation contracts and collection. Executable ownership,
Frames, Machine/control and optimization follow their prerequisites. Audit exact
callers, boot order, identity and native lifetime at each boundary before coding.
