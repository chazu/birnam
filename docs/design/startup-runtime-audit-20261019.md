# Startup and runtime audit — 2026-10-19

Baseline: `cc9a82a9`, after the candidate-policy migration. The investigation
separates startup, warm application execution and explicit collection. It uses
the existing JSON and Logue oracles, native stack sampling, and disposable native
overlays. No change to the user installation is part of this work.

## What startup actually loads

Rust owners are macOS dynamic libraries. Authored artifacts, including some files
named `.dylib`, contain relocatable ELF objects. Saved images embed many such
objects: resumable and synchronous program bodies, plus named adapters. Loading
these bytes does not compile C, but it must relocate the supplied machine code.

Previously, `TCC loadObject:` copied the input Bytes to a Rust buffer, created a
temporary directory/file, wrote the buffer, asked libtcc to reopen/read it, and
removed the file/directory. The input was already compiled and already in memory.

| Bootstrap image | Programs | Objects | Image size |
| --- | ---: | ---: | ---: |
| Before source-registry migration | 1,728 | 4,139 | 198.67 MiB |
| Authored source registry | 1,789 | 4,281 | 206.29 MiB |
| Candidate-policy migration | 2,009 | 4,758 | 244.14 MiB |

The candidate-policy port added 477 objects and roughly 38 MiB to the image.
This is a direct increase in loading work, independent of registry-query costs.
Visible early-startup stacks include ImageFormat inspection; late-startup stacks
are dominated by object loading. Native samples show substantial file-system
calls, copying, Id binding and GC bookkeeping. Separate sampling windows overlap
and contain incompletely symbolized generated code; their counts are not an
additive timing breakdown.

## Retained change: memory input for the existing relocator

`tcc_add_object_memory` supplies a bounded borrowed buffer to the same section
merger and relocator used by `tcc_load_object_file`. A small input adapter handles
memory ranges or existing file-descriptor reads. Section/symbol data retained by
libtcc is copied into its own storage; the input is not retained or executed.
Malformed headers, truncated ranges and invalid section/string references are
rejected with normal cleanup. This remains a trusted-native-code interface.

The TCC owner obtains the buffer through the current `bytes` message and finishes
consuming it before another Id send. Each loaded Unit still has independent
mutable native data. Methods, entered frames and suspended/recovery frames retain
their actual code owners; mapping retirement and deoptimization are unchanged.
There is no new language service, foreign-layout access or cached dependency.

General libtcc file/archive APIs remain available. Birnam's documented
`loadObject:` route consumes one relocatable object, not a general library/archive
container. The existing artifact/image format is unchanged. `compileObject:` still
uses file output: libtcc unlinks/recreates its output path, so its existing output
directory cannot simply be removed. System-library resolution also remains with
libtcc. The eliminated work is object staging, not all process file access.

Five interleaved fresh-process measurements per variant, after warmups, used
identical preexisting authored artifacts/images and precise GC:

| Workload | Baseline wall | Memory loader wall | Baseline CPU | Memory loader CPU |
| --- | ---: | ---: | ---: | ---: |
| Startup plus trivial script | 4.780 s | 4.031 s | 4.715 s | 4.024 s |
| Saved image and its compiled main | 5.206 s | 4.446 s | 5.138 s | 4.442 s |

These diagnostic overlays save about 0.75 seconds (15–16%). Background user work
was active; the matching CPU reduction supports the direction, but these are
local measurements, not a release benchmark or a universal speedup claim.

## Warm runtime findings

The current 50-record JSON workload passes independent result checks for each
prepared phase. Times exclude startup, preparation, input I/O and the explicit GC
preceding each measured invocation; automatic GC remains enabled.

| Phase | Median thread CPU |
| --- | ---: |
| Full parse/aggregate/render | 279.47 ms |
| JSON parse/lower | 153.10 ms |
| Native PEG matching | 11.69 ms |
| Lower prepared trees | 175.23 ms |
| Validate prepared records | 45.38 ms |
| Decode quoted strings | 92.47 ms |
| Unescape prepared bodies | 66.45 ms |
| Convert prepared number nodes | 39.58 ms |

These are independent workloads and are not additive. The full pass's separate
no-auto-GC census charges about 2.75 MB / 104,680 objects. String decoding remains
material, but the old 1.6-second unescaping result is no longer representative.

Logue passes its 34-check regression and the independent full-state oracle for
quiet/crowded turns, dungeon generation and live collection-method replacement.
Initial normal-GC medians were 38.8 ms per quiet 16-turn batch, 803.3 ms per crowded
batch, and 2.678 s per seeded five-level generation. Timers exclude loading,
fixture creation, snapshots and reporting. Allocation-only passes are separate.

