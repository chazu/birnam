# Birnam startup analysis

Measured on October 1, 2026. The installed release reaches the first terminal
REPL prompt in about **2.3 seconds from the repository** and **2.4–2.5 seconds
from `/tmp`**. Loading native object code, reconstructing the image heap, and
constructing the initial dispatch generation account for most of this time.
Activating the REPL module itself takes about 6 ms.

## Measurement scope

The profile covers process launch through the first `birnam> ` prompt, using a
pseudo-terminal. It includes native bootstrap, the installed bootstrap image,
REPL dependencies, console configuration, and initial generation publication.
Each measured session then exited successfully with `:quit`.

The executable was `/Users/chazu/.local/opt/birnam/bin/birnam`, built with the
release profile on macOS 14.6, arm64. Its installation receipt records commit
`4ef2e58d85625760cb36cca460e4b1089347bbf4` and `git_clean=false`: the installed
source snapshot included uncommitted changes. Relevant installed Host, Image,
Codegen, and native compiler source files matched the checkout when profiled.

There were 28 uninstrumented launches covering bare `birnam` and `birnam --repl`
from both directories. Phase attribution used five runs per directory with
timing checkpoints in an isolated copy of the installed source. Additional
instrumentation measured object loading, heap reconstruction, and metadata
reads. Native stack samples covered early and late startup.

Filesystem caches were not flushed. These measurements describe repeated
startup, not a controlled cold-cache experiment. The repository manifest had
no configured application modules or main, so application initialization was
outside this profile. A separate readiness check evaluated `[1 + 2]`, received
`3`, and reached the next prompt in 11 ms.

## Timing breakdown

The following are representative medians from the phase-instrumented release
build. Per-phase medians are computed independently and may not sum exactly.

| Startup work | From repository | From `/tmp` |
| --- | ---: | ---: |
| Load native object bundle and initialize methods | 952 ms | 948 ms |
| Reconstruct image heap | 712 ms | 702 ms |
| Construct and adopt initial dispatch generation | 389 ms | 520 ms |
| Initialize console capabilities and configuration | 58 ms | 113 ms |
| Activate REPL and dependencies | 6 ms | 6 ms |
| Native bootstrap | 27 ms | 27 ms |
| Image structural and checksum validation | 47 ms | 47 ms |
| Remaining image publication and recovery | 46 ms | 46 ms |
| Remaining launch, image read and cleanup, and greeting | ~48 ms | ~39 ms |
| **First prompt** | **2.29 s** | **2.45 s** |

The uninstrumented results confirm the overall scale:

| Series | Directory | Command | Runs | Median | Range |
| --- | --- | --- | ---: | ---: | ---: |
| Initial | Repository | `birnam --repl` | 5 | 2.351 s | 2.277–2.441 s |
| Initial | Repository | `birnam` | 5 | 2.297 s | 2.271–2.334 s |
| Initial | `/tmp` | `birnam --repl` | 7 | 2.493 s | 2.456–2.586 s |
| Initial | `/tmp` | `birnam` | 5 | 2.515 s | 2.497–2.558 s |
| Final confirmation | Repository | `birnam --repl` | 3 | 2.255 s | 2.244–2.325 s |
| Final confirmation | `/tmp` | `birnam --repl` | 3 | 2.383 s | 2.371–2.400 s |

Bare and explicit REPL startup have similar costs in this configuration.

## Native object loading

The bootstrap image is 88,981,283 bytes and contains 84,437,223 bytes of native
object code. Its 985 portable programs and 371 named bindings produce
**2,341 separate objects**: two implementations per program, resumable and
synchronous, plus a named adapter for each binding.

Startup loads each object separately. It creates a temporary directory and
file, writes the object bytes, constructs a fresh TinyCC state, configures
output and SDK paths, reads and relocates the object, then removes the temporary
file and directory. Stack sampling shows filesystem calls and repeated macOS
SDK discovery through `xcselect_host_sdk_path`.

Across the complete REPL session, including foreign wrappers, the detailed
instrumentation counted 2,372 object loads and zero `Unit::compile` calls.
The expensive path loads precompiled objects rather than recompiling C source.

Detailed medians from `/tmp` split the internals of `load_object` as follows:

| Work inside object loading | Time across 2,372 loads |
| --- | ---: |
| Create directories and write object files | ~170 ms |
| Configure TinyCC output, including SDK discovery | ~166 ms |
| Read object files with `tcc_add_file` | ~53 ms |
| Relocate code | ~144 ms |
| **Total inside `load_object` before file cleanup** | **~533 ms** |

