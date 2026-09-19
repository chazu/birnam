# 81AX — Namespace owns reverse-name lookup

> **Status:** Implemented and debug-validated, following Namespace state in 81AW.

Move exportedNameOf and its four native consumers (Image, Alien, Portable Code
and Codegen/Reflect) to a dedicated Namespace names capability. Remove the field
from IdApi rather than retaining a forwarding entry. Import, export and presence
remain separate subsequent work. Preserve first-current-alias selection, null
results, borrowed Symbol bytes, owner/observation checks and all call ordering.

Namespace's prepared storage factory becomes v2 and adds creation/destruction of
an owned names capability. Creation receives the selected immutable storage
reverse operation and opaque context, preserving instrumented/replaced storage
without casting its representation. Existing storage operations stay unchanged. Each
Runtime owns its namespace context and names service; no global side namespace is
introduced. The names service uses two immutable floor operations: the existing
owner/storage-access preflight and raw Symbol-name borrowing. These perform no
user sends, allocation, GC or reentry. Selection itself stays in Namespace.
Independent unit Runtimes use owner-thread active-runtime callbacks; production
uses the selected process Runtime, preserving the previous primitive's choice.

The public names descriptor is 32 bytes: version, extent, context and name_of.
Ordinary consumers read its current context/operation before every call. Retain
the original context/operation separately for pure Block admission; an altered
capability must withdraw admission without calling it. Bump the producer
Block-construction declaration to reflect both operands, and remove the old
producer export. No admission result is cached.

IdApi becomes v8: 33 fields and 264 bytes on arm64. Update every current consumer,
C mirror, manifest and loading/install path; historical versions remain evidence.
The host prepares Namespace before id_init and obtains the names capability from
the initialized floor, then attaches it only to declared consumers. Native
helpers must follow the same explicit lifetime/dependency contract. No fallback,
aggregate compatibility API, new language protocol or image encoding is added.

Validate against installed 81AW using separately built historical clients, not
by casting one table version as another. Exercise aliases, nulls, rebinding,
borrow lifetime, callback/context replacement, copied native contexts, GC and
independent runtime state. Verify both accepted and declined Block admission,
Image/Alien/Reflect consumers, startup, source-free compiled caller, v8 native
Code image and baked executable in a fresh debug installation. Commit only after
these focused checks pass. No release validation.


## Implemented boundary

- Namespace storage v2 is 96 bytes and creates/destroys the per-runtime names
  service. It copies the selected storage reverse operation and opaque context;
  neither service creation nor destruction calls the floor. Destroying a service
  uses its stable API address even after the live context has changed.
- Names v1 is 32 bytes; its floor access is 40 bytes. Default lookup preserves
  preflight, selected reverse lookup and borrowed Symbol-name order, including
  null and missing values. Unit runtime selection remains independent after
  completed bootstraps move.
- IdApi v8 is 33 fields / 264 bytes. The reverse-name field and production Id
  forwarding implementation are gone. Image's fourteen calls and Alien,
  Codegen/Reflect and Portable Code use the explicit live capability. Current
  C clients and Rust native-test helpers use v8; historical manifests and C
  fixtures remain intact.
- Host preparation uses Namespace factory v2 before id_init and validates the
  initialized names provider before mounting its four declared consumers. Normal
  Namespace mounting does not reset state. Id has no normal Rust dependency on
  Namespace's implementation.
- Block construction declaration v3 compares twelve operands, including the
  original names operation and context. A changed operation or context declines
  without invoking it; restoring the originals restores admission. No v2 producer
  export or fallback remains. Image encoding and language protocols are unchanged.

## Validation

All builds and execution checks used debug artifacts. No release validation or
full install-smoke run was performed.

- Workspace check/build passed. Focused tests passed: ABI 5; Namespace 5 with
  1 diagnostic ignored; host 86 with 1 ignored; builder 4; Id 245 with 15 ignored,
  plus the added moved-runtime names isolation regression. ABI/Namespace strict
  Clippy passed. The broader strict Clippy command encountered five pre-existing
  Artifact/Artifact Codec lint categories; the same diagnostics were reproduced
  from the installed 81AW source snapshot. That snapshot also lacks a native
  compiler build input, so it is not evidence of a passing broader lint run.
- The independent namespace oracle passed 2,048 ordered binding cases and 4,096
  independent storage cases, including absent-name interning, null-valued slots,
  alias order, rebinding, enumeration, selected storage callbacks and precise GC
  with an active stack boundary. Names tests cover malformed floor access,
  callback order, copied access, borrowed returns, independent services, live
  operation/context changes and destruction after context replacement.
- Independent C admission checks passed for all four consumers: malformed
  descriptors and retargeting reject; separate copied descriptors and repeated
  attachment succeed. Host loader negatives cover missing/invalid factory,
  rejected floor dependency and missing/invalid initialized names capability.
- Separately compiled historical v7 fixtures from commit ecf07b37 and current v8
  fixtures produced identical installed traces for Namespace, Source Loader,
  Native Loader, Block Loop, File IO, Environment, System Input/Output/GC, Error
  Value, numeric error consumers, Text View and Printing, including their existing
  callback-reentry modes. Historical fixtures were built against their historical
  headers and support code; no table casts or aggregate adapters were used.
- Independent v8 C client/module, numeric/Boolean native fixtures, both native
  Block-construction modes and nine selected-floor discovery cases passed.
  Block checks include a separately attached copied names descriptor, each of
  its live operands changed independently, no callback on decline, and recovery
  after restoration. Image's compatibility authority passed ten tests and permits
  only the complete name-call substitution with unchanged operands and one
  dependency declaration.
- Installed Reflect observes an alternate native name and null; Alien wrapper
  recording and Image saving observe replacement operation/context. Namespace
  startup, source-free compiled caller, v8 native Code image and baked executable
  pass, preserving aliases, nil/null distinction, roots, enumeration, reload and
  separation from Globals. Source Loader's corresponding startup/compiled/image/
  baked regression also passed. Standard library: 145 passed, zero failed.
- Final fresh prefix: `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81ax-prefix.m0dwenel`.
  It contains 43 native libraries and 28 authored modules. All 1,750 source hashes
  stayed unchanged during installation and final execution checks; only this
  validation record and the v8 fixture README changed afterward. The earlier prefix's additional consumer
  regressions used the same production sources; the refresh incorporated two
  test-only lint fixes and the Codegen export inventory update. Final-prefix ABI,
  consumer, startup/compiled/image/baked checks passed again. Exact export deltas,
  Namespace/normal-Id inventories and system-only linkage passed. The development
  Id inventory was checked after a standalone Id build, avoiding workspace feature
  unification of Artifact's module wrapper.

Receipts are `.cargo-target/81ax-{unit,runtime-names,focused-clippy,clippy,
baseline-clippy,namespace,names-native,host-validation,image-authority,
prefix-native,prefix-inventory,installed-abi,floor-discovery,stdlib,
source-loader-image,final-images,install}.log` and
`.cargo-target/81ax-source-hashes.json`.

The next bounded piece is raw namespace enumeration: move count/name-at and
Reflect's sole production consumer behind an explicit Namespace capability.
Import, export and presence remain later extractions.
