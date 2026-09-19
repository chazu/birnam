# 81AP — Integer uses shared Error value policy

> **Status:** Implemented, following Float's migration in 81AO.

Remove Integer's identical is_error, make_error and numeric_error helpers and
import the existing Error Value adapters at their current calls. Preserve every
other method, representation and native numeric declaration body. This includes
arithmetic overflow, division/modulus, mixed numeric values, existing comparison
precision, equality's separate rule, and to:do: block-result propagation.

Load Error Value before Integer. Move the host's String/Word/Float preparation
trigger to that earlier boundary without changing their relative order or
preparation bodies. Error Value needs only prepared String; it allocates no
objects and retains no IdApi. Attach its descriptor before Integer initializes
and can publish/register methods. Keep Integer's first selected execution table,
canonical identities, roots, live dependency selection and native admission.

Compare an independent C oracle against installed 81AO, including failure calls
during registration, lazy construction, exact versus inherited recognition,
numeric ErrorVt aliases, fresh factory lookup, callback mutation/reentry, GC and
iteration stopping. Exercise the existing signed-128-bit numeric oracle and
numeric admission checks. Update real fixture mounts and dependency build
closures. Verify fresh external debug startup, a source-free compiled module,
saved v8 native Code image and baked executable, then commit. No release
validation, new functionality, API version change or compatibility facade.

## Validation

- The removed helpers match Error Value's existing policy. Every remaining
  Integer function body, including its native numeric declaration, and its
  storage implementation are unchanged. Startup changes only the provider's
  position and the preparation trigger; String/Word/Float preparation retains
  its existing sequence and bodies.
- An independent C oracle passes 395 failure, recognition and construction
  combinations. Complete callback traces match installed 81AO, development
  libraries and the fresh installed prefix. All nine affected binary methods
  produce errors during registration; to:do: also executes during registration.
- The checks preserve receiver reads before null argument-carrier rejection,
  exact Error recognition before numeric conversion and zero checks, inherited
  Error rejection, equality's separate rule, Integer/Float ErrorVt aliases,
  signed overflow, both Float zero divisors and MIN modulus -1. Every existing
  failure kind and message remains exact.
- Construction preserves absent/null factories, current nil, null/aliased kinds,
  messages and results, live String constructor/context replacement, factory
  publication during message construction and the imported factory snapshot.
  Fresh ordinary/custom lookup, nested construction, callback GC, copied Methods
  and reload preserve behavior. Integer reload retains replacements rather than
  reinstalling methods, as before.
- Iteration stops at the first exact Error, ignores inherited Error and missing
  exports, observes an ErrorVt published during the block callback, handles a
  numeric Error alias, and retains the existing empty/max-endpoint behavior.
  Precise collection preserves the retained values and subsequent error calls.
- Integer's eight-symbol inventory adds only Error Value attachment. Its 80-byte
  operations table, 48-byte codec, payload and existing install-name convention
  are unchanged. Both development and installed libraries match the inventory
  and retain system-only linkage. Float, Word and Prelude exports are unchanged.
- The existing Integer oracle passes all 405 signed-128-bit arithmetic cases,
  mixed values, identities, errors, replacement and roots. The copied-IdApi
  native-admission fixture passes every context operand, live constructor
  wrapping and restoration. Both fixtures now mount the declared dependencies;
  all explicit Integer fixture build closures include Error Value.
- Focused Float Error Value (373), Word Error Value (24), Float (676 arithmetic
  cases), Word (81 equality cases), Error Value (114), System Output (680 in each
  of three modes), Text View (682), System GC (496 in each of three modes),
  Printing (1,104 in each of two modes) and Boolean (34) checks pass, including
  installed native comparisons. All 145 stdlib assertions pass. Integer's
  numeric declaration unit test, host tests (86, one ignored), builder tests
  (four), and Integer Clippy with warnings denied pass.
- The fresh external debug prefix contains 36 native libraries and 28 compiled
  modules. All 1,649 source hashes stayed unchanged during installation; this
  validation record was added afterward. Installed metadata and all 64 required
  libraries match the installation inventory. No release validation or full
  install-smoke run was performed.
- Installed 81AO and the new prefix pass dynamic startup, a source-free compiled
  module, a saved v8 native Code image and a baked executable. Checks preserve
  Integer payloads and aliases, arithmetic, overflow/division/modulus failures,
  exact Error propagation, precise GC, iteration and max endpoints, and fresh
  Error factories returning an object alias, a String and nil.

Receipts are `.cargo-target/81ap-{check,build,compare,integer-error-value,
float-error-value,word-error-value,float,word,error-value,system-output,text-view,
system-gc,printing,boolean,unit,clippy,prefix,prefix-native,image-baseline,image,
stdlib}.log`, plus source hashes and baseline/development/installed Integer traces.

## Next boundary

Extract System readLine into a small System Input provider, following System
Output's registration/execution-table boundary. Prelude keeps System identity,
all selector interning and registration order. Move the entire existing method,
including stdin locking/buffering, byte-preserving reads, removal of only a final
LF, EOF nil and lazy ioError construction. The lock remains held through String
or Error callbacks, so reentry and buffered subsequent reads require independent
baseline checks. String and Error Value are already explicit dependencies; no
new language functionality or changes to the other System IO methods are needed.
