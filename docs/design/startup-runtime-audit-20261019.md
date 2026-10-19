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
