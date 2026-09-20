# 81BB — Namespace owns presence and its native handoff

> **Status:** Implemented and debug-validated, following Error Value handoff in 81BA.

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

## Implementation and review

The WIP checkpoint was committed separately at the user's request. The completed
implementation uses Namespace factory v5, IdApi v9, six installer v2 descriptors,
Float/Word preparation v2 and the eleven-operand numeric declaration v3.

The owned service copies immutable floor access and interner callbacks. On each
call it scans the C name, resolves a transient interner context after owner and
storage-observation admission, then runs the selected lookup/find. No movable
Runtime pointer is retained. The original operation/context pair is kept for
numeric admission separately from the mutable descriptor. Destruction uses the
owned service address even after live context replacement.

Caller review preserves incoming versus first-saved API selection. Prelude and
Integer save API/presence before publication callbacks; Float and Word do so at
prepare. Alien saves the pair at its earlier API capture point. System/Block
installers capture the pair at their existing first-install point. Lang, Tcc,
Artifact and Codegen use saved presence wherever initialization switches to the
saved API; earlier incoming checks keep incoming presence. Native Loader keeps
its single module_init lookup and forwards API, null state and execution presence.
Error Value and ForeignAddress receive explicit synchronous borrows.

Independent old/new native fixtures exposed a test-helper omission when mounting
Integer with a copied API; its helper now takes explicit presence too. The audit
also corrected owner-admission order and Lang/Tcc's late reload selection. These
are covered by tests rather than changing baseline traces or weakening assertions.

## Validation

- `cargo check --workspace --all-targets --offline --locked` and debug workspace
  build passed. ABI, Id, Namespace and host library tests passed: 353 total,
  17 intentionally ignored. Strict ABI/Namespace Clippy passed.
- 51 current independent C fixtures passed strict syntax checks. The v9 native
  client/module passed all 32 fields, module state/presence forwarding and the
  separately loaded primitive families.
- Sixteen old/current native transcripts matched exactly. Baseline clients were
  compiled from commit `723b3d6c` against its actual v8 headers and 81BA libraries;
  current clients used v9. Coverage includes Integer/Float/Word errors and System
  input, environment, file IO, native/source loading and Block loops, copied APIs,
  registration/execution reentry, earlier callback changes, GC and reload.
- Namespace's loaded-floor oracle passed 2,048 ordered binding cases and 4,096
  independent-state cases, including empty/non-UTF8 names, missing-name interning,
  null bindings, aliases, enumeration and precise GC. `dladdr` places the default
  presence operation in Namespace. `scripts/namespace-test.sh` also checks exact
  null/owner/observation diagnostic precedence and allowed GC-root presence calls.
  All six boundary outcomes match independent v8 executions.
- Eighteen actual host admission cases passed: missing/old/malformed factory,
  missing/rejected floor dependency, Names failures and missing/malformed Presence.
  Numeric declaration v3 admits the copied API/capability pair, rejects changed
  presence function or context without invocation, and readmits after restoration.
- Fresh debug prefix: `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81bb-prefix.wwj9u0nj`.
  All 1,796 source hashes stayed fixed during installation. Subsequent edits were
  documentation and the reload fixture's installed Tcc filename only; production
  Rust/native build inputs remain identical to that frozen source snapshot.
  The prefix contains 43 native libraries and 28 authored compiled modules.
- `namespace-image-smoke.sh`, `error-presence-image-smoke.sh`,
  `error-value-image-smoke.sh` and `native-loader-image-smoke.sh` all passed startup
  through the dynamic host, source-free compiled callers, saved v8 native Code
  images and baked executables. The presence probe includes Lang/Tcc reload with
  a different incoming API/presence pair, independent nested Error Value contexts
  and late callback/context changes. Actual Error/factory replacement, aliases,
  precise GC and direct repeated native initialization remain covered.
- All 71 old/new library export inventories match except the declared v5/v2/v3
  symbol changes and new Id presence provider. All 11 affected manifests match;
  the bare Id manifest was checked separately from the installed floor's existing
  module wrapper exports. Declared install names and system-only linkage pass.
  Integer retains its preexisting absolute debug-build install name.
- The installed standard-library suite passed 145 assertions. No release validation.

Local receipts are `.cargo-target/81bb-final-unit.log`, `81bb-clippy.log`,
`81bb-namespace-gate.log`, `81bb-boundary.log`, `81bb-host-validation.log`,
`81bb-installed-native.log`, `81bb-idapi-client.log`, `81bb-inventory.log`,
`81bb-install-driver.log`, `81bb-source-hashes.json`, `81bb-namespace-image.log`,
`81bb-presence-image.log`, `81bb-error-image.log`, `81bb-loader-image.log` and
`81bb-stdlib.log`. ABI schema and documentation describe the native break;
Method and image encoding remain unchanged. The broader 81-series work continues.