These are subsets of startup, not additional costs. Temporary file cleanup,
buffer copying, language messages, root management, and method initialization
contribute to the rest of the approximately 950 ms bundle-loading phase.

Relevant implementation:
[bundle construction](../rust/birnam-codegen/src/image.rs),
[per-object loading loop](../rust/birnam-codegen-runtime/src/image.rs),
[native object loader](../rust/birnam-native-compiler/src/unit.rs), and
[macOS SDK discovery](../vendor/libtcc-src/tccmacho.c).

## Heap reconstruction

The image restores **17,994 objects with 264,376 fields**. The largest object
families are 7,635 `VectorNode` objects and 3,788 `VectorVt` objects. The field
count includes all serialized node fields, not just mutable slots.

Detailed timings divide the approximately 700 ms phase into:

| Heap restoration work | Time |
| --- | ---: |
| Decode nodes and allocate object shells | ~220 ms |
| Construct CodeObjects and Blocks | ~29 ms |
| Resolve references and restore slots and captures | ~430 ms |

Most of the cost is allocating the graph and wiring its references through
messages. Native samples show Id dispatch, byte hashing, root retain and
release, and allocation. The [image loader](../rust/birnam-image/src/native.rs)
restores individual slots with `restoreImageSlotAt:in:put:`.

## Initial dispatch generation

Generation construction and adoption take about 390 ms from the repository
and 520 ms from `/tmp`. `initialGenerationProjection` reads the compiled
metadata for ten modules. `adoptInitialGeneration` then traverses the source
map and reads that metadata again.

Each read unpacks relational Vectors, allocates Integer indices, sends
messages, retains roots, copies values, and validates source digests. Late
stack samples show `source_metadata::decode` and garbage collection during
adoption. The authority construction step following revision materialization
took only about 4 ms in the detailed trace.

One adoption read of `compiledUnit` took about 176 ms from `/tmp`, compared
with 28 ms from the repository. Sampling shows collection inside metadata
decoding, but the exact cause of the directory-dependent timing difference
was not isolated.

The relevant paths are
[initial projection](../rust/birnam-codegen-runtime/src/source.rs),
[source map validation](../rust/birnam-generation/src/candidate.rs),
[generation adoption](../rust/birnam-generation/src/candidate_ledger.rs),
and [compiled metadata decoding](../rust/birnam-codegen-runtime/src/source_metadata.rs).

## Optimization priorities

These are candidates inferred from measured costs. No optimization was
implemented, and the phase budgets are not guaranteed savings.

1. **Reduce independent native object loads.** Investigate consolidating the
   bundle into fewer linker states, admitting objects from memory, and avoiding
   repeated SDK discovery. Review whether both implementations of every
   portable program must be loaded, preserving explicit synchronous and
   resumable execution semantics. This addresses the largest phase, about
   950 ms.
2. **Avoid decoding unchanged metadata twice during generation publication.**
   Consider passing a validated snapshot through construction and adoption,
   or persisting a canonical byte representation. Preserve Store authority
   and invalidation when images or modules change. This addresses the
   390–520 ms phase and its allocation and collection costs.
3. **Batch heap reference restoration within ObjectModel ownership.** Reduce
   per-slot messages and repeated symbolic lookup and root operations while
   retaining the Id-message contracts. The reference-wiring portion alone
   costs about 430 ms.

The earlier source-parsing and linear symbol lookup hotspots are not the
dominant paths in this profile. Compiled metadata is restored from Store,
and symbol interning already has a hash index.

## Evidence and reproduction

Raw evidence is retained locally under
`.cargo-target/measurements/startup-20261001/`, following the
[measurement storage policy](measurements/README.md). It is ignored by Git.
The directory contains baseline and phase JSON, detailed counters, native
stack samples, installed artifact identities and hashes, measurement scripts,
and the temporary instrumentation diff. Peak RSS in the baseline JSON describes
the complete process, including teardown, rather than memory at the prompt.

For a fresh uninstrumented launch-to-prompt measurement, run this from the
directory whose startup configuration should apply:

