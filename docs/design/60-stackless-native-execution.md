# 60 — Stackless Native Execution

> **Status:** Architecture assessment, 2026-07-22. This document records the
> feasibility, migration boundary, and recommended first proof for stackless
> native Birnam execution. It is not an implementation commitment and does not
> place continuations on the active product roadmap.
>
> Builds on [32 — Refound on libid](32-refound-on-libid.md),
> [40 — The Image](40-the-image.md),
> [45 — Compiled Units](45-compiled-units.md), and
> [48 — Concurrency](48-concurrency-reactors-and-isolates.md).

## 1. Question and conclusion

Birnam currently compiles language methods and blocks to ordinary C-ABI
functions. A message send synchronously resolves a function pointer, calls it,
and receives an `Oop`. Recursive Birnam calls therefore consume the host
thread's C/Zig stack.

Moving Birnam to stackless native execution is feasible without replacing the
object model, vtables, Store, compiled-unit authority, or native-code pipeline.
It is nevertheless a substantial compiler/runtime project because the
synchronous method contract is shared by libid, the public `IdApi`, generated C,
native modules, higher-order primitives, FFI callbacks, GC roots, and compiled
artifacts.

The recommended target is:

- an explicit per-fiber frame stack owned by a native `Machine`;
- native-compiled resumable method state machines;
- a trampoline that performs call, return, tail-call, suspend, and restore;
- synchronous leaf primitives retained behind an adapter;
- delimited, initially one-shot continuations;
- explicit continuation barriers at unmigrated native and FFI boundaries;
- process-local continuations that are not claimed as durable image state.

This is a replacement of Birnam's **execution ABI and code-generation shape**,
not a rewrite of its identity or relational substrate. A useful first version
is plausibly a two-to-three-month focused project. Unrestricted multi-shot
continuations across all native control primitives, callbacks, live code
replacement, and debugging are a larger four-to-nine-month runtime program.

## 2. Current execution contract

The universal native method type is:

```zig
pub const Method = *const fn (
    self: Oop,
    args: [*]const Oop,
    n: usize,
) callconv(.c) Oop;
```

`IdApi.method` installs that function pointer and `IdApi.send` invokes a send to
completion. `id.send` binds the selector and immediately calls the resolved
method. Method dictionary values are raw function pointers rather than
descriptors carrying an execution kind.

The C emitter follows the same contract:

- ordinary sends become `birnam_send(receiver, selector, args, n)`;
- nested Birnam sends become nested C calls;
- lexical locals and temporaries become C locals;
- block bodies become file-scope Method-ABI C functions;
- a block's captured values live in its heap payload;
- argument arrays are C compound literals whose lifetime spans only the
  synchronous send.

The runtime has no Birnam-owned activation record, operand stack, continuation,
handler stack, scheduler frame, or guaranteed tail-call elimination. The
collector consequently treats the native stack and saved registers as roots.

Some iteration is deliberately stack-flat. `whileTrue:` and `whileFalse:` run a
Zig `while` loop in one host frame and synchronously send `value` to their
condition and body blocks. This avoids growth per iteration, but the loop's
control state remains on the Zig stack and cannot survive suspension.

## 3. What “stackless” should mean

Stackless execution need not mean that the implementation has no stack. It
means Birnam call depth does not consume the native C/Zig stack. The runtime may
and should use an explicit frame vector, segmented stack, or chunked arena.

The native stack remains bounded to the trampoline, dispatch lookup, one method
step, and any atomic native primitive it invokes. Each compiled Birnam method
returns to the trampoline at every send or suspension point, so those host
frames do not accumulate with language call depth.

Trampolining alone and continuations are distinct milestones:

1. A tail-call trampoline can make selected tail recursion stack-safe without
   representing every suspended caller.
2. Arbitrary suspension requires every live local and return point at a
   safepoint to reside in an explicit frame.
3. A one-shot continuation detaches or transfers ownership of an explicit frame
   segment.
4. A multi-shot continuation copies or persistently shares that segment and
   defines how repeated restoration interacts with mutable state and resources.

The compiler work required by step 2 is the decisive boundary. Once arbitrary
Birnam activation state is explicit, tail calls, coroutines, generators,
delimited control, non-local returns, resumable conditions, and stack
inspection become different policies over the same machinery.