Warm native samples of both applications identify Id binding as the largest
named leaf, with symbol lookup, admission checks, copying and native bookkeeping
also visible. The JSON sample includes explicit collection between passes; its
GC share must not be mistaken for the timed application phase's GC cost. Logue's
sample uses actual repeated turns with normal automatic GC and a checked result;
its sampled interval also includes one final arena reset and collection.

## Experiments not retained

- Replacing general-purpose hashing in private GC and ObjectModel address
  indexes preserved results and traversal order, but did not give a reliable
  end-to-end win. In ABBA runs combined with the smaller file experiment, JSON
  medians were 277.4 → 284.2 ms and crowded Logue 777.1 → 785.5 ms. These results
  do not isolate a regression to hashing; they do not justify retaining it.
- Removing only per-object directories and an input copy saved about 0.11 s of
  startup CPU in the initial matrix, with a much weaker wall-clock result.
  It left the unnecessary write/read round trip. The memory loader supersedes it.
- Reusing a compilation-output descriptor failed the object roundtrip test:
  libtcc replaces the output inode. That experiment was removed; output and input
  are different contracts, and their correctness tests caught the distinction.

## Next targets, in order

1. **Reduce eager code loading and duplicated generated bodies.** The current
   image contains 123.47 MiB of resumable bodies, 95.23 MiB of synchronous bodies
   and 16.69 MiB of adapters. Investigate required-variant emission/mapping or
   shared immutable code within a compilation unit. Preserve native callbacks,
   late-bound future calls, image rebuilding, suspension and deoptimization.
   Do not simply discard the synchronous bodies.
2. **Separate required lookup from repeated dispatch admission.** Id binding is
   a common hotspot in both applications. Investigate owner-maintained facts or
   cheaper access to current dictionary entries, with replacement/custom
   metabehavior and invalidation tests. Replacements must still take effect and
   invalidate optimized paths. Keep that mechanism with its owner; do not add
   monkeypatch guards to authored Birnam code.
3. **Reduce image inspection/copying work.** Trace large buffers through the
   ImageFormat/Bytes/Codegen boundaries and reuse operation-local snapshots when
   their ownership permits it. Avoid a new persistent mirror or foreign-payload
   shortcut. The early-startup profile makes this worth measuring next.

Exact duplicate ELF payloads account for 16.55 MiB (about 7% of object bytes).
Deduplicating stored bytes is distinct from sharing loaded Units: identical object
bytes can initialize different mutable data. The new loader expressly preserves
that independence. A format/storage change may help, but byte identity alone is
not permission to share executable owners.

## Qualification

The [receipt](../measurements/startup-runtime-audit-20261019.json) records profiles,
timing rows, oracle results, driver sources, artifact hashes and validation logs.
All changed production sources match the fresh `final-prefix`; its changed native
binaries match the measured overlay byte-for-byte. Native exports and dynamic
dependencies are unchanged.

- Nine native compiler tests pass normally and with AddressSanitizer on libtcc's
  C implementation. They cover unavailable temporary storage, overwritten and
  unaligned input, independent Unit data, file/memory symbol-resolution parity,
  every truncated prefix of a compiled fixture, invalid ranges, cleanup and actual
  executable mapping lifetime/protection.
- A complete existing-format image fails with a nonexistent `TMPDIR` on the old
  loader and loads/runs successfully on both the new overlay and fresh prefix.
- Compiler, artifact activation, materialization, candidate retirement, borrowed
  input and byte-ABI integration suites pass. Fresh-prefix mapped/rebuilt,
  stripped and resaved image checks pass, as does parser/compiler-disabled startup.
- All 847 language assertions pass.
- Existing formal checks pass: 391 Lean theorems, 122 rejected mutations and 266
  expected TLC outcomes. These check abstract rooting, borrowing and lifetime
  contracts; they are not formal verification of libtcc's C implementation.

This is a cached development build and local qualification, not a cold release
qualification. No user-prefix installation or push was performed.

## Follow-up after memory-loader commit `7547af98`

The [follow-up receipt](../measurements/startup-redundancy-probes-20261019.json)
records fresh profiles, read-only image/ELF inventories, diagnostic patches,
drivers and raw timing rows. These are **opportunity measurements**, not additional
qualified runtime changes. All diagnostic production-source edits were reverted.
The measured prefix and user installation were kept separate.

