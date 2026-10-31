# Integer addition allocation trace

Investigation at `8b6ea74b`, 2026-10-31. Production code is unchanged.

One ordinary addition of existing Integers 4 and 3 allocates 40 managed objects.
Two independent diagnostic runs account for all of them: allocator backtraces
identify construction sites, and Id's active call selectors identify the authored
operations. Both return 7. Automatic GC was disabled for these traces; tracing
overhead is not a performance measurement.

| Work | Objects |
| --- | ---: |
| Four Boolean selections and one Boolean `&` adapter | 25 |
| Four explicit Block entries | 8 |
| Two operand Word conversions | 2 |
| Four Word arithmetic/bit operations | 4 |
| Integer result | 1 |

Each Boolean adapter allocates a private synchronous answer, an argument Sequence,
another private answer for `invokeOn:arguments:`, deferred body-entry work, and
deferred reply work. Each Block entry allocates a private answer and body-entry
work. In total, 28 control Methods and five argument Sequences account for 33 of
the 40 objects. These counts describe this implementation and input, not required
language allocation behavior.

The adapter sequence is:

```text
enter selected Boolean adapter with private answer
  -> normalize arguments
  -> build argument Sequence
  -> send invokeOn:arguments: to captured body Method
     -> open another private answer -> prepare body entry -> run body
  -> convert result -> prepare reply -> record outer answer
```

The four selections choose handlers; they do not invoke those handlers. The
subsequent explicit Block messages account for the separate eight allocations.
No numeric payload interpretation is involved in identifying these costs.

## Diagnostic replacement, not an accepted change

A process-local experiment installs ordinary authored versions of Boolean `&`
and `selectIfTrue:ifFalse:otherwise:` with the same bodies and lexical captures,
omitting their native-signature adapters. It uses the existing arithmetic probe
and unchanged release libraries in baseline/direct/direct/baseline order.
Selectors, inputs, compilation and installation are outside timing. Each median
has ten samples of 1000 calls with automatic collection enabled.

| Operation | Baseline | Diagnostic | Allocations |
| --- | ---: | ---: | ---: |
| Integer addition | 28.17 µs | 20.55 µs | 40 → 15 |
| Exact Error check | 5.40 µs | 4.03 µs | 5 → 0 |
| Signed addition/overflow helper | 7.40 µs | 5.70 µs | 12 → 7 |

Addition takes about 27% less time in this experiment. With automatic collection
disabled it measures 28.47 versus 20.60 µs. This establishes a worthwhile target,
not an achievable semantics-preserving speedup or an application result.

**Do not apply the Boolean replacement as a production fix.** An explicit behavior
probe finds that missing and extra arguments to `&` fail with the adapter but
return normally with the plain compiled method. Both reject a raw null argument;
that alone does not prove equivalent effect ordering. Callback, captured-invocation,
reflection and suspension behavior are not qualified by this experiment.

An initial exploratory replacement accidentally used global lookups instead of
the original lexical captures and regressed addition to about 70 µs despite the
same allocation reduction. The final comparison above restores the captures.
This also demonstrates why allocation counts alone are insufficient evidence.

## Next bounded task

Investigate the generic ordinary-result adapter path used by these Boolean
operations. Preserve arity and argument validation before body effects, result
conversion, failure observation, current explicit-invocation behavior, live
replacement, GC and callback/suspension rules. State the proposed operation
sequence before implementing it, and measure full addition after focused negative
tests. Do not infer permission to bypass a replaceable operation from these counts.

Keep Boolean and Integer policy in Birnam. Do not introduce selector-specific
arithmetic shortcuts or a new Id payload catalog. A generic compiler optimization
must justify its complexity and recovery behavior; fewer allocations do not by
themselves justify another optimization layer.

The existing ordinary-return proposal's independent-drive policy question remains
unanswered. This investigation does not resolve it. There is no authorization here
to weaken native contracts, remove checks, or change a failure into a normal result.

## Evidence and replay material

[result.json](result.json) contains the full allocation paths, categorized counts,
timing summaries, negative-probe results and hashes. Raw logs and probe projects
are under `.cargo-target/measurements/arithmetic-allocation-trace-20261031`.

Diagnostic patches are based on the recorded commit and should only be applied
in an isolated checkout:

- [one-addition.patch](one-addition.patch) changes the existing arithmetic probe
  to trace one send and verify its answer.
- [trace.patch](trace.patch) instruments allocation backtraces;
  [path-trace.patch](path-trace.patch) separately records active selector paths.
  Build Id with `CARGO_PROFILE_RELEASE_DEBUG=1` for the backtrace variant, put
  that library in a copied source-built prefix, and run the one-addition probe
  against that prefix. Neither variant is used for timing.
- [boolean-experiment.patch](boolean-experiment.patch) modifies the existing
  arithmetic probe. Build it using the dependency manifest from that probe's
  runner. Run against the unchanged baseline prefix, with and without
  `BIRNAM_DIRECT_BOOLEAN_EXPERIMENT=1`; reverse case order on runs two and four
  with `ARITHMETIC_REVERSE=1`. `BIRNAM_BOOLEAN_NULL_CHECK=1` runs the raw-null and
  wrong-arity probes instead of timings. These replacements exist only in the
  test process.

All production instrumentation was removed and the shared release Id artifact
rebuilt from unchanged source. No installation, push, migration or release
qualification is claimed.
