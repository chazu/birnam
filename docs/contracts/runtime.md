# Current runtime behavior

This reference collects continuing behavior from the completed runtime refactors.
It is subordinate to [INVARIANTS.md](../../INVARIANTS.md). It does not prescribe
Rust ownership rules, private tables or particular module boundaries.

## Methods and definitions

Compiling a body produces code. Loading native code maps it into the process.
Creating a Method associates an implementation with the data it needs. Installing
that Method under a selector changes an object's method dictionary. Those are
distinct actions; the first three do not implicitly perform the fourth.

Default Method construction checks the required parameters, captures and code
description before returning a usable Method. Repeated use of the same admitted
code does not require another shape query; conflicting associations fail.
Callbacks must not expose inconsistent construction state. A native callable
does not require a portable CodeObject solely to be invoked.

Code implementations supply their reflection and persistence behavior through
messages. Do not recreate the removed universal Method-metadata database or add
bootstrap Method context getters to recover it. GC inspection is not a substitute
for a missing reflection operation.

## Editing and explicit undo

Ordinary dictionary edits require neither Store facts nor a generation
certification step. Id has no universal `methodChangesTo:` subscription. Store
retains explicit invalidation for the edits it describes; ordinary lookup
invalidation still follows actual dictionary changes.

Explicit undo checks the current binding against the recorded values. An already
restored value needs no write. A value equal to the expected edited value may be
restored; another current value is a conflict and remains untouched. An intervening
edit that returns to the expected value does not by itself prevent undo.

Do not reconstruct global mutation history or an Id atomic-undo primitive for
that operation. Undo may stop after partial progress. It does not silently replay
failed work or promise to undo arbitrary callback effects.

## Execution and optimization

New sends observe current lookup, including custom lookup and delegation. A
captured or already entered Method remains usable after dictionary replacement.
Running and suspended computations keep the code and objects they need. Removing
a binding is not sufficient reason to release that code.

Optimized execution preserves results, effects, errors, evaluation order,
observable identities, aliases and shared mutable state. Guards must establish
the relevant behavior, not merely recognize a selector's spelling. Changed
assumptions require recovery or ordinary execution. Supported inspection and
deoptimization must reconstruct the logical values and frames required there,
without replaying completed effects.

Optimization need not reproduce baseline allocation counts, allocated bytes,
reclaimed counts, or the timing of automatic collections. Do not allocate dead
objects, invent synthetic charges or keep dead payload journals to simulate an
unoptimized run. Explicit collection and actual resource cleanup still occur.
Unknown or replaced constructors and finalizers may have observable effects;
optimizations must account for them or use ordinary execution.

Default `Machine prepareCall:selector:` performs a complete frame transfer.
Its internal slot copies and action construction do not send `oopAt:`,
`oopAt:put:`, `callSite:` or `tailCallSite:`. Replacing the preparation operation
still changes compiled calls. Explicit slot and action messages remain replaceable
when sent, including from an authored preparation implementation. Ordered copies,
argument identity, inspection and scheduling behavior are preserved.

Continuation values can be returned and stored as data. Suspension and answer
delivery use explicit control operations. Repeated or invalid use of a consumed
continuation must not replay completed work. See the
[Execution implementation](../../rust/birnam-execution/README.md) and
[concurrency guide](../guide/07-concurrency.md) for the supported operations.

## Executor ready queue

The default ready queue is a persistent `Queue` value. `add:` returns a new
queue; `takeFirst` returns a Vector containing the oldest value and the remaining
queue. Empty queues return `#[nil self]`. `size` is constant-time and `asVector`
produces an ordered snapshot. Successive dequeues use two immutable lists, with
one reversal when the front list is exhausted; old queue snapshots stay valid.

The data operations are implemented with the other persistent collections.
Queue's three slots contain only Oops. Its Sequence list nodes are accessed by
messages; Id does not inspect Queue data. Scheduling policy remains authored.
`TaskExecutor queue:` converts ordered collections through `asQueue`. Explicit
`field: #queue` views remain ordinary slot views; a supplied Vector still works
through the generic `takeFirst` operation. Iteration, comparison and slicing
operate on ordered snapshots. Consumed jobs are not retained by the remaining
queue unless another snapshot or program reference still needs them.

## Compiled artifacts and images

An image's compatible compiled code is executable content, not a disposable cache
to be silently replaced by reparsing source. Rebuilding from recorded source is
explicit. Inspecting an artifact must not execute its code; loading a module may
execute its initializer and install definitions.

Live image loading may leave partial changes when an operation fails. It does not
promise automatic rollback or arbitrary-effect reversal. Explicit candidate undo
remains a separate operation. Loading invalidates the relevant old editing and
compilation state; it must not restore obsolete authority after failure.

Store and Relational are optional. An image that uses an optional provider records
that requirement and restores it before restoring dependent objects. Missing
providers fail explicitly. Core images need not require those facilities.

Concrete artifact formats remain in their versioned references and codecs. A
format document does not dictate the language's implementation architecture.
See [images](../guide/04-images.md) and
[compiled artifacts](../guide/09-shipping-artifacts.md).

## Native code

Keep Birnam references and executable code valid across native calls, callbacks
and reentry. Invalid sizes, references or executable metadata must not cause
unchecked memory access in Birnam's implementation.

Calling arbitrary native code in the process is not a security sandbox. A wrapper
must meet the actual library's initialization, threading, pointer and cleanup
requirements. Address concrete faults without adding universal locks, registries
or a new runtime architecture to compensate for hypothetical native misbehavior.
These are implementation responsibilities, not ownership or borrowing features
that Birnam programs must express.