### Highest payoff: load only requested image body variants

`Codegen` currently maps and initializes both variants of every portable program
before heap restoration. `Image` then requests a particular variant through
`portableImageBlockMethod:synchronous:` for each restored Block. Inspection of the
actual bootstrap heap found:

| Item | Count |
| --- | ---: |
| Portable programs | 2,009 |
| Eagerly loaded body objects | 4,018 |
| Restored Blocks | 787 |
| Distinct body variants referenced by those Blocks | 731 |
| Of those, resumable / synchronous | 513 / 218 |
| Body variants not referenced by restored Blocks | 3,287 |
| ELF bytes in those unreferenced variants | 172.32 MiB |

This census follows Block locators to their CodeObject program indexes; it does
not infer demand from text matches or assume that synchronous bodies are unused.
The objects are still valid code. They are unnecessary for these restored Blocks,
not forbidden from being requested by a replacement loader or another operation.
The existing image stage drops its temporary method roots when loading finishes;
ordinary later materialization uses its separate CodeObject path.

A disposable probe skipped the exact unreferenced indexes for this one known
bootstrap. It preserved positions with `nil` placeholders; **that is not a general
implementation**. The trivial script still ran successfully. Three interleaved
fresh-process runs per variant, after warmups, gave these medians:

| Startup probe | Wall | CPU |
| --- | ---: | ---: |
| Current memory loader, diagnostic switches disabled | 4.037 s | 4.031 s |
| Load only body variants referenced by this heap | 2.761 s | 2.752 s |
| Selected bodies plus runtime/SDK setup bypass | 2.637 s | 2.632 s |

The body omission saves 1.277 seconds, about 32%. This is a measured opportunity
on this image, not an established performance result for a general lazy loader.

**Recommended implementation:** preserve the existing stage and request API.
Keep its private owned object bytes available; load and initialize each body on
its first `portableImageBlockMethod:synchronous:` request, retaining the resulting
Method for repeat requests. Keep adapter handling and Method/Unit ownership with
their existing owners. Release unused bytes with the stage. This needs no new
persistent generation registry, foreign object-layout access, frozen method
lookup, or permanent native language service. Rebuild should avoid emitting and
compiling variants that its stage never requests as well.

Before accepting it, test mapped, rebuilt and stripped images, compiler-disabled
startup, both body dispositions, repeated requests, GC during loader callbacks,
failed initialization and cleanup, custom/replaced loader operations, live-image
partial failure, callbacks and suspended/deoptimizing frames. Preserve the
pre-application boundary for failures of bodies needed by the restored heap.
Measure peak memory too: retaining the input bundle during heap restoration has a
cost, even when it avoids thousands of mappings. Existing rooting/lifetime formal
models should accompany those concrete tests; the omission probe proves none of
these safety or semantic properties.

### Second: repeated native runtime and SDK setup

The new late-startup sample still contains `stat`, `open`, archive reads and SDK
resolution underneath libtcc. `tcc_set_output_type` discovers the macOS SDK for
each memory-output state; `tcc_add_runtime` searches for libc and reads libtcc's
support archive for each relocation. Removing object staging did not remove this
independent work.

All 4,758 ELF payloads in the measured bootstrap refer only to these undefined
native symbols: `memmove`, `memset`, `strlen`, `realloc`, `free`, `strcmp`, and (in
1,669 objects) `memcpy`. This is a census of this image, not a restriction on the
public native compiler.

| Probe with all current body variants | Startup wall | Saved-image wall |
| --- | ---: | ---: |
| Control | 4.033 s | 4.547 s |
| Skip automatic runtime libraries/archive | 3.862 s | 4.368 s |
| Also skip SDK discovery | 3.631 s | 4.161 s |

Three interleaved runs per variant/workload passed their `ready` oracle. CPU
medians fell from 4.025 to 3.626 seconds for startup and 4.537 to 4.149 seconds for
saved-image execution. Skipping SDK discovery **alone failed startup**: the
remaining libc search still needs its SDK stub. The useful change is coordinated
resolution, not deleting that one call.

A production solution should resolve already-resident native symbols and defer
SDK/library/archive work until unresolved dependencies require it, retaining the
existing helper/library fallback. Do not use `-nostdlib` as a substitute: it also
disables libtcc's ordinary `RTLD_DEFAULT` symbol resolution. Arbitrary generated C
can require compiler helpers even though this bootstrap does not. Add fixtures
that actually require those helpers and compare ordinary file/memory loading.
This is a contained libtcc change, not a new inter-module Birnam ABI.

