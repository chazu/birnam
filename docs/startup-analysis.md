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