```sh
python3 - <<'PY'
import os, pty, select, subprocess, time

for _ in range(7):
    master, slave = pty.openpty()
    started = time.perf_counter()
    process = subprocess.Popen(
        ["birnam", "--repl"], stdin=slave, stdout=slave, stderr=slave
    )
    os.close(slave)
    output = b""
    try:
        deadline = started + 30
        while b"birnam> " not in output:
            remaining = deadline - time.perf_counter()
            if remaining <= 0 or not select.select([master], [], [], remaining)[0]:
                raise RuntimeError("REPL prompt timed out")
            chunk = os.read(master, 65536)
            if not chunk:
                raise RuntimeError("REPL exited before the prompt")
            output += chunk
        print(f"{(time.perf_counter() - started) * 1000:.1f} ms", flush=True)
        os.write(master, b":quit\n")
        process.wait(timeout=10)
        assert process.returncode == 0, output
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
PY
```

The retained `instrumentation.diff` documents the phase checkpoints used in
this profile. For later comparisons, record the installed executable, image,
source identity, build profile, and configuration again: these results belong
to the measured installation snapshot.

## October 4: ImageFormat rewrite regression

The unfinished, uncommitted Birnam ImageFormat implementation introduces a
minute-scale image-admission bottleneck in `validateHeaps:programs:inventory:bindings:behaviors:`.
This diagnosis used an isolated prototype prefix; the installed user binary was
not replaced. The exact reported 25-minute startup was not run to completion.

Bounded measurements on macOS arm64, from `/tmp`:

| Check | Observed time |
| --- | ---: |
| Installed `/Users/chazu/.local/bin/birnam --repl`, first prompt | 2.748 s |
| Last committed AlienCompiler rewrite prefix, first prompt | 6.210 s |
| Rust ImageFormat inspecting the complete 110,675,622-byte bootstrap image, including process launch | 0.348 s |
| Birnam ImageFormat: process launch through inventory inspection | 0.829 s |
| Birnam ImageFormat: through heap decoding | 2.905 s |
| First validation pass, first 1,000 objects | 11.402 s additional |
| First validation pass, next 1,000 objects | 10.893 s additional |
| First validation pass only, objects 9,000–9,099 | 10.783 s additional |

The last row is a diagnostic subset, not complete image admission. That subset
contains field-heavy objects. The full image has 23,324 objects and 314,281
fields. Its first 2,000 objects contain only 591 fields, so extrapolating from
those early objects substantially understates the total cost.

Heap validation performs two whole-object passes, with nested field loops,
error checks, tag-membership tests, reference checks, and native view access.
Those small operations produce many language messages and temporary objects.
A two-second native stack sample during the first pass contained 1,496 samples;
at least 896 leaf samples (59.9%) were in Id dispatch, binding, dictionary lookup,
and their hashing operations. Allocation, root management, and collection also
appear. The slow phase is after portable-inventory inspection and byte decoding.

A controlled temporary variant reused the numeric tag vectors rather than
constructing them in each check. Its first 1,000-object pass fell from 11.402 to
9.765 seconds, and the next from 10.893 to 9.560 seconds. This is a modest gain,
not a solution to the overall per-field message and allocation volume.

The temporary variants do not bypass validation in production. No runtime fix
or successful full authored-startup timing is claimed here. ImageFormat should
not be committed as a completed rewrite until full-image admission and startup
have an acceptable measured cost. Small-format equivalence fixtures alone do
not establish this.

Evidence is under `/tmp/birnam-imageformat-rewrite/`: `startup-diagnosis.log`,
`startup-diagnosis.sample`, `cached-diagnosis.log`, and `dense-diagnosis.log`.
These files are temporary and are not repository artifacts.

## October 4: source cleanup and first guarded optimization pass

ImageFormat now constructs its tag sets and validation labels once, uses bounded
Integer iteration instead of per-index Cells and condition Blocks, and removes
small forwarding calls from the field loops. The separate validation walks and
first-failure ordering remain. Unsigned fact comparisons now subtract same-sign
operands before comparing with zero, preserving exact u64 ordering despite the
language's floating-point rounding in ordered Integer comparisons.

The next pass changes three shared mechanisms:

- Id specializes the actual built-in lookup implementation. Every send rechecks
  the lookup implementation and metabehavior; replacements and custom lookup run
  the ordinary metacircular path. It caches no foreign method contexts.
- Id's private address indexes use a small integer hash. Symbol-name hashing and
  ordered method/Symbol enumeration are unchanged.
- Emitted C caches constant selector/name Symbols, retained by their TCC Unit.
  Namespace/Globals reads and method sends remain live. Explicit source Symbol
  construction still uses ordinary interning.