These savings overlap with selective body loading: after skipping unused bodies,
the setup bypass saves only another 0.123 seconds in the paired probe. Do not add
the standalone 0.40-second result to the 1.277-second body result.

### Remaining opportunities and rejected shortcuts

- **Warm dispatch:** both application profiles still put Id binding first among
  named leaves. Ordinary sends repeatedly look up the metabehavior's `lookup`
  implementation before looking up the target method. Investigate cheaper access
  to the current entry or owner-maintained invalidation, preserving replacement,
  custom metabehaviors, parent changes and retirement. No new runtime speedup was
  measured here; this remains the leading application-CPU investigation.
- **Image buffer copies:** ImageFormat slices the large code record and its unit
  suffix; Image and Codegen then take independent private snapshots across Id
  calls. A narrower owned-buffer/slice protocol may eliminate some full-size
  copies. Ordinary borrowed views cannot simply survive sends. Preserve current
  message semantics and measure before choosing a new Bytes API.
- **Generated helper duplication:** the ELF census finds repeated initializer,
  cleanup, selector and rooting helpers. Sharing immutable helpers with explicit
  per-Unit context could reduce bytes, but adds a native calling/lifetime contract.
  Defer that design until selective loading is measured; it is substantially more
  intrusive than avoiding work that the current load never consumes.
- **Exact adapter duplicates:** 16.55 MiB of stored duplicate objects does not mean
  16.55 MiB of redundant live code. The current loader already reuses identical
  selected adapter templates at the same arity. Storage deduplication alone will
  not reproduce the body-loading gain. Byte-identical program objects still must
  not be treated as permission to share mutable Unit state.

At this checkpoint the recommended next slice was demand-driven body loading
within the existing image stage, followed by conditional native runtime setup.
The body-loading implementation and acceptance are recorded below; warm Id binding
remains a separate target for application execution costs.

## Implemented: demand-driven image body loading

Codegen now retains pending inputs in the existing image stage and resolves each
program/disposition on its first `portableImageBlockMethod:synchronous:` request.
Mapped stages share one private immutable bundle snapshot; rebuilt stages retain
canonical backend requests. A successful Method replaces its pending input. No
stage borrow survives an Id call. GC traces only ready Methods, adapters, prepared
bindings and the source owner. Cancellation drops unused inputs and temporary
roots; returned Methods/Blocks and active or suspended frames retain their Units.

Named adapter handling is unchanged. Saving still emits the complete existing
bundle; this slice improves loading and rebuilding, not image size or save-time
code emission. No new service, artifact version, exported native symbol, foreign
layout access or monkeypatch guard was introduced. Current TCC/NativeBackend
bindings are resolved when a pending body is demanded, and a successful cached
Method continues to own that implementation. Unrequested payloads are not checked
for linker errors. Required body failures remain in heap restoration before normal
image application; direct stage requests can retry failures until cancellation.

The regression exercises both dispositions, zero body work at stage creation,
current loader replacement after stage creation, stage-only GC protection,
collecting callbacks and rejected reentry, emitter/load/initializer rejection,
native initializer failure, repeat-request identity, malformed demanded ELF,
cancellation, post-stage execution and complete Unit reclamation. It fails against
the previous loader at the eager-allocation assertion.

Qualification passed:

- Fresh cached development installation: 38 native libraries, five source
  bootstrap modules, 24 authored tools. Installed production source matches the
  worktree. Native exports and dynamic dependencies are unchanged.
- All 847 language assertions, plus compiler, artifact activation/templates,
  materialization, candidate retirement, borrowed-input and byte-ABI checks.
- Image failure/owner/conflict checks; fresh-prefix mapped, rebuilt, stripped and
  resaved roundtrips; twenty successive resaves with a fixed expected live set.
- Parser/compiler-disabled startup and an existing compiled image with nonexistent
  temporary storage.
- Existing abstract formal suite: 391 Lean theorems, 122 rejected mutations and
  266 expected TLC outcomes. These are existing rooting, invocation and image
  lifetime contracts, not a new formal proof of the demand-loader implementation.

The demand-loading slice was committed as `daa8c443`. No user-prefix installation,
release qualification or push was performed. The follow-up libtcc setup work is
recorded below.

The [implementation receipt](../measurements/image-body-demand-20261019.json)
records five interleaved fresh processes per variant/workload after warmups, with
precise GC and identical preexisting image/artifact bytes. The measured overlay
changes only Codegen runtime, using the exact binary from the fresh prefix.
Correctness checks finished before timing; other user workloads were not controlled.

