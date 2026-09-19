# 81AV — Source Loader owns System load:

> **Status:** Implemented, following Native Loader in 81AU.

Move load: and its source-reading/staging/evaluation/diagnostic helpers unchanged
from Prelude into Source Loader. Preserve the one-GiB cap, exact borrowed path,
owned source bytes, Codegen selection and sourceRevisionOf:, at:,
recordModuleSource:named:, evalPersisting:from:, lastError and markLoaded: order.
Keep lazy Error recognition/construction and diagnostics, including path borrows
across callbacks. Do not combine the read with File IO or add loading policy.

Use the 24-byte installer with registration/execution tables and selected System
behavior/load: selector. Prelude retains System/Block identity, capture layout,
creation/adoption, selector interns and original binding order. Retain its first
execution pointer before callbacks. Move actual Integer, String, Text View and
Error Value consumers and remove those four now-unused Prelude attachments.
Source Loader initializes only its provider and loads before Prelude. No IdApi,
representation, public protocol or image format changes.

Compare independent native traces against installed 81AU, including binary
source, staging outcomes, diagnostics, live callbacks, snapshots, reentry and GC.
Verify a fresh external debug startup, source-free compiled caller, saved v8
native Code image and baked executable, then commit. No release validation.

## Validation

- load: and its five helpers are copied verbatim. All retained Prelude function
  bodies are unchanged except the installation call replacing the load: binding.
  Its four unused semantic attachments move to Source Loader; identity, layout,
  adoption, selector interns and installation order remain in Prelude.
- The independent native fixture passes 524 cases in each of three modes:
  normal, first load:-intern reentry and first print-binding reentry. Complete
  traces match installed 81AU, development libraries and the new installation.
  These include 340 binary source/receiver cases, 64 staging combinations and
  120 evaluation/error/notification combinations. The normal mode also checks
  the exact one-GiB boundary and rejects one additional byte before Codegen lookup.
- Wrong arity, null carriers, inherited/non-String paths, missing files,
  directories, embedded NUL and invalid UTF-8 retain their existing outcomes.
  Stage counts and exact diagnostics verify source revision, name extraction,
  recording, evaluation, lastError, printing and loaded-module notification.
  Null remains distinct from language nil throughout the protocol.
- Live String/Integer operations and contexts, authoritative recognition,
  borrowed extents, null/alias constructor results and current nil are observed.
  Codegen retains its imported snapshot across constructor callbacks. Nested
  successful loads and callback GC preserve owned source bytes and objects.
  Mutations of borrowed path bytes remain visible in later diagnostics.
- Error recognition, missing/null/replaced factories, publication during message
  construction, imported snapshots and custom factory lookup preserve existing
  order and results. A nonprintable Error is still returned unchanged.
- Methods run during registration. Reentry, copied Methods, Prelude reload and
  direct installation retain the original execution table. Mutation during the
  preceding getEnv: binding preserves selected behavior/selector snapshots;
  subsequent installation adopts the new values. Precise GC preserves aliases.
- Descriptor admission rejects null, wrong version/extent, missing install and
  retargeting, while repeat attachment succeeds. Initialization allocates no Id
  objects, methods or roots. Development and installed libraries match the
  seven-symbol inventory, relocatable install name and system-only linkage.
  Checked prior providers keep their export inventories.
- Focused Native Loader, Block Loop, File IO, Environment, Input, Integer
  arithmetic/error/admission, Float and Word numeric/error, Error Value, Output,
  Text View, System GC, Printing and Boolean checks pass, including installed
  comparisons with 81AU. All 145 stdlib assertions pass. ABI tests (four), Prelude
  identity (one), host tests (86, one ignored), builder tests (four) and Source
  Loader Clippy with warnings denied pass.
- The fresh external debug prefix contains 42 native libraries and 28 compiled
  modules. Source Loader is its sole added library. Installed source metadata
  and all 70 required libraries match the refreshed inventory. All 1,718 source
  hashes stayed unchanged during installation; this validation record and a
  README ownership description were updated afterward. No release validation
  or full install-smoke run was performed.
- Installed 81AU and the new prefix pass dynamic startup, a source-free compiled
  caller, a saved v8 native Code image and a baked executable. A native guard
  verifies the load: Method's library. Separate input source is retained and
  changed between runs: restored/baked callers read its new value while retaining
  prior values and aliases. Errors, precise GC and private method/factory
  replacement preserve results. The Block Loop image fixture also passes,
  exercising real Prelude reinitialization after its attachment changes.
- Both installations register the source fixture's Module declaration while
  isLoaded: remains false. Inspection confirms Module is present in Globals but
  absent from Id's export table, so the existing optional markLoaded: path is
  skipped. The native oracle separately exercises that path when Module is
  exported. No registry bridging or behavior change is introduced.

Receipts are `.cargo-target/81av-{check,compare,source-loader,rust-tests,clippy,
prefix,prefix-native,image-baseline,image,block-loop-image,stdlib}.log`, focused
`81av-*-regression.log` files, source hashes and baseline/development/installed
traces for each reentry mode.

## Next boundary

Prelude is now an identity/layout/installation coordinator; further splitting it
would not remove another semantic implementation. Return to the raw export
namespace in Id, distinct from System's Environment module and Store-backed
Globals. Its ordered Symbol/value bindings, same-name replacement, first reverse
alias, enumeration and roots form one state boundary.

Before extracting it, account for its use during Id bootstrap, the weak internal
dictionary mirror, Codegen's native enumeration and callback-free Block admission.
Imports currently intern even absent names, null-valued bindings retain their
enumeration slot, and the namespace roots both names and values. Preserve those
facts and independent Runtime instances. Review the early provider handoff and
actual consumers before deciding the next ABI change; do not substitute Globals,
add a compatibility facade or combine this with Machine/control extraction.