These are guarded dispatch and emission improvements, not general small-callee
inlining, closure elimination, unboxed loops, or a completed deoptimization tier.
No new recovery frames are required by the dispatch shortcut: a declined guard
continues the original send before any source method has executed.

Limited wall-clock timings, one run per cell, compilation excluded:

| Workload | Original source/runtime | Source cleanup | Cleanup plus runtime/emission |
| --- | ---: | ---: | ---: |
| First heap pass, first 1,000 bootstrap objects | 12.05 s | 9.85 s | 4.83 s |
| First heap pass, bootstrap objects 9,000–9,099 | 11.44 s | 9.01 s | 4.59 s |
| Both heap passes, complete admission of 128 objects with 4,096 nil fields | 10.37 s | 8.66 s | 5.37 s |

The source changes save about 17–21%; the combined changes save about 48–60%
on these workloads. These are diagnostic timings, not cold-start benchmarks.
The small complete image is synthetic and has no code/behavior/fact records.
The full 110 MB bootstrap admission still exceeded a 120-second deadline after
cleanup and optimization; the process was terminated. No improved full-bootstrap
or REPL startup time is claimed, and the installed user prefix was not changed.

Validation: workspace cargo check; six Id unit tests (including lookup replacement,
method replacement and custom metabehavior fallback); three backend unit tests;
production compiler and memory integration checks including collecting callbacks,
paused frames and code reclamation; 132 ImageFormat admission cases and 16 exact
roundtrips against the frozen Rust reference. The previous authored artifact
incorrectly admitted the new adjacent-u64 rejection case; the corrected source
passes it. Exhaustive suites and a fresh production installation were not run.

The bounded reproduction tool is [image-format-bench.py](../scripts/image-format-bench.py).
By default it runs complete admission; `--range START COUNT` deliberately measures
only the first heap pass. The original source snapshot, synthetic image, timing
JSON and native library hashes are retained in the ignored directory
`.cargo-target/measurements/image-format-optimization-20261004/`.

## October 4: compact tables, fact validation and compiler checkpoint

The next source pass removes the per-field language walk from bulk framing and
membership checks. ImageFormat now asks Bytes-owned compact tables for invalid
tags, reference bounds, offsets and sparse projections. It creates row/field
objects only for checks that need them. Fact records use the same generic framing
and scanning operations instead of individually boxing and dispatching every
cell. Image policy remains in `modules/image-format.birnam`: allowed tags,
CodeObject/System rules, portable inventory, closure checks and rejection order.

The native table operations hold only owned bytes and scalar metadata, with no
language-object edges. All cross-owner calls use Id messages; numeric conversion
and result construction still call the live Integer owner. These operations
introduce no ImageFormat-specific native validator or foreign payload access.

Bounded measurements on the same 110,675,622-byte bootstrap image:

| Workload | Wall time |
| --- | ---: |
| Complete admission after compact heap tables, before fact cleanup | 20.156 s |
| Complete admission after fact cleanup, including process launch | 2.235 s |
| Heap-validation portion of that final run | 1.614 s |
| Temporary prefix REPL launch to first prompt | 6.661 s |

These are single-run measurements. Admission compilation is excluded. The REPL
measurement uses an isolated prefix with updated libraries and ImageFormat and
the previously built bootstrap image; it is not a fresh installer/image build.
The installed user binary remains unchanged. The old full-admission run exceeded
120 seconds; the reported 25-minute startup was never measured to completion.

The first compiler optimization handles uncaptured, allocation-free helpers
whose result is their receiver or an argument, including local aliases. The
compiler proves this from its existing small-callee analysis and attaches a
companion through Method's existing inline protocol. Execution retains current
method selection, frame admission and baseline fallback. Live Namespace and
Integer permissions guard omitted indexing; replaced owners and bounded frame
execution take the ordinary path. This is a limited first pass, not a completed
general optimization/deoptimization tier.

A focused benchmark of 180 calls measured 9.736 ms and 5,760 allocations for the
baseline versus 7.787 ms and 4,680 allocations with the companion: about 20% less
time and 19% fewer allocations. This does not imply a 20% startup improvement.