## 4. Recommended execution model

### 4.1 Machine and frames

A process may own one or more logical machines or fibers. Each machine carries
all state that varies with an execution:

```text
Machine
  frame stack
  current value or action
  handler and delimiter stack
  suspension state
  active compiled-generation pins

Frame
  resumable implementation identity
  program counter
  receiver
  arguments
  locals and temporaries
  return destination
  lexical-method identity where required
```

Frames should use chunked or arena-backed storage rather than a separately GC'd
allocation per send. Oop slots need precise tracing; program counters, slot
indices, and native metadata must be marked raw. The existing layout and root
provider APIs are sufficient foundations for this division.

### 4.2 Native resumable methods

The compiler should lower a method to basic blocks with explicit live slots. A
native method step resumes from its saved program counter, runs until its next
safepoint, records the next program counter, requests an action, and returns to
the trampoline:

```c
void example_step(Machine *machine, Frame *frame) {
    switch (frame->pc) {
    case 0:
        /* Compute receiver and arguments into frame slots. */
        frame->pc = 1;
        birnam_machine_call(machine, receiver, selector, args, n, result_slot);
        return;
    case 1:
        /* Consume result_slot and continue. */
        birnam_machine_return(machine, result);
        return;
    }
}
```

This remains native-compiled Birnam. Live mode may compile the step function
with TCC and shipped artifacts may compile the same generated C with the AOT
toolchain. An interpreter for the same intermediate representation may be useful
as a differential oracle, but it need not become the primary execution form.

### 4.3 Trampoline actions

The driver needs a small closed action set:

- `call` — bind a selector and push a resumable frame or invoke an atomic
  synchronous primitive;
- `tailCall` — replace the current resumable frame;
- `return` — pop a frame and place the answer in its caller's result slot;
- `continue` — resume the current frame;
- `suspend` — return a suspension or operation to the host;
- `capture` — detach or copy a delimited frame segment;
- `restore` — install a captured segment and its supplied answer;
- `abort` — unwind explicitly to a handler or delimiter.

Errors currently propagate as ordinary values without native unwinding. That is
compatible with the new driver and need not change during the first slice. A
later condition/restart system can add handler-stack actions without depending
on `longjmp` or C++ exceptions.

## 5. Compatibility strategy

Replacing every native method at once is unnecessary and excessively risky.
Introduce a second method kind alongside the current ABI:

```zig
pub const SyncMethod = *const fn (Oop, [*]const Oop, usize)
    callconv(.c) Oop;
pub const StepMethod = *const fn (*Machine, *Frame)
    callconv(.c) void;
```

The current `IdApi.method` and `IdApi.send` remain available. New append-only
API entries register resumable implementations and start or resume a machine.
A compatibility `send` creates or enters a machine and drives it to completion.

The vtable dictionary may continue to store the current callable address during
the migration if an implementation registry maps that address to its resumable
entry, frame layout, compiled unit, and code generation. Newly compiled Birnam
methods register as resumable. Existing primitive methods default to
synchronous and atomic.

This preserves older native modules and keeps leaf primitives cheap. It does not
make every old method continuation-safe: a synchronous method that calls back
into Birnam while retaining state on its Zig/C frame is a barrier. Capturing a
continuation through that frame must either be rejected or delayed until the
method is migrated.

## 6. Native-method classes

Native methods fall into three migration classes.

### 6.1 Atomic leaves

Arithmetic, scalar conversion, direct object allocation, many Store operations,
and other methods that do not invoke Birnam callbacks may remain synchronous.
They run to completion during one trampoline action and return an `Oop`.

### 6.2 Higher-order control primitives

Methods such as `ifTrue:`, `whileTrue:`, `to:do:`, collection iteration, and
other operations that invoke blocks while retaining native control state must
become resumable frames before continuations can cross them. Suitable options
are:

- rewrite the policy in compiled Birnam;
- implement a small native step method with explicit index/phase slots;
- temporarily designate the method as a continuation barrier.

This is the main native-module migration surface. The raw count of native
methods overstates it: leaf primitives can remain unchanged, while the smaller
set of callback-driving methods must be audited deliberately.

### 6.3 Foreign calls and callbacks

An ordinary C caller expects an Alien callback to return its declared C value
before the callback frame exits. The callback adapter may enter a nested machine
and drive it to completion, but it cannot yield through an unaware C caller.

