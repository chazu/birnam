# Retained upper integration

Design 90 batch 1 has moved native execution into `birnam-execution`. This crate
is not loaded by the production prefix and does not currently build against the
retired Runtime/native API. Do not restore aggregate Runtime construction to make
its old callers compile.

The duplicate language Method, Machine driver, scheduling, task-control and
flat/graph recovery wrappers have been deleted. Their current implementations and
native production acceptance live in [Execution](../birnam-execution/README.md).

The TCC function-address owner bridge and its Runtime exports are also deleted:
production TCC now constructs owned Methods and traces each Unit's references.

The remaining integration work is intentionally retained for its actual producers:

- `runtime/native_tier.rs` and its children retain compiler construction, region
  kernels and allocation-elision. `runtime/numeric.rs` retains chain/loop compilation
  and reuse. Reassess these under Design 90's
  [optimization contract](../../docs/design/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
  Do not port dead-allocation journals, synthetic charges, counted index ledgers
  or the deferred-identity service. They implement a retired equivalence policy.
  Reuse useful analyses and recovery for observable values; the native PC candidate
  interface alone is not evidence for these optimizers.
- Image registration and retirement still await converted producers.
  TCC owns mappings; Execution owns acquired/frame/generation retention.
- `runtime/task_control_tests.rs`, scheduling/control tests, and the retained
  `runtime/recovery/graph/tests.rs` preserve source/compiler acceptance cases.
  They are not passing prefix tests. Store-backed language task policy awaits Store;
  source compilation and reclaimable compiled mappings now pass in the compiler
  production test; generated optimizer recovery remains separate work.

Image, workers and full CLI startup remain later integration. Current acceptance
commands and their limits are in [validation](../../docs/validation.md).
