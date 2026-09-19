# 81BA — Error Value borrows the caller's presence capability

> **Status:** Implemented and debug-validated, following interned lookup in 81AZ.

Remove Error Value's dependency on IdApi.isExported. Its native operations receive
an explicit borrowed Namespace presence descriptor on each call. The recipient
uses its current function/context only at the original presence check, after kind
interning and String construction for errors, or after the null-value short circuit
for recognition. It retains no descriptor or context. Different nested callers
can select different capabilities; replacement during earlier callbacks stays live.

Namespace presence v1 is a 32-byte function/context descriptor, with version and
extent. There is no process-global presence dependency or API-address registry.
Error Value becomes v2 because both native operation signatures gain this explicit
argument. Its descriptor remains 32 bytes; current hosts, native fixtures and all
declared consumers select only v2. Existing Error representation and dispatch stay
unchanged.

The current native callers still own live IdApi slots. Their call-site adapters
lend the address of that one slot through the new descriptor, without copying its
function early. The slot adapter accepts no IdApi and exposes no aggregate table.
This lets the shared recipient move independently from its callers' later native
initialization handoff. Do not claim IdApi field removal at this checkpoint: the
remaining direct consumers and module/installer transport still require migration.

Keep callback order, fresh factory lookup, imported behavior/factory snapshots,
negative presence flags, null/alias inputs and outputs, eager kind/message creation,
borrowed buffers, reentry, GC, reload and numeric declaration identities. No object,
method, root, native module, language protocol or image encoding is added.

Verify the existing independent Error and numeric consumer oracles against 81AZ,
then exercise the native v2 boundary with a supplied presence capability while
IdApi.isExported aborts. Change function/context during preceding callbacks; reenter
with another capability and verify the outer selection survives. Verify dynamic
startup through the host, source-free compiled caller, saved native Code image and
baked executable in a fresh debug prefix. No release validation.


## Validation

- Focused Rust checks pass for the ABI, provider, host and all nine consumers.
  ABI unit tests pass (7), including independent live slots and replacement of
  capability function/context; host tests pass (86, 1 ignored). ABI/Error Value
  strict Clippy passes. The provider has no remaining isExported field access.
- Independent native admission checks exercise all nine consumers before their
  first attachment. They reject null, v1, wrong extent, either missing operation
  and retargeting; repeated attachment succeeds. No old provider/dependency export
  remains. Initialization still exposes the provider without invoking its methods.
- Error Value's 114-case oracle passes, plus two new cases replacing the caller's
  isExported slot during kind interning and String construction. Integer's 395
  error cases and 405 module cases, Float's 373 and Word's 24 pass. Numeric
  declaration checks retain copied API and live Integer callback operands.
- Native Loader (1,020 cases), Source Loader (524) and Block Loop (980) pass in
  all three existing normal/registration-reentry modes. Separately compiled
  historical v1 and current v2 fixtures produce identical complete traces for
  Error Value, Integer/Float/Word errors and all three loader/loop modes. Each
  historical fixture uses its historical header and support code, without ABI casts.
- A separate C oracle passes a presence capability while IdApi.isExported aborts.
  It proves null recognition skips all capability access, nonzero/negative flags
  retain meaning, kind/String callbacks can change the selected function/context,
  and nested calls with another API and capability preserve the outer selection.
  Both actual operation addresses resolve to the loaded Error Value dylib.
- That oracle passes dynamic startup, source-free compiled caller, saved v8
  native Code image and baked execution. The existing Error Value image suite
  also passes, preserving actual Error construction/recognition, aliases, precise
  GC and fresh factory replacement. Standard library: 145 passed, zero failed.
- All 71 installed native/authored library export sets match the previous prefix
  except the provider and nine dependency v1-to-v2 renames. Current independent
  manifests and system-only linkage pass. Declared install names match. Integer
  has no install-name manifest entry and retains its existing absolute build-path
  install name; the scoped inventory check confirms the same baseline form.
- Fresh debug prefix:
  `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81ba-prefix.orlh58cx`.
  It contains 43 native libraries and 28 authored modules. All 1,772 source hashes
  stayed unchanged through installation and validation; only this validation
  document changed afterward. No release validation or full install-smoke run
  was performed.

Receipts are `.cargo-target/81ba-{check,unit,abi-final,clippy,error-value,
integer-error-value,float-error-value,word-error-value,native-loader,
source-loader,block-loop,native-audit,inventory,error-presence-image,
error-value-image,stdlib,install}.log` and the source-hash manifest.

## Next boundary

Move the callers' presence selection through native initialization and installer
handoffs, preserving registration versus saved execution tables and direct
Native Loader forwarding. This enables replacing the slot loans and retiring the
IdApi field. The remaining direct consumers, including the shared ForeignAddress
helper, need the same explicit selection; no implicit API-address registry or
process-global replacement is an acceptable substitute.
