# Retained upper integration

Design 90 batch 1 has moved native execution into `birnam-execution`. This crate
is not loaded by the production prefix and does not currently build against the
retired Runtime/native API. Do not restore aggregate Runtime construction to make
its old callers compile.

The duplicate language Method, Machine driver, scheduling, task-control and
flat/graph recovery wrappers have been deleted. Their current implementations and
native production acceptance live in [Execution](../birnam-execution/README.md).

The remaining integration work is intentionally retained for its actual producers:

- `runtime/method.rs` keeps the old TCC function-address owner registration and
  managed-turn bridge. Batch 2 converts generated installation to owned Methods
  and owner messages, then deletes this bridge and its native exports together.
- `runtime/native_tier.rs` and its children retain compiler construction, region
  kernels and allocation-elision. `runtime/numeric.rs` retains chain/loop compilation
  and reuse. Batch 2 must reconnect allocation-budget and deferred-identity owner
  protocols before claiming these optimizers; the native PC candidate interface
  alone is not evidence for them.
- Compiler/image registration and retirement still await converted producers.
  TCC owns mappings; Execution owns acquired/frame/generation retention.
- `runtime/task_control_tests.rs`, scheduling/control tests, and the retained
  `runtime/recovery/graph/tests.rs` preserve source/compiler acceptance cases.
  They are not passing prefix tests. Store-backed language task policy awaits Store;
  generated recovery and reclaimable compiled mappings are batch 2 acceptance.

Image, workers and full CLI startup remain later integration. Current acceptance
commands and their limits are in [validation](../../docs/validation.md).
