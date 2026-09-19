# 81AL — System output methods

> **Status:** Implemented, following shared text views in 81AK.

Move System print:/println:, their shared output body and stdout writer into
`birnam-system-output`. Use Text View v1 for the existing conversion policy.
Move the existing writer test with its implementation and copy the small borrowed
argument adapter still used by Prelude. No new language behavior or IO policy.

The provider loads after Text View and before Prelude. Module initialization only
enables its immutable 24-byte installation descriptor; it creates no objects,
methods, roots or layouts. Prelude keeps System creation/adoption and all twelve
selector interns. At the original first two registration slots, pass the current
registration table, Prelude's original execution table, the selected System
behavior and the already interned print:/println: selectors. Retain execution
before invoking registration callbacks, including when the first installation
is reached through reentry with a different registration table. Keep the supplied
behavior/selectors as synchronous snapshots; do not reintern or retain them.

Copy the method and writer bodies unchanged. Both methods require one borrowed
argument; invalid arity/carrier or unavailable printable bytes return the live
nil value. Otherwise write the exact bytes, append a newline only for println:,
then flush. Ignore IO errors at the existing outer writer and return the original
receiver, including null. Preserve empty writes, short-write/error propagation
inside the writer, fresh printing dispatch and callback/GC ordering before IO.
Keep Text View's pointer/lifetime contract; no copied buffers or extra roots.

Compare independent native output and installation traces against installed
81AK, including calls during registration, first-intern/first-binding reentry,
selector/behavior snapshots, live callbacks, nil identity, arbitrary receivers,
replacement, reload, IO failure and GC. Reuse the shared text consumer oracle for
binary data and conversion behavior. Verify a fresh external debug installation,
source-free compiled module, saved v8 native Code image and baked executable.
Commit this slice before continuing. No release validation, IdApi/layout/image
change, new public selectors or compatibility forwarders.

## Validation

- Output and writer bodies, the argument adapter and the existing writer test
  match 81AK after normalizing formatting. Prelude's coordinating installer
  changes only the first two method bindings to the new installer call. All
  other Prelude function bodies and registration statements are unchanged.
  Both native Methods resolve into System Output, with no Prelude forwarders.
- The independent C oracle passes 680 byte/receiver combinations in normal
  startup, first-print-intern reentry and first-print-binding reentry. Complete
  output and callback traces match installed 81AK, development libraries and
  the fresh installed prefix. Each registration callback invokes the supplied
  Method before installation finishes, proving execution is selected in time.
- Receivers include null, nil, an ordinary object and System. Binary strings
  include embedded NUL and invalid UTF-8. Malformed carriers, invalid arity,
  unavailable conversion and live nil identity preserve results and callback
  counts. Captured output is read after the method returns without an extra
  Rust flush; the moved prompt test also verifies newline-free flushing.
- Broken-pipe stdout in a child process preserves both Methods' receiver results
  for ordinary and null receivers. Nested output and GC during conversion pass.
  Copied Methods remain callable after public replacement; reinstallation and
  direct repeated installer calls keep the original execution table.
- A first-binding callback changes exported SystemVt and selector mapping.
  The second binding retains the selected behavior/selector snapshots; the next
  installation adopts the changed values. Provider initialization adds no
  allocations, method bindings or root providers. Precise collection preserves
  rooted values and their subsequent printable output.
- Admission rejects null, wrong version/size, missing install and retargeting;
  the same pointer can be attached again. Development and installed libraries
  match the four-symbol inventory, relocatable install name and system-only
  linkage. Prelude's only export change adds System Output attachment.
- Text View's 682 binary-output cases and all remaining Prelude consumer traces
  match installed 81AK. Focused System GC (496 cases in each of three modes),
  Printing (1,104 in each of two modes), Boolean (34), Float (676 arithmetic cases)
  and Word (81 equality cases) pass, including installed native checks.
  All 145 stdlib assertions pass. ABI tests (four), Prelude identity (one),
  moved output writer test (one), host tests (86, one ignored), builder tests
  (four), and ABI/System Output Clippy with warnings denied pass.
- The fresh external debug prefix contains 35 native libraries and 28 compiled
  modules. All 1,624 source hashes stayed unchanged during installation; this
  validation record was added afterward. Installed package metadata and all 63
  required libraries match the updated install-smoke inventory. Neither release
  validation nor the full install-smoke script was run.
- Both installed 81AK and the new prefix pass dynamic startup, a source-free
  compiled module, a saved v8 native Code image and a baked executable. These
  exercise exact output, empty/newline output, receiver and payload aliases,
  precise GC, fresh printString replacement and replacement of both output
  selectors on a System clone. System Output is observed loaded during the new
  startup, saved-image and baked runs.

Receipts are `.cargo-target/81al-{check,build,compare,system-output,text-view,
system-gc,printing,boolean,float,word,unit,clippy,prefix,prefix-native,
image-baseline,image,stdlib}.log`, plus source hashes and per-mode
baseline/development/installed output traces.

## Next boundary

Extract Prelude's shared error construction and exact-recognition helpers before
moving its remaining Block and System methods. They depend on live Error/ErrorVt
exports, fresh kind:message: dispatch and String construction, and must keep the
existing nil behavior before Error is loaded. Preserve callback order, exact
recognition, current table selection and null/alias results. Audit the similar
Integer/Float/Word copies and their earlier preparation order before migrating
those consumers; Error's own payload constructor has different semantics.
