# Second runtime simplification round

All measurements are operation microbenchmarks, not startup or application
speedups. JSON records contain baseline revisions, workloads, samples and medians.
Results below are incremental and must not be multiplied into an overall speedup.

| Work | Representative measured result | Limit |
| --- | --- | --- |
| Portable-program offsets | 10,000 node reads: 21.05 ms → 30.77 µs (684×) | Validation ~16% slower; retained offset storage is two machine words per node and one per binding, plus small headers. |
| Remaining byte-buffer construction | C emission with a 256 KiB literal: 1.27× | Includes text generation; native compilation is outside timing. |
| Generated temporary roots | Immediate and discarded results no longer get tokens | Call throughput was essentially flat; the inline-token-buffer trial was removed (record 11). Small-call timings were slightly worse/noise-sensitive. |
| Vector bulk construction | 8,192-element reversal: 13.27 → 1.31 ms (10.1×) | Bulk results use fresh nodes; published vectors remain immutable. |
| Fixed Host selectors | 100 sends/scope: 45.43 → 9.19 µs (4.94×) | Selector Symbols are retained; receiver and method bindings are still read live. |
| Host scope storage reuse | 100 sends/scope: 9.19 → 7.45 µs (1.23×) | Empty buffers retain capacity, not references. |
| JSON unescaping | 4,096-byte escaped body: 3.34 → 2.59 s (1.29×) | Authored dispatch still dominates; this removes growing-string copies and per-character state vectors. |
| Compiled-Method metadata | Known Method: ~4× at the larger registry; captured alias: 51.5 µs → 89 ns (579×) | The small known-Method case regressed; hash iteration made its old scan unusually cheap. Unknown aliases retain comparison fallback. |
| Persistent ready queue | 2,048 jobs: 0.91 → 0.56 s (1.62×); 512 jobs: 1.10× | 128-job case ~8% slower. Enqueue setup excluded; nil reset handling retained. |
| NativeDoc ordering | 128 reverse-ordered names: 23.21 → 1.09 s (21.3×) | A worst-case sorting workload, not whole documentation generation. |

The compiler also hashes the source once per emission instead of once per nested
function. Native method declarations now derive literal indexes by name at
compile time; there is no new Id operation or module wire format.

## Architecture

- Portable bytes and validation rules are unchanged. Validation retains offsets
  it already computes, plus name/data offsets; dense indexes provide direct access.
- Native producers use the existing Bytes object and bulk messages through a
  stateless shared helper. Explicit Oop argument Sequences, layout-kind lists and
  bootstrap Names remain their own operations. No pointer-access or borrowing
  protocol was added.
- Generated code still protects live heap values across callbacks. It does not
  root immediate/null values or unused frame-write/publication results. The
  unsuccessful inline-storage experiment is preserved as measurement history.
- Vector slice/reversal/sort results are assembled in groups of fresh tree nodes.
  Concatenation copies the published tail once and fills new tails before linking
  them. Current allocation messages and collection remain supported.
- Provider-local weak Method indexes also record ordinary captures at creation.
  They do not keep aliases alive, expose Method internals, or change ordinary Id
  dispatch. Unregistered aliases still use current executable comparisons. The
  separate explicit native-signature adapter path is not a universal metadata API.
- Queue is a persistent collection implemented alongside Vector. Its three slots
  are Oops and its list nodes are ordinary Sequences, accessed through messages.
  Scheduling stays in Birnam. The initial authored-list experiment was correct
  but slower and was discarded. See the [runtime contract](../../contracts/runtime.md#executor-ready-queue).

Id and Collect source implementations were not changed by this round.

## Reproduction

`round2_costs` measures indexed portable access and validation. `round2_runtime`
has `emit`, `vector`, and `metadata` modes and uses an explicitly built native
composition. `invariant_costs calls` and `invariant_costs roots` supply the generated
call and Host measurements. All are examples in `rust/birnam-siphon`.

`round2_language` requires a complete installed prefix via
`BIRNAM_TEST_MODULE_DIRECTORY=<prefix>/lib`. It supports `json`, `json-test`,
`queue`, `queue-test`, and `sort`. `--current` loads this checkout's scheduler;
`--merge` selects the merge-sort comparison. Queue measurements can use `BIRNAM_QUEUE_SOURCE` to load the pre-queue scheduler
recorded in the JSON and the corrected scheduler on the same native runtime. JSON runs embed the decoder source,
so rebuild the example for the selected revision. NativeDoc's consumer change
uses the existing `mergeSortBy:` operation.

Setup, compilation, explicit collection and result checking are outside timing
unless the corresponding JSON scope says otherwise. Five or seven samples are
reported; small differences are not confidence intervals or release thresholds.

## Checks

Focused tests cover canonical/corrupt portable formats, code-construction purity,
large compiled literals with oversized Sequences forbidden, current byte providers,
collecting constructors, Vector tree boundaries and old snapshots, selector reuse
with live method/Namespace replacement, closed-scope reclamation, direct Method
lookup with comparisons forbidden, and weak captured-alias cleanup. Queue checks
cover FIFO order, reentrant enqueue, immutable snapshots, ordinary field views,
current node messages, collecting allocators and consumed-job reclamation.

The authored JSON suite passed all 107 assertions. The updated scheduler passed
all 177 task-control checks, including quantum 1/128, cancellation, cleanup, native
barriers and interleaving. Workspace all-target/all-feature compilation and the
focused native tests passed. These checks are not the entire release matrix.

The installed-image check exposed a nil-reset regression that source reload had
masked: the executor must not depend on a `nil asQueue` extension. The setter now
handles nil explicitly. `installed_scheduler` checks reset, initialization, wait
and resume against an installed image without reloading scheduler source; it is
included in the native-message test script. The new test failed on the affected
installation at the reset assertion. The queue measurement was repeated with the
corrected setter and the same native runtime for both scheduler versions.