The first contract should therefore be:

- synchronous FFI calls are continuation barriers while foreign code is active;
- a C-to-Birnam callback runs a nested delimited execution to completion;
- capture cannot include the foreign caller's native frames;
- truly asynchronous libraries require explicit completion callbacks, event
  registration, or operation objects rather than captured C stacks.

Copying or switching native stacks would weaken these guarantees and is not the
recommended route. Native frames may contain interior pointers, foreign cleanup
state, ABI-specific register state, and resources whose duplication is invalid.

## 7. Live evolution and code lifetime

A suspended frame cannot safely retain only a raw instruction address. Live
method replacement may make that address stale, and a disposable TCC unit may
otherwise release its code.

Each resumable implementation therefore needs stable metadata:

- method or implementation identity;
- compiled-unit and dispatch-generation identity;
- frame-layout and program-counter version;
- native step address for the pinned generation;
- source or revision identity for inspection and diagnostics.

A live continuation should pin the code generation it contains until it is
consumed or collected. New sends after restoration may late-bind through the
current dispatch generation, while already suspended activations continue in
the generation whose frame layout and program counter they contain. This follows
the ordinary rule that a running activation completes in the code it entered,
but makes the lifetime explicit.

An alternative is to migrate suspended frames across recompilation. That is a
separate, much harder feature requiring compiler-emitted frame maps and user or
system migration policy. It must not be implied by the first implementation.

## 8. GC, inspection, and durability

Explicit language frames are easier to trace and inspect precisely than
conservative native frames. The migration may eventually reduce the language's
dependence on conservative native-stack scanning, but synchronous primitives
will continue to hold temporary Oops on the C/Zig stack. The first slice should
therefore retain the existing native stack anchor and additionally register
machine frames as roots.

Frame inspection should expose safe descriptors rather than native function
pointers. An on-demand view may include method identity, selector, receiver,
source span, program counter, and redacted/local values. Reifying every running
frame as durable Store facts on every call would add unnecessary mutation and
should not be the execution representation.

Continuations should initially be explicitly non-durable. Current image and
isolate contracts distinguish durable recovery descriptions from in-flight
turns, pending replies, channels, and callbacks. A continuation held in a
durable Store position should therefore make image validation fail with a clear
kinded error, as other process-local handles do.

Durable continuations would require a portable method identity, frame layout,
program counter, values, code revision, resource policy, and restore contract.
That capability should be designed separately if a concrete application needs
it.

## 9. Compiler work

The present emitter recursively writes C expressions. Stackless code generation
requires an explicit intermediate form, even if that form is short-lived and
never serialized:

1. Normalize expressions into ordered operations with named temporaries.
2. Split each method and block at sends, capture points, handler operations, and
   other safepoints.
3. Compute liveness and assign receiver, arguments, locals, temporaries, and
   results to frame slots.
4. Emit stable basic-block/program-counter identifiers.
5. Mark tail positions so the driver can replace rather than push a frame.
6. Emit a frame layout and source map beside the native step function.
7. Register the implementation metadata when a TCC or AOT unit is mapped.

Every send must copy or otherwise retain its receiver and arguments in explicit
storage before the current method step returns. The current C compound-literal
argument arrays cannot cross that boundary.

The same IR can support a small interpreter for differential tests and native C
emission for production. Keeping execution semantics in the IR rather than in
C-expression nesting is also the clean seam for later handler, debugger, and
scheduler work.

## 10. Difficulty and estimates

These are order-of-magnitude estimates for one developer already familiar with
the codebase, assuming focused work and the current test infrastructure:

| Target | Difficulty | Rough effort | Delivers |
|---|---:|---:|---|
| Tail-call trampoline for selected compiled methods | Medium | 2–4 weeks | Stack-safe tail and mutual recursion; no arbitrary suspension |
| Explicit native frames plus delimited one-shot continuations | Hard | 2–3 months | Stack-safe calls, suspend/resume, coroutines, generators, initial conditions/restarts |
| Broad multi-shot continuations | Very hard | 4–9 months | Repeated restoration, migrated control primitives, debugger/reification, robust live-code pinning, explicit FFI policy |

The first row is not throwaway work only if it introduces the parallel method
kind and frame/action protocol used by the second. A special-case self-tail-call
rewrite would prove less and create migration debt.

