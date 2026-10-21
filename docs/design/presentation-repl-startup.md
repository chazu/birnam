# Presentation REPL startup: prepare code once

Historical prototype measurement. The custom preparation/cache driver described
below has been replaced by the standard project wrapped-library workflow, including
adjacent and embedded packaging. See [shipping artifacts](../guide/09-shipping-artifacts.md#native-wrappers)
for the current contract. The raw evidence below describes the earlier prototype;
the current startup harness measures `birnam build`/`run`.

2026-10-21, installed Birnam 0.18.0, macOS arm64. The opt-in launcher now prepares
its foreign wrappers and authored modules when inputs change. Three interleaved
fresh-process measurements reached a usable prompt in **4.63 seconds from source
versus 3.18 seconds prepared**, a **31% reduction**. Ordinary REPL startup in the
same series was 2.93 seconds.

| Entry | Median | Range, three runs |
| --- | ---: | ---: |
| Ordinary REPL | 2.93 s | 2.77–3.25 s |
| Presentation, source | 4.63 s | 4.50–4.66 s |
| Presentation, prepared | 3.18 s | 3.15–3.27 s |

Every measured presentation launch also evaluated `[1 + 2]`, observed `3`, and
exited with terminal modes restored. Source/prepared order alternated. The
prepared measurement includes the launcher's cache validation (about 41 ms in
the separate probe); initial preparation is excluded. Filesystem caches were
warm. These observations are local measurements, not a cold-cache or release
benchmark. Earlier development runs had substantially more startup variability.

## Profile

An isolated source launcher added monotonic wall and process CPU checkpoints.
A one-second macOS native sample of application setup also showed Id dispatch
and TinyCC tokenization/compilation. Median phase observations:

| Source startup phase | Wall | CPU |
| --- | ---: | ---: |
| Load Termbox wrapper source and install foreign methods | 512 ms | 511 ms |
| Load Birnam presentation model | 196 ms | 196 ms |
| Load terminal source through its foreign-wrapper declarations | 936 ms | 935 ms |
| Remaining terminal method installation | 7.5 ms | 7.5 ms |
| Initialize terminal | 0.7 ms | 0.3 ms |
| First draw | 5.8 ms | 2.5 ms |

Source loading includes parsing and compilation before its first statement runs,
so the two wrapper rows are not pure foreign-compiler measurements. They identify
the expensive setup region; terminal I/O and painting are small. The compiled
startup test below distinguishes preparation from activation directly.

There are 23 Alien declarations across the existing Termbox adapter and this
example. Ordinary `Alien wrap:` asks `AlienCompiler` to generate source before
consulting its object-code cache. Merely seeding that cache would therefore leave
generation work on every launch. The prepared path instead uses the existing
`Alien installRecipe:` interface, then `AlienRecipes record:method:` for the
same ordinary registration bookkeeping. Each process resolves fresh native
addresses and constructs current Methods; no process pointer is persisted.

## Implemented boundary

`examples/presentation-repl/build.py` prepares three separate ordinary Birnam
artifacts. Keeping the source units separate preserves their lexical/global
lookup boundaries. It replaces the example's recognized literal Alien
declarations with embedded object-code recipes; unsupported declaration shapes
fail preparation. The model and terminal logic remain authored Birnam. Every
normal method send still uses current dispatch.

The native library is bundled beside each immutable artifact set for normal
native-dependency admission. The example's Config main loads a small generated
entry file. The Host continues to own turns, roots, checkpoints and teardown;
the default installation and ordinary REPL are unchanged.

Input fingerprints cover source content, the build driver, the selected installed
binary and installation receipt, and the installed bootstrap's file identity.
Output digests detect damaged artifacts. Moving the checkout or changing the
selected installation invalidates the cache. A fresh preparation in the isolated
cache test took 5.87 seconds. Source/runtime changes pay that build cost once;
warm launches validate and reuse the generated files under the example's
`.birnam`. Compilation failures leave the previous loader and manifest intact
and cause the launcher to fail instead of entering stale code.

## Acceptance

- Prepared module activation passes with `Lang parse:` and
  `AlienCompiler compile:signature:` replaced by failing probes. The original
  source path fails the parser and wrapper negative controls, with observed
  probe invocations. The small driver itself compiles before these guards are
  installed; this does not claim zero parsing anywhere in process startup.
- An isolated cache test covers initial preparation, a warm launch without a
  usable C compiler, source changes with preserved timestamps, observation of
  the newly compiled binding, artifact corruption, failed compilation, and
  runtime-receipt invalidation. It never modifies the user's installed prefix.
- The prepared launcher passed all 24 headless model assertions and the full
  real-PTY interaction sequence: identity reuse, GC, paging, output, diagnostics,
  Unicode editing, multiline entry, resize, expiration and normal/fault cleanup.
  Its follow-up interaction pass measured typing at 3.5–4.2 ms p95, selection at
  5.0 ms p95 and scrolling at 4.8 ms p95. These are output-observed PTY latencies.
- No core runtime code changed. The wider runtime/release suite was not rerun.
  The preceding prototype slice's 26 ordinary REPL golden checks remain separate
  evidence; they are not a qualification of active presentation-image recovery.

The remaining startup time is close to the ordinary image startup cost. Further
large gains should start with profiling the common image/code loading path;
there is little terminal initialization work left to remove.

## Reproduce

```sh
uv run scripts/presentation-startup.py --samples 3
python3 scripts/presentation-cache-test.py
uv run scripts/presentation-repl-test.py --samples 1 \
  --output .cargo-target/measurements/presentation-startup/interaction.json
```

Raw timings, wall/CPU phase checkpoints, compiler-guard outcomes, source/cache
identities and interaction samples are under
`.cargo-target/measurements/presentation-startup/`. The compact
[receipt](../measurements/presentation-startup-20261021.json) binds that evidence
and the measured inputs. The retained native sample is `application.sample` in
the same ignored evidence directory.
