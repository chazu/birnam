# Generated temporaries reported by their code Unit

Baseline `adbe9b05`, 2026-10-31. This changes automatic reference keeping in
TCC-generated code. Public Roots operations, native-provider compatibility and
Birnam's message semantics remain separate from that implementation choice.

## Required behavior and operation sequence

Generated locals, captures and results must remain valid through GC, callbacks,
errors and suspended execution. Returning a value must not expose it to collection
before its caller can keep it. Compiling a Method must not manufacture private
metadata behaviors or advance binding epochs.

Previously each generated call kept its intermediate values using public Roots
tokens. Its native buffer held tokens, while the registry held their Oops and a
token-position hash index:

```text
message result -> Roots retain: -> append token to call buffer
call exit -> releaseTokensProtecting -> current release: for each token
```

The TCC path now holds Oops directly in the call's native buffer:

```text
message result -> append reference to call buffer
GC -> trace the live code Unit -> report its active buffers through visit:
call exit -> unlink buffer -> free native storage -> return
```

Each compiled Unit registers one synchronous native reference entry from its own
declarations. TCC delegates reporting of the Unit's native storage to that entry;
it does not interpret the C buffer layout. The existing native-entry depth guard
keeps the executable mapping resident through callbacks. Registration updates the
existing Unit record without a new behavior, registry, Id API or dictionary edit.

Active native calls form a private linked list within the Unit. Reentrant calls
have separate buffers. A call saves its rooting mode when it first keeps a value,
so its cleanup continues to use the same buffer representation. Native freeing and
unlinking make no Birnam calls, leaving no collection callback in the return handoff.
Resumable steps continue to retain their durable values in Execution's frames.

The standalone native-provider initializer retains the token path. This is an
existing supported integration mode whose supplied owner need not be a TCC Unit.
It still validates Root acquisitions and performs protected, ordered cleanup.
Existing generated artifacts keep their old implementation until recompiled.

## Behavioral distinction and checks

Implicit Root-token acquisition is compiler bookkeeping, not a Birnam memory rule.
The TCC backend therefore no longer routes its intermediates through overridden
`Roots retain:` or `release:` methods. Authored sends to Roots remain ordinary live
sends. The root-lifetime test now checks explicit Root-call failures on the TCC path
and preserves the original failed/zero/absent-token cases on an independently
compiled native-provider path; it does not silently drop that compatibility check.

The same test keeps fresh values and captures alive across precise GC, then verifies
that temporary values are reclaimed after return. Disabling the Unit reporter makes
that GC test fail. A separate Unit test checks registration readiness, indexes,
rejection of continuation entries, unchanged binding epochs, trace-error propagation,
and collection after the reported reference is cleared.

Native implementation helpers perform the reporting algorithm for their own Unit.
Other objects are still reached through messages. No language ownership, borrowing,
constructor-origin restriction or payload catalog is introduced. The implementation
adds a callback slot and reporting code while removing per-value registry work from
the normal generated path; this is not a Rust source-line reduction or a completed
Birnam migration.

## Investigation and measurements

An earlier sorted token-index experiment was rejected. It improved reverse-order
microbenchmarks but left addition essentially unchanged (39.85 versus 39.86 µs).
Randomized release of 65,536 roots rose from about 47 to 113 ns per token. Its
[receipt](rejected-index.json) is evidence of rejection, not a suggested change.
The production root registry was restored unchanged.

The initial matched generated-root experiment rebuilt Boolean and Integer modules
with identical source and flags. Addition fell from 39.55 to 30.65 µs, with the same
49 managed allocations. These are preliminary partial-prefix measurements.
The final comparison rebuilt both complete prefixes from source, then used
baseline/candidate/candidate/baseline order with builds and tests stopped. Arithmetic
uses five samples of 1000 calls per process and GC mode; inventory uses seven
samples per fixture after two warmups, in resumable mode.

| Automatic GC enabled | Baseline | New generated roots | Reduction |
| --- | ---: | ---: | ---: |
| Integer addition | 39.86 µs | 29.82 µs | 25.2% |
| Signed addition/overflow helper | 10.06 µs | 7.73 µs | 23.1% |
| Exact Error check | 8.26 µs | 5.66 µs | 31.5% |
| Nil-returning Block | 0.85 µs | 0.71 µs | 16.9% |
| Dense authored inventory | 356.11 ms | 336.92 ms | 5.4% |
| Real-module authored inventory | 830.31 ms | 790.20 ms | 4.8% |

Addition still allocates 49 managed objects per call. The inventory is still far
slower than complete native metadata analysis (about 8 and 24 ms here), and its
production cutover remains unqualified. These workload results are not an
application-wide speedup or completion of runtime simplification.

## Validation and delivery scope

The workspace/all-targets/all-features check and 323 library tests pass. Twenty
integration targets pass against the fresh source prefix, including method-construction
purity, explicit Root failures, GC and replacement, Execution/suspension, native
context/code lifetime, candidate retirement, image roundtrip and resaves, image slots,
refusal and snapshot mutation. The independent native-provider Root test also passes
with system-compiled emitted C, retaining the invalid-token checks on that path.
The new Unit reference test covers trace errors and subsequent recovery/collection.

The fresh build provides both bootstrap and tool images. CLI version/help checks
passed; they are not an interactive application benchmark. The installer receipt
labels the builds `development-cache`; their native caches and prefixes were fresh.
This is not full release qualification or a user-prefix installation.

[result.json](result.json) records build receipts, source/library hashes, final
commands, timings and checks. The existing arithmetic and inventory probes were
reused; raw runs and the measurement orchestrator remain in the ignored measurement
directory.

Both variants have also been built into isolated prefixes from source using fresh
native caches. The candidate retains the existing cold-build Lang seed and does not
add another seed. Build artifacts, logs and probes remain under
`.cargo-target/measurements/generated-roots-20261031`; the user's installed prefix
has not been changed.