## 11. Incremental implementation path

### S0 — Semantics and ABI record

Decide and document:

- delimited versus undelimited capture;
- one-shot versus multi-shot first delivery;
- where capture is legal;
- foreign/native barrier behavior;
- continuation equality and consumption;
- code-generation pinning;
- image rejection and inspection surfaces.

Append, rather than replace, the new `IdApi` entries. Bump compiled artifact and
candidate ABI versions where a unit begins emitting resumable implementations.

### S1 — Parallel machine substrate

Add `Machine`, explicit frames, actions, a driver loop, resumable-method
registration, and root tracing. Retain all current methods as synchronous.
Prove that entering and completing a machine produces the same ordinary value
as `send`.

### S2 — Minimal resumable compiler path

Lower a deliberately small subset—literals, locals, blocks, and ordinary sends—
to the new IR and native state-machine C. Keep the existing emitter available as
a differential reference during the spike.

The decisive test is mutually recursive Birnam methods running millions of
sends while the native stack remains within a fixed bound.

### S3 — Suspension and one-shot restoration

Add an explicit yield/suspend operation, represent a suspended machine as a
process-local object, and resume it with a supplied value. Then add delimited
one-shot capture by detaching a frame segment.

This slice should test dead/consumed continuations, nested delimiters, error
propagation, GC reachability, code pinning, and image rejection.

### S4 — Control primitives

Migrate Boolean branch methods, `whileTrue:`/`whileFalse:`, integer iteration,
and the collection higher-order methods required by real programs. Prefer
Birnam-authored policy where it does not impose unreasonable overhead; retain
small native step primitives where measurement justifies them.

### S5 — Conditions, scheduling, and inspection

Build handler/restart policy, cooperative task scheduling, cancellation, and
safe stack inspection over the shared frame machinery. Keep each as a policy
layer rather than expanding the frozen identity floor.

### S6 — Optional multi-shot and durability work

Only after applications exercise one-shot delimited continuations should Birnam
select copying, persistent chunks, or copy-on-write frame segments for multi-shot
use. Durable continuations remain a separate decision.

## 12. First proof and acceptance gates

The recommended first proof is intentionally narrow:

1. Add the parallel step ABI without removing the synchronous ABI.
2. Compile two mutually recursive methods as native resumable state machines.
3. Run at least one million sends with bounded native-stack movement.
4. Suspend midway, return control to Zig, and resume to the correct answer.
5. Confirm explicit frames remain live across a forced collection.
6. Confirm an old synchronous leaf primitive works through the adapter.
7. Confirm capture across a declared native barrier fails deterministically.
8. Compare send and frame overhead against the current benchmark methodology.

Do not begin by converting the standard library, implementing unrestricted
`call/cc`, or preserving continuations in images. The proof succeeds when it
validates the execution seam and makes the remaining migration measurable.

## 13. Open decisions

1. Should the first public control surface expose generators/coroutines,
   delimited `shift`/`reset`, or lower-level continuation objects?
2. Are continuations initially one-shot by language law or merely by the first
   representation?
3. Does an unmigrated callback-driving native method reject capture, or does the
   compiler/runtime prevent entering capture-capable code beneath it?
4. Should a restored activation late-bind all subsequent sends against the
   current dispatch generation while its own native code remains pinned to the
   old generation?
5. Is the explicit frame IR transient compiler data, reflected descriptors, or
   eventually a durable executable representation?
6. What measured overhead is acceptable for a resumable send in live TCC and
   shipped AOT modes?
7. Does the reactor proposed by Design 48 consume `suspend` actions directly, or
   should execution scheduling remain a separate design until the machine proof
   exists?

## 14. Historical design prior

The archived pre-refounding roadmap proposed a bytecode VM with explicit
per-thread frames, tail-called opcode handlers, non-local-return unwinding, and
on-demand frame reification. That implementation belongs to the retired kernel
and should not be revived wholesale. Its central separation remains useful:
execution frames are fast native materialization; reflection reifies them only
on demand; durable facts are not mutated on every call.

The current architecture changes the backend choice. Rather than making bytecode
interpretation the required execution form, Birnam can compile the same explicit
frame semantics to resumable native C. The historical plan is therefore a design
prior, not current-state evidence or a ready implementation.