Validation: 132 ImageFormat admission cases and 16 byte-exact roundtrips; full
bootstrap admission; REPL prompt and clean quit; production compiler integration
including the alias-return candidate, replaced Integer/Machine callbacks and
bounded-frame collection; compact-table tests with live collecting/replaced
Integer methods; workspace cargo check. Fixed-record framing tests cover byte
offsets, truncation, unknown tags, trailing records and overflowing bounds.
Only the checks required for these changes were run. Timing evidence is in
`/tmp/birnam-imageformat-second-pass/full-bootstrap.json` and the temporary prefix
is `/tmp/birnam-imageformat-second-pass/prefix`.

## October 4: fresh installed startup confirmed

The normal release installer rebuilt the current native and authored modules,
generated new bootstrap and tools images, qualified both in fresh processes,
and atomically published `/Users/chazu/.local/opt/birnam`. Cargo reuse was
explicitly enabled; no prior Birnam image was used to generate these images.
The existing `/Users/chazu/.local/bin/birnam` symlink now reaches this build.
The exact installed manifest, file bytes and executable passed the receipt check.

The new bootstrap image is 139,473,346 bytes; the tools image is 224,129,556 bytes.
Using the installed launcher from `/tmp`, with no BIRNAM environment overrides,
three REPL launches reached the first prompt in 6.026, 5.853 and 5.722 seconds
(median 5.853 seconds). Every session evaluated `[1 + 2]` to `3` and quit with
status zero. The first timing overlapped a separate admission probe; the later
two did not. These are ordinary process launches with warm filesystem caches,
not machine-reboot cold-start measurements.

Complete admission of the fresh bootstrap image took 2.675 seconds including
process launch, with 1.940 seconds in heap validation. FizzBuzz produced all 20
expected lines, and the collections example passed its sum and map-lookup checks;
both exited cleanly with no stderr. The installer's console and `doc stdlib`
probes also passed. No exhaustive suite was rerun for this installation check.

The minute-scale admission regression is therefore resolved in the actual
installed build. Startup remains roughly six seconds; this does not establish
parity with the earlier 2.748-second installed baseline. The working-tree changes
have not been committed or pushed as part of this installation.

Temporary evidence: `/tmp/birnam-fresh-install-20261004.log`,
`/tmp/birnam-installed-startup-20261004.json` and
`/tmp/birnam-fresh-admission-20261004.json`. The bootstrap image SHA-256 is
`988efd2cde052c4201639c9b9c3b3d6505ccee3de7ebc0d8720806755d4cd999`;
the installed manifest SHA-256 is
`1468cc9a8e64ff9532c13592937c41f1941c4bf4d81dc08f6c9278c0cbe2f96e`.

Post-install boundary audit: the changed String, Integer and NativeBackend
libraries retain their declared exports and have no cross-owner dynamic linkage.
Generic table scans inspect only their owner's byte snapshot; no foreign Oop
payload is decoded, and numeric conversion uses current Id messages. The alias
companion is registered through `Method withInline:at:owner:`; Execution retains
selection, recovery authority, rooted arguments and baseline fallback. Its local
scoped-root list updates invoke no external callbacks. Production compiler and
table replacement/collection checks passed again using copies of the installed
libraries, with only TCC replaced by its resource-counter build because the
compiler test requires opt-in diagnostic messages. The ordinary installed TCC
does not expose those counters; the installed prefix was not changed by this
audit. This is focused contract evidence, not an exhaustive proof of all language
invariants.

## October 11: startup memory investigation

A script containing only `[System println: "ready"]` reproduced the log
application's roughly 2.4 GiB peak. Disabling authored-image startup removed most
of that peak. The 4 KiB Id lookup-hint table is unrelated: it is fixed per runtime.

The diagnostic bootstrap image is 232,611,241 bytes (222 MiB), of which
224,835,879 bytes (214 MiB) are its native-code section. Phase probes found
RSS rising from about 352 MiB before image admission to 1.26 GiB after admission,
then 2.37 GiB during native-code loading. Heap reconstruction added comparatively
little. These probes were removed from the production sources after diagnosis.

Two problems amplified this image size. File bytes, language Bytes carriers,
validated Rust snapshots and native-object copies overlapped. Several carriers
were rooted for the entire restoration operation, even after their last use.
Also, GC counted the small Bytes object payload but omitted its external vector
capacity: hundreds of MiB could be allocated without that storage contributing
to the 16 MiB collection budget. A diagnostic collection after validation released
about 640 MiB; that forced collection was not retained.

The implemented changes are:

- Release the input file snapshot after validation, and the validated native-code
  snapshot after the current Bytes factory consumes it.
- Scope validation carriers and individual loader inputs to their actual use.
  Keep each returned native unit in the receiving outer scope before dropping
  its input scope. A replaced loader may retain its input independently.
