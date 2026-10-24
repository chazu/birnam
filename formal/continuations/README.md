# Continuation-aware Id: formal models

The package now includes [metacircular lookup, bootstrap and native callback
boundaries](BOUNDARIES.md), in addition to the first model described below.

This is a proposed semantic boundary, not an implementation or a frozen ABI.
The user explicitly reopened additions to Id, preferring a small public contract
over an implementation privately shared by Id and Execution. Production sources
and their failing custom-lookup tail gate are unchanged.

## Contract under examination

Before: a send selects a Method and synchronously returns its answer. Execution
cannot safely recover the pending Call/TailCall meaning from arbitrary source
re-entry during lookup or native callbacks.

Proposed: a send carries an explicit answer destination, represented by an Oop.
Lookup may suspend while its owner retains the pending computation. On completion,
the selected Method is entered directly with the original receiver, arguments and
answer destination. Selection is not repeated when resuming entry. A tail call
reuses the existing destination; an ordinary call retains an additional return
continuation. There is no implicit lookup on the selected Method.

Id transports Oops and implements a documented transfer convention. Execution
owns continuation/frame representations and their reference reporting. Scheduler
policy owns ready/waiting computation references. The small native transfer loop
must return between execution pieces; no theorem here establishes that a future
C or Rust implementation actually does so.

The new required mechanism is the public continuation-aware calling/transfer
contract. No payload-type catalog, private Id/Execution pairing, shared layout,
or new implicit application protocol is proposed. The exact ABI and operation
names are still design work. The boundary extension models a concrete bootstrap
base case and its mutable lookup binding. The mathematical owner interface in
Lean is not a proposed native service table: it specifies the observable
obligations of an owner reached through the eventual public object protocol.

## What is checked

`tla/ContinuationSend.tla` models independent computations through lookup work,
selection, entry, body, answer and a following tail call. Lookup can park after
an effect or after selecting a Method. A body can also park. Wakeup resumes the
saved phase; it does not restart lookup. Binding replacement and collection may
interleave with any transition. The scheduler can choose any ready computation.

The checks require:

- selection observes the binding when the model's lookup reads it;
- the selected Method, rather than a later binding, is entered;
- lookup effects and entries are not replayed;
- answers retain their original destinations across interleaving and tail calls;
- tail chains do not add logical return frames;
- selected and entered code remains alive, including while parked;
- a computation has exactly one scheduler location while unfinished;
- consumed wakeups do not acquire ownership again;
- finite computations eventually complete under the stated fairness assumptions.

In this first model, lookup is represented by explicit phases and one binding
read. The boundary extension expands the recursive metacircular sends. The model
does not impose a transactional snapshot on arbitrary lookup: mutations before
its read are observed; mutations after its
selection do not rewrite that selected invocation. A replacement Method is held
by its publisher before installation. Omitting that lifetime initially produced
a real model counterexample; publication cannot create a reference to code that
has already been reclaimed.

`lean/Continuation.lean` supplies complementary, unbounded finite-trace proofs:

- a driver parameterized over an abstract owner state preserves the reference
  control observations when the owner implements the stated operations;
- two independently implemented frame layouts (nearest frame first versus last)
  satisfy that contract and produce identical observations for every operation
  sequence, including invalid operations on empty frames;
- arbitrarily long tail chains preserve the caller's frames and answer destination;
- returning after such a chain delivers to the original destination;
- consumed wakeup tokens remain rejected after arbitrary other consumption history;
- a continuation Oop can be returned as ordinary data without being mistaken for
  a transfer instruction.

`Oop` is an arbitrary Lean type. The generic driver has no operation to inspect
the owner's state. The proof concerns the modeled observations (entered Methods,
destinations, answers and invalid operations); it is not equivalence for every
possible reflection, allocation or timing observation in Birnam.

Lean's `Op`/`Event` constructors describe the formal transfer language; TLA+'s
phase strings describe Execution state. Neither is a proposed tag catalog for
ordinary Oop payloads. Distinguishing a transfer from a language answer must be
an explicit part of the eventual calling convention, never inferred from the
answer object's representation.

## Counterexamples and non-vacuity

The checker requires bad models to fail for their named reason: stale binding
selection, selection repeated at entry, replayed lookup effects, growing tail
frames, wrong answer destination, dropped code retention, duplicate wakeup,
synchronous lookup that cannot resume, and a scheduler without fairness.

Separate witness checks deliberately assert that lookup never parks, selected
code is never replaced while parked, a body never parks, or two computations
never wait together. Each assertion must produce a reachable counterexample.
These witness failures are successful coverage checks, not safety defects.

Lean is freshly compiled and every theorem's kernel dependencies are audited.
Only Lean's standard foundational axioms are permitted; no project axioms,
unfinished proofs or native evaluation proofs are accepted. Four changed
implementations must fail proof checking: tail push instead of replacement,
wrong destination in the alternative frame layout, reusable wakeup tokens, and
erased observable events.

## Bounds and assumptions

TLC explores two computations with two calls each, one computation with four
calls, and three computations with one call each. There are two Method identities
and one live binding replacement. Each computation can park once at each of three
phases per call. This is an exploration bound, not a proposed language restriction.
The Lean trace and tail-chain results have no such length bound.

Progress requires weak fairness separately for each computation's execution and
wakeup, finite method bodies, and eventual external completion of parked work.
Fairness does not make arbitrary I/O finish or an infinite computation terminate.
Removing fairness has a required counterexample. `CHECK_DEADLOCK FALSE` permits
completed behavior; the explicit temporal completion property is still checked.

Collection is an abstract code-retention check, not a complete heap collector.
Ready/waiting membership specifies scheduler ownership, not a proof that a native
root provider reports every reference. Oop address reuse, allocation failures,
full DNU/error semantics, cancellation/cleanup, continuation cloning,
thread-hosted domains and executable mapping retirement
are outside this first model. Native callback barriers and bootstrap are covered
by the boundary extension. One-shot wakeup is the scheduler policy modeled,
not a fixed type or restriction imposed on every continuation object by Id.

There is no proved refinement between the TLA+ model and the Lean model, nor from
either to production code. They check complementary parts of the design.

## Reproduction

From the repository root:

```sh
python3 scripts/check-continuation-model.py --fetch-tools
```

This uses the existing pinned Lean 4.24.0 and TLC 1.7.4 tool definitions in
`formal/rooting/tools.json`; Java is required. Omit `--fetch-tools` once cached.
Existing tools can be supplied with `--lean`, `--tla-jar` and `--java`.
No tools are downloaded without the explicit flag.

The runner uses one TLC worker, bounded subprocess timeouts, fresh Lean outputs,
strict expected-failure checks and source hashes. Logs, counterexample traces
and `receipt.json` are written to `.cargo-target/continuations/check/`.

## Next proof and implementation boundary

These models can support an initial design decision about opaque owner state and
explicit transfer. They do not yet establish a complete continuation-aware Id.
The boundary extension supplies finite metacircular lookup and native callback
models plus unbounded native-scope proofs. Before choosing an ABI, implement a
small executable model using the public Oop protocol and compare its traces with
the specification, including suspending lookup and its callbacks. Only then should the
compiler and existing Execution machinery be migrated.

Reference tool guidance: [TLC behavior and fairness checks](https://dl.tlapl.us/tlatoolbox/doc/model/overview-page.html)
and [Lean kernel dependency auditing](https://lean-lang.org/doc/reference/latest/ValidatingProofs/).