| Workload | Eager wall | Demand wall | Eager CPU | Demand CPU | Wall reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Startup plus trivial script | 4.136 s | 2.973 s | 4.089 s | 2.959 s | 28.1% |
| Existing saved image and compiled main | 4.549 s | 3.615 s | 4.541 s | 3.606 s | 20.5% |

These are the implementation results, replacing the earlier omission probe as the
basis for a delivered speedup claim. They measure startup/image execution, not
warm application CPU or release-wide performance.

Memory is a tradeoff, not a claimed win: median maximum RSS rose from 1,504.4 to
1,524.5 MiB for startup (+20.1 MiB, 1.3%), and fell from 1,556.9 to 1,553.6 MiB for
the saved image. macOS peak footprint fell by about 4.4 MiB and 1.3 MiB,
respectively, effectively unchanged at roughly 1.3 GiB. The private input bundle
now remains available during restoration until its stage is cancelled. There is
no permanent pending-body registry, and the twenty-resave fixed-live-set check
passed. Further large-buffer-copy work remains worthwhile.

The next slice, recorded below, conditions libtcc runtime-library/SDK setup on
actual dependencies. Its standalone diagnostic saving must not be added to the
body-loading saving; the opportunities overlap.

## Implemented: conditional native runtime setup

The private native compiler owner now opts its fresh macOS memory-output states
into resident-runtime resolution. When every undefined ordinary symbol is
available through the current dynamic linker, libtcc skips SDK discovery and the
automatic libc/support-archive search. Actual relocation still resolves symbols
through the same lookup routine. No address cache survives across Units, and no
Birnam method dispatch, Module dependency, image format or exported ABI changes.

Reserved runtime/compiler names always take the original SDK/archive path, as do
unresolved imports, explicit library pragmas and special runtime/linking options.
This preserves compiler-helper precedence and required-library failures. Ordinary
libtcc clients retain eager setup and their existing search-path order. Object-file
emission remains unchanged. This is a small vendored-libtcc adjustment configured
by the existing private native compiler owner, not a new language service.

Review caught and reproduced a defect in the initial prototype: a host-exported
`__builtin_popcountll` could displace libtcc1's helper. The regression exports a
deliberately wrong implementation returning 97; the incomplete optimization used
it where 5 was expected. The retained version takes the archive path and returns
5 (and 64 for all bits set) through both compiled-source and memory-object routes.
Explicit symbols supplied to separate Units remain independent.

Native tests also cover libc-only imports, explicit math-library loading, a
missing requested library with no required symbols, unresolved strong imports,
malformed objects, and exact compiler-allocation cleanup. The reserved-name
fallback is conservative: some internal system symbols use the slower path even
when the host could supply them. It avoids changing runtime-helper selection for
a marginal additional saving.

The [runtime-setup receipt](../measurements/native-runtime-setup-20261019.json)
records the corrected implementation's five interleaved fresh-process runs per
variant/workload after warmups, with precise GC and identical preexisting images.
Only TCC's binary differs; the candidate matches the fresh prefix byte-for-byte.
Correctness jobs finished before measurement. Other user activity was uncontrolled.

| Workload | Prior wall | Conditional setup wall | Prior CPU | Conditional setup CPU | Wall reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Startup plus trivial script | 2.865 s | 2.749 s | 2.859 s | 2.742 s | 4.0% |
| Existing image and compiled main | 3.654 s | 3.558 s | 3.647 s | 3.551 s | 2.6% |

This is an incremental saving on top of demand-driven body loading. It does not
establish a warm-language execution or release-time speedup. Peak memory is
essentially unchanged; the receipt includes maximum RSS and macOS footprint for
every process, and no memory reduction is claimed.

Qualification passed: twelve native compiler tests normally and under ASan,
including independent Unit data, memory/file relocation parity, malformed input,
required-library failures, host/helper precedence and allocation cleanup; all
847 language assertions; compiler, artifact, materialization, retirement and
borrowed-input integration checks; fresh-prefix image roundtrips and
compiler/parser-disabled startup. Existing abstract checks also pass: 391 Lean
theorems, 122 rejected mutations and 266 expected TLC outcomes. They are not a
formal verification of this C linker change. Native exports and dynamic-library
dependencies are unchanged, and installed production sources match the worktree.

This new slice remains uncommitted and was tested in isolated prefixes. No normal
user installation, push or release was performed. The next larger startup target
is duplicate image-buffer storage and copying; warm dispatch admission remains a
separate application-CPU target.