- Decode native objects as slices of Codegen's private immutable bundle snapshot.
  That owned snapshot survives every loader call; foreign byte borrows are still
  copied before callbacks. All objects are validated before loading starts.
- Add the ordinary `GC chargeExternalAllocation:` message. Bytes reports its
  initial vector capacity and subsequent capacity growth after publishing storage
  and finalization, while the object is rooted and no owner-state borrow is held.
  The default handler adds to accumulated allocation work without collecting.
  Existing allocation checkpoints apply the unchanged fixed 16 MiB budget.

Current ImageFormat admission, Bytes construction and native loading remain
ordinary sends. Accounting replacements may collect or reenter; tests cover that
case. Existing identity, image rejection, loader failure, recovery and native-unit
ownership contracts remain in force. These changes introduce no speculative
cached behavior or virtual values requiring additional deoptimization machinery.
Automatic collection remains suppressible, and the budget does not scale.

Four fresh processes per variant, in ABBA order with warm filesystem caches,
produced these medians on macOS arm64:

| Measurement | Baseline | Candidate |
| --- | ---: | ---: |
| Minimal script, authored-image peak RSS | 2.37 GiB | 1.49 GiB |
| Minimal script, authored-image wall time | 2.98 s | 3.46 s |
| Minimal script, native-prefix peak RSS | 129.4 MiB | 117.6 MiB |
| Minimal script, native-prefix wall time | 0.222 s | 0.263 s |

Peak image-startup RSS fell **37.3%**, approximately **905 MiB**. Startup became
**15.8% slower**, about 0.47 s, as byte allocations now cause collections through
the fixed budget. Retain this as a memory-accounting correction and memory
improvement, with an explicit latency cost. It is not a startup-speed improvement.

Sustained processing of the same 50-record input remained approximately unchanged:
an ABBA comparison, with five timed iterations per process and explicit GC before
each iteration, gave median wall times of 2.0057 s before and 1.9907 s after.
Both variants used the same published application artifact and matched the
independent oracle. The small difference is not a meaningful speedup claim.

A separate process held at an input handshake used about 1.79 GiB before explicit
GC on the baseline and 1.24 GiB on the candidate. After GC it used about 1.16 GiB
and 1.02 GiB respectively. These are single diagnostic observations, not live-heap
sizes. A TCC metrics build reported 4,071 live units, 271 MiB of relocation
allocations and 302 MiB of total requested compiler heap storage. Those measures
may overlap and must not be added.

The remaining footprint requires another investigation. First distinguish live
allocations from allocator-retained pages. Then examine image/Bytes copying and
sharing, the size and number of native units, and metadata retained by their
compiler owners. Any code-storage reduction must keep active and suspended code,
recovery metadata and current late-bound behavior valid. Do not infer that all
remaining RSS is TinyCC scratch, or increase/scale the GC budget to mask it.
External accounting currently covers Bytes-owned vectors, including String
backing; it does not yet cover every native owner or shared indexed-table storage.

Validation passed 71 Codegen unit tests and 13 production native tests, including
mapped/rebuilt/stripped image restoration, precise collecting callbacks, injected
loader/publication failures and the repeated save/reload lifetime chain. The
source startup path (`BIRNAM_FORCE_SOURCE=1`) and the log application's valid and
invalid fixtures, project tests, deterministic output and automated edit/rebuild
cycle passed. Formal checks passed 168 Lean theorems, 109 TLC cases and 30 rejected
Lean mutations. New proofs cover capacity-growth accounting and owned-slice
contents; existing scope/borrow models check lifetime obligations. They verify
abstract contracts, not the compiled Rust implementation.

Reproduce the bounded startup measurement with:

```sh
python3 scripts/startup-memory.py --prefix /path/to/prefix \
  --output /tmp/startup-memory.json --runs 3
# Optional platform-specific regression ceiling:
python3 scripts/startup-memory.py --prefix /path/to/prefix \
  --output /tmp/startup-memory.json --runs 3 --max-rss-mib 1800
```

The [evidence record](measurements/startup-memory-20261011.json) includes individual
samples, artifact hashes, formal-result provenance and local diagnostic paths.
The candidate changes four native owners in an isolated diagnostic prefix while
using the same bootstrap/tools images. These measurements do not constitute installation, remote publication or
release qualification. The source-startup check is not a complete cold-install
build qualification.
