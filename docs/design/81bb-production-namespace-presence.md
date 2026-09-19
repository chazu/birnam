# 81BB — Namespace owns presence and its native handoff

> **Status:** Implementation in progress, following Error Value handoff in 81BA.

Move canonical isExported into Namespace and remove its IdApi entry. Namespace
creates one mutable presence descriptor per Runtime, using its selected lookup,
find and opaque storage plus Id's restricted Symbol interner. Preserve null-name
diagnostics, C-string scanning before runtime admission, missing-name interning,
raw-null versus nil bindings and independent/moved runtimes. No user callback is
introduced inside the primitive. Capture original operation/context separately
for pure numeric admission; live replacements must decline without invocation.

IdApi becomes v9 (32 fields, 256 bytes). Native module_init gains a third explicit
presence pointer after the existing API and state arguments. Every current native
initializer and caller migrates; no old-signature cast, aggregate compatibility
table or API-address registry is used. The host supplies the selected floor's
presence capability. Native Loader forwards its original execution selection and
null state at the existing call, without new symbol queries or dependency mounting.

Module initializers use the incoming presence for incoming registration APIs and
retain it together with the first saved execution API where methods retain that
choice. File IO, Environment, Input, Block Loop, Source Loader and Native Loader
installers receive explicit execution presence. Float/Word preparation receives
presence at the same point it first captures the execution API, before callbacks.
Do not accidentally replace that selection on later activation or registration
reentry. Capability storage/code must live as long as its corresponding retained
native consumer; per-call Error Value and ForeignAddress borrows remain synchronous.

Migrate every actual presence consumer and the shared helpers. Snapshot the live
function/context only at each original presence call. Keep callback order,
identity, roots, adoption, reload, GC, errors and image encoding unchanged. Bump
native numeric declaration operands to include the canonical presence context.
The old Id forwarding operation and field must be absent at completion.

Validate against independently compiled 81BA clients, including caller-specific
capabilities, mutation/reentry during registration and execution, default interning,
raw null/nil, numeric admission and direct loading. Verify startup, source-free
compiled caller, saved native Code image and baked executable in a fresh debug
prefix, then commit. No release validation.

## Work-in-progress checkpoint

Checkpointed at the user's request before completing the extraction. This is
not a completed production validation receipt.

Implemented so far:

- Namespace factory v5 owns per-runtime presence; IdApi v9 removes isExported.
- Native initializers, the host, direct Native Loader calls, Float/Word preparation,
  retained execution selections and six System/Block installers carry presence
  explicitly. Alien captures the pair before initialization callbacks can reenter.
- Current C mirrors and fixtures use the new table and initializer. Copied-table
  fixtures have separate presence descriptors; historical IdApi versions remain.
- Presence unit coverage exercises empty/non-UTF8 C names, interning before find,
  raw-null absence, independent contexts, live replacement during a callback,
  destruction after replacement and inert rejection of malformed construction.

Checks at this checkpoint:

- `cargo check --workspace --all-targets --offline --locked` passed.
- `cargo build --workspace --offline --locked` passed in debug mode.
- 49 current C fixtures passed strict syntax checks.
- ABI and host library tests passed (7 and 86 respectively; one host test ignored).
  The combined Id/Namespace library test run was still in progress when this
  checkpoint note was prepared; its local log is `.cargo-target/81bb-unit.log`.

Still required before calling 81BB complete:

- Finish reviewing first-API versus registration-API selection, reentry, capability
  lifetime, native initializer callers and all migrated independent fixtures.
- Update native export inventories, current ABI/reference documentation and
  changelog, including Namespace factory v5, the six installer v2 descriptors,
  Float/Word preparation v2 and numeric declaration v3's eleven operands.
- Replace copied v8 wording in the new v9 fixture README with current evidence.
- Run actual C fixtures against current debug libraries and independently compiled
  81BA clients, including pure numeric admission after presence function/context
  replacement, host admission negatives, diagnostics and GC/reload behavior.
- Build a fresh debug prefix from frozen sources and verify siphon startup,
  source-free compiled loading, saved native Code images and baked execution.

No release validation has been requested or performed for this checkpoint.
