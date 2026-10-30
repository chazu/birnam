# Execution over Oop-only Id

Execution owns Blocks, Machines, frames, continuations, code admission and
optimizer/deoptimization state. All communication with other owners uses Id
messages with Oop receivers, selectors, arguments and results. Id does not inspect
Execution payloads or decide how a Machine is scheduled.

The [invariants](../../INVARIANTS.md) define the requirements. The
[current runtime reference](../../docs/contracts/runtime.md) describes Methods,
execution and optimization. Earlier migration designs are historical evidence.

## Methods and entry

Ordinary sends perform current method lookup and enter the selected Method
without another message on that Method. Explicit captured-Method invocation
remains available. Bootstrap owns the native Method representation; Execution
uses its declared entries and opaque contexts through the public message API.

Creating a Method checks its code, parameter/capture counts and descriptor
agreement before returning the Method. It does not install bindings, manufacture private
metadata vtables or advance binding epochs. Native and resumable code providers
own their reflection and persistence data. There is no universal Method-metadata
database or bootstrap Method context accessor.

A Method keeps its implementation alive independently of its dictionary bindings.
Running calls, active frames, suspended continuations and explicit leases retain
their required code and objects. Retirement waits for those holders and the end
of a managed turn. Removing a binding does not invalidate a captured Method.
There are no dictionary-binding reference counters or staged-binding undo APIs
in this engine. Actual executable lifetime is retained through traced owner Oops,
not an empty reference-counted token.

## Implementation map

| Source | Responsibility |
| --- | --- |
| `synchronous.rs`, `native_code.rs`, `oop_reflection.rs` | Public messages, synchronous code admission, Blocks, provider-owned reflection and tracing. |
| `resumable_runtime.rs` | Connect admitted code and Machine operations to Id continuation transfers; retain current code and values across calls. |
| `execution.rs`, `execution/boundary.rs`, `execution/scheduling.rs` | Validated descriptors, frame storage, owner/capability checks, lifetime counts, suspension and control boundaries. |
| `execution/recovery.rs`, `oop_recovery.rs`, `recovery_graph.rs` | Validate and publish recovery state without replaying completed effects. |
| `oop_native_tier.rs` | Owned optimization candidates, current guards and fallback. |
| `oop_call_preparation.rs` | Admit and execute generated call preparation through replaceable owner operations. |
| `control.rs`, `frame_view.rs` | Control operations and logical frame inspection. |

Codegen owns the live tail-entry transfer planner in its own crate. The unused
Execution tail-restart experiment and its duplicate cached plan are removed.
This does not remove generated tail calls, bounded scheduling opportunities,
optimizer candidates or deoptimization.

## Validation

`bash scripts/oop-id-slice-test.sh` builds the current native owners and exercises
compiler/Execution integration, custom lookup, direct entry, construction purity,
suspension, interleaving, native callbacks, GC, recovery and optimization guards.
`bash scripts/oop-bootstrap-test.sh` additionally checks cold compilation and
published startup. The workspace unit tests cover frame transitions, recovery
admission and retirement with every supported kind of holder.

Some integration tests are explicitly ignored by ordinary `cargo test`; their
owner-specific scripts assemble the required runtime and run them. A passing
unit suite alone is not full runtime qualification or a performance claim.
