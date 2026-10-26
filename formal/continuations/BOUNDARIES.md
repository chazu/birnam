# Metacircular lookup, bootstrap and native callbacks

This extends the [first continuation model](README.md). It is a semantic model,
not a new ABI, an implementation of Id, or a native-code verification result.
The production runtime is unchanged.

## Concrete dispatch sequence

The earlier model summarized lookup as phases and one binding read. The new
`Metacircular.tla` expands the recursion explicitly:

```text
send(object, work, arguments, answerDestination)
  send(vtable(object), lookup, [work], enterOriginal)
    send(vtable(vtable(object)), lookup, [lookup], enterPreviousLookup)
      ... designated bootstrap lookup send ...
      bootstrap owner reads its current lookup binding
    enter selected lookup Method, return the Method it selects
  enter selected lookup Method, return the Method it selects
enter original selected Method with original arguments and answer destination
```

Each pending send is saved by a continuation owner. Returning a Method from
lookup resumes that owner's pending entry directly. It does not look up the
original selector again and does not send an implicit application message to
the selected Method. Argument identity/order and the original destination are
retained. The model keeps a separate selection witness to check entry against
the actual returned Method.

The base case applies only to the designated root receiver and the `lookup`
selector. It reads the **current binding**, rather than substituting a permanently
cached lookup Method. Each level's lookup binding can independently be replaced
once in the finite model; the work Method can also be replaced. The root binding
is replaceable under the same rule. The bootstrap owner's primitive table search
interprets that owner's own table representation; it is not permission for Id
to decode arbitrary vtable or continuation payloads.

This models the minimal recursive algorithm, without the production cache/default
lookup shortcuts. The bootstrap graph has a well-founded chain of vtables ending
at a self-associated root. Its concrete heights are two and three. The Lean
`metaPath` theorem establishes the chain length and terminal root for arbitrary
height. Arbitrary malleable graphs may have cycles; neither tool proves that
all possible replacements of lookup or vtable associations terminate.

## Lookup suspension and re-entry

Lookup can suspend after performing an effect and before reading its dictionary.
Wakeup continues that saved phase, without repeating the effect. Replacement and
collection can happen between selection and entry as well.

The lookup for `work` also calls an ordinary source helper on the original object
before selecting the target. That helper runs through the very same metacircular
dispatcher. It has the same receiver and argument count as the eventual work
call, but a different destination. Its answer must resume lookup, not replace
the pending work call. A separate saved lookup activation retains its selected
Method and code while the helper runs or suspends.

This directly tests the ambiguity that defeated the earlier ambient tail-hint
experiment. A mutation that discards this lookup continuation must fail. The
effect-count invariant counts both the current lookup and all pending lookup
activations, so nested source re-entry cannot silently lose unfinished work.

## Bootstrap publication

The model separates graph construction, installation of seed bindings and
publication of ordinary send. Sending is unavailable before publication. An
early-publication mutation violates the readiness invariant. Lean proves this
readiness invariant for arbitrary sequences of attempted bootstrap operations,
including attempts in the wrong order.

This is logical bootstrap readiness. It does not model allocation failure,
partially constructed machine words, dynamic-library loading or publication to
another OS thread. The native bootstrap still requires a concrete implementation
and its own acceptance checks. No prebuilt Birnam artifact is assumed. The base-case sequence follows the
[Piumarta/Warth paper, section 3.2](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf);
its cached/default-lookup optimizations are outside this model.

## Native callback boundary

The selected work Method is native and calls a source helper. That helper and
its lookup methods use the same dispatcher as the outer send. Two configurations
exercise different native contracts:

| Native contract | When a callback tries to suspend | How completion returns |
| --- | --- | --- |
| Synchronous | Refused while its waiting native activation remains live; the callback can handle refusal and finish synchronously | The callback answers, then its native caller returns once |
| Explicit asynchronous handoff | Allowed only after the originating native activation has returned, and if no ancestor native activation remains | An owned completion continuation delivers the language answer once; it does not return to an expired C frame |

The native Method's code is retained while its callback runs, including after an
asynchronous native return. A duplicate completion is rejected. Attempting a
second drive of the same running computation is rejected; ordinary nested source
entry remains part of that current drive. This does not forbid separate domains
from running independently.

`NativeScopes.tla` checks the additional nested case: an asynchronous inner native
activation returns while an outer synchronous native activation is still waiting.
Removing the inner activation must leave the outer barrier intact. The callback
handles suspension refusal and completes synchronously before the outer caller
returns. Once all native activations are gone, a later detached wait is permitted.
Foreign return identities, wrong-owner completion and duplicate completion are
also exercised. These are execution-owner policies over Oop identities; Id does
not inspect native frames or continuation payloads.

Lean proves preservation of the native-boundary invariant for arbitrary finite
histories and arbitrary nesting depth, including out-of-order return attempts.
It also proves that inner return preserves ancestors, foreign return cannot pop
another activation, and a successful owner-scoped completion preserves the answer
destination and cannot be repeated.

## Checked alternatives and limits

Required bad variants remove the bootstrap anchor, publish too early, freeze the
anchor binding, bypass metacircular sends, repeat selection, lose arguments,
replay lookup effects, insert lookup on a selected Method, drop selected code,
discard a lookup callback's return state, cross a native barrier, lose a native
return continuation, repeat completion or admit overlapping drives. Nested native
variants erase ancestors, accept a foreign return, ignore completion ownership,
reuse a ticket or ignore a live barrier.

Separate witness checks establish reachable suspended lookup, source re-entry
during lookup, suspension after asynchronous native return, synchronous suspension
refusal, execution of replaced lookup code and rejected duplicate completion.
The checker also retains all earlier interleaving models and Lean proofs.

TLA+ explores one complete outer send with its lookup-source callback and native
source callback, finite Methods and replacements, and at most one pause at each
modeled lookup activation. The earlier two/three-computation models still supply
interleaving coverage; this is not their Cartesian product with the new model.
Completed pause history and the full historical sequence of entered lookup
Methods are omitted: the current activation pause flag, effect/read counts,
entered-Method set and live selections suffice for these properties. No trace
equivalence for arbitrary effect ordering is claimed by that abstraction.
Progress assumes finite modeled method bodies, weak fairness for execution and
wakeup, and eventual external completion. Refused suspension is handled by the
test callback; an arbitrary callback need not recover or terminate.

The enum-like labels in TLA+ and Lean name specification operations and owner
states, not a payload-type catalog for runtime Oops. The generic Id/Execution
representation-independence proof from the first checkpoint is unchanged.

Remaining gaps include complete DNU/error and exception-unwind semantics,
cancellation/cleanup, actual root-provider correctness, address reuse, arbitrary
user lookup programs and graphs, native code generation, calling-convention and
trampoline implementation, and a formal correspondence between TLA+, Lean and
executable code. The new checked cases do not close these gaps by implication.

## Reproduce

Run the full package with `python3 scripts/check-continuation-model.py`, supplying
the existing tool options or `--fetch-tools` as described in [README.md](README.md).
For development, `--suite boundaries` checks the new TLA+ models and all Lean
modules; its receipt identifies that narrower suite. The final checkpoint uses
the full suite. Every new Lean mutation also has a concrete kernel-evaluated
counterexample, avoiding rejection solely because an old proof script no longer
fits an equivalent implementation.
