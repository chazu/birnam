# 60E — Resumable Registration and the Entry Seam

> **Status:** S0 decision record, proposed for acceptance, 2026-08-04.
> This record closes the registration seam that
> [60A](60a-stackless-execution-semantics.md) §5 and
> [60C](60c-stepmethod-calling-abi-options.md) §4.2 both depend on and both
> defer. It decides what a vtable holds for a resumable implementation, how a
> send from unmigrated code reaches one, and how the driver recognizes one. It
> does not define compiler lowering, liveness, or source maps; those remain the
> later compiler record.

## 1. The gap this closes

60C freezes a StepMethod as an ordinary `Method` invoked with a Machine receiver
and answering a StepAction. 60A §5.3 then says the driver pushes a resumable
frame "if the address is registered as resumable". Neither record says what
performs that registration, where the record lives, or what an ordinary send
does when it reaches a resumable selector.

That omission is not cosmetic. The floor installs methods as bare addresses:
`birnam_install_method` in birnam-codegen-runtime reaches the Store MOP
`addMethod:on:with:`, whose `tracked_add_method` transmutes the carrier and
calls `api.method`. `Vtables::lookup` answers that address and `send_abi` calls
it immediately. `MethodAddress` is a bare non-null pointer with no tag. Nothing
on that path can consult a registration record.

So if a StepMethod address were installed in a vtable under its own selector,
every ordinary send to that selector — from unmigrated Rust methods, the REPL,
`modules/*.birnam`, or reflection — would invoke it with a non-Machine receiver
and a nonzero argument count. Per 60C §4.1 the entry answers an AbortAction, and
**the caller receives that AbortAction as its ordinary result value.** A wrong
answer, not a failure. This record removes that possibility by construction.

## 2. S0 decisions

| Area | Decision |
|---|---|
| Vtable contents | A vtable never holds a StepMethod address. It holds an ordinary Method: a synchronous implementation or an **entry thunk**. |
| Driver path | A Call or TailCall whose resolved implementation names a live ResumableEntry pushes a frame and calls the StepMethod. **The thunk is not invoked.** |
| Send path | An ordinary `IdApi.send` from unmigrated code reaches the thunk, which drives the implementation and answers the ordinary result. |
| Nested entry | A thunk reached from inside an active drive's synchronous barrier pushes onto the **current Machine** above an entry-barrier delimiter rather than allocating a second Machine. A thunk reached with no active barrier starts a fresh Machine. |
| Registration record | A process-local **ResumableEntry** owns the StepMethod address, FrameDescriptor, generation pin, declared arity, and implementation identity. |
| Recognition | Stated as a requirement, not a mechanism: an implementation named by its **defining** vtable and selector maps to at most one live entry, never keyed by code address alone, and never aliasing across unload. |
| Preservation | Migration preserves result and error values for a matching argument count. It preserves neither mismatched-arity behavior (§6) nor maximum call depth (§5.3). |
| Failure surface | A 60A §9 driver failure reaching a thunk boundary is fatal, never an ordinary result value. |
| Registration surfaces | The Store MOP for named methods, direct `birnam_method` for block bodies, `Tcc install:as:on:` for symbols resolved from a relocated unit, and image boot, which restores installs and must re-register before any send reaches a restored thunk. All produce the same record. |
| Microcode | Microcode selectors are sealed in the method dictionary against add, overwrite, removal, and shadowing through a private child vtable alike. |

## 3. Two paths, and only one of them is the thunk

This is the distinction the rest of the record depends on, so it is stated once,
explicitly:

| Caller | Resolution | Cost |
|---|---|---|
| Migrated: a StepMethod issuing a Call or TailCall | The driver resolves the implementation (60A §5.2), finds a live ResumableEntry, and pushes a frame directly | One frame push. No thunk, no native frame, no Machine acquisition. |
| Unmigrated: any ordinary `IdApi.send` | Lookup answers the thunk address and `send_abi` calls it | One native activation plus a nested drive. |

The migrated-to-migrated path is the one the whole design exists for, and it
never passes through a thunk. An implementation that routes it through the thunk
reintroduces exactly the native-stack growth 60 exists to remove, and will fail
60 §7 proof point 2 on its first run.

### 3.1 Thunk shape

The thunk is an ordinary Method and introduces no new function shape:

```c
static Oop example_entry(Oop receiver, const Oop *arguments, size_t count) {
    return birnam_enter_resumable(EXAMPLE_ENTRY, receiver, arguments, count);
}
```

`birnam_enter_resumable` is a host-provided runtime symbol in the same family as
`birnam_method` and `birnam_install_method`. It is not an IdApi field, so 60C's
commitment that the id floor does not grow for stackless execution holds; 60C
§14's evidence item 8 is amended to match (§9.3).

A per-implementation thunk is the obvious emission and is what a compiler will
produce. It is **not** required, but the alternative is not one shared thunk.

A single shared thunk cannot work: the Method ABI conveys no selector, so a
thunk invoked under an arbitrary selector has no way to learn which
implementation it is standing in for, and 60D §4 forbids the ambient state that
would supply one. Recovering the entry from the receiver's vtable is not a way
out either — that is the receiver-keyed rule §4.1 rejects, and it misses every
inherited method.

What preserves the no-code-generator path is a **pool of distinct runtime-owned
thunks**, one bound per registration. Each pooled address is unique, so it
identifies its entry without a selector, and none of them has to be compiled at
registration time. That is what lets a resumable implementation be registered
without running a code generator, which 60C §13.4 preserves for a future
non-native or sandboxed artifact format and which an interpreter tier would
need. Whether the pool is pre-built, grown, or exhausted is not decided here.

Note that this is a property of the *thunk*, which needs to identify its own
entry. It does not soften §4.1, which governs a different question: how the
**driver** maps an implementation it has resolved to an entry, without invoking
a thunk at all.

### 3.2 Thunk behavior

`birnam_enter_resumable` performs:

1. resolve the ResumableEntry and validate its generation is still live;
2. consult the Runtime drive stack (§5.2) to decide fresh or nested entry;
3. push the initial frame, binding receiver and arguments into traced frame
   slots under the §6 arity rule;
4. drive until that frame returns;
5. answer the returned value as an ordinary Oop.

A completed drive is indistinguishable from a synchronous return. A caller that
never mentions Machine never sees one.

### 3.3 Driver failures do not become values

Step 5 answers only language values. This matters because 60A §9 classes driver,
ownership, malformed-action, and barrier failures as out-of-band typed runtime
errors, and a thunk invoked inside an ordinary send **has no out-of-band
channel** — its only carrier is the returned Oop.

Converting those failures into ordinary error values would recreate the §1
hazard in a new place, and the floor would not treat them consistently.
`vector_do` in birnam-collection discards its block's result entirely, so a
driver failure raised inside `do:` would vanish and the loop would continue;
`block_loop` and `integer_to_do` in birnam-prelude check for errors and return
them, aborting their loop. The same runtime fault would end `whileTrue:` and
`to:do:` as an ordinary error value and be invisible in `do:`. Worse, because
this language has no non-local return and errors are ordinary values, `on:do:`
would happily catch a driver failure and continue running against a Machine the
driver has just declared invalid.

**Decision.** A 60A §9 failure that reaches a thunk boundary is a fatal
invariant, reported through the same host path `send_abi` already uses when
`doesNotUnderstand:` is unbound. It is not returned.

A later record wanting a recoverable subset cannot get one by choosing an error
kind. `kindMatches:` in `src/id/stdlib.birnam` answers true whenever the wanted
kind is `#error`, before consulting the receiver's kind at all, so `#error` is
an unconditional wildcard over every `ErrorVt` value and `on:do:` reaches
`kindMatches:` only for values that are already `ErrorVt`. A recoverable subset
would therefore need a representation that is not an `ErrorVt` value, and it
must amend 60A §9, which today says none of its kinds are in-band.

### 3.4 The failure surface is not uniform during migration

§5.1 makes a drive entered from unmigrated code a nested drive, and §3.3 makes
its failures fatal, while a drive the host starts directly reports 60A §6's
`Failed(driver error)` as a value. Until the S4 primitives are migrated, most
drives are nested, so the same runtime fault aborts the process in one context
and answers a host result in another, decided only by whether an unmigrated
frame happens to sit below it.

That non-uniformity is accepted rather than hidden: the alternative is the §1
hazard. It is a reason to prioritise S4, and a reason the S3 host-drive surface
should be exercised on a fully migrated probe rather than on ordinary program
text.

## 4. The ResumableEntry record

    ResumableEntry
      StepMethod address           (what the driver calls)
      FrameDescriptor identity     (60B §4)
      generation identity and pin  (60B §9)
      declared arity
      implementation identity      (for FrameView, source maps, and 49)

The registry is process-local, owner-thread-checked, and is not a GC root set.
Entries are validated by index plus generation, following the Machine registry
pattern of 60B §1.

### 4.1 The recognition requirement

The driver holds a resolved address and must decide whether it is resumable.
This record states the requirement functionally and leaves the mechanism open:

> Recognition must map a resolved *implementation* to at most one live
> ResumableEntry, and must never associate an implementation with an entry from
> a different generation.

The implementation is named by the **defining** vtable and the selector — not by
the code address alone, which §4.2 shows can be recycled beneath a live vtable,
and not by the receiver's vtable, which is wrong for every inherited method. A
per-selector side table on the vtable record and a tag word beside the
dictionary entry both satisfy this. What none of them may do is outlive the code
they name.

The floor's lookup does not report the defining vtable today: `Vtables::lookup`
walks the parent chain and answers an address, discarding which record it came
from. The driver's resolution therefore needs a lookup variant that reports
both. That variant is internal to the floor — the driver is Rust — so it grows
no IdApi surface, and it must not cost a second parent-chain walk, which would
be unbudgeted against §11's Call/Return line.

The alternative of keying on the receiver's vtable is not merely less precise;
it is silently wrong. A resumable method installed on a prototype and sent to a
child would miss, the driver would conclude "synchronous Method", and every
inherited migrated call would degrade to the thunk path — restoring the
per-level native activation §3 exists to remove, in exactly the case hardest to
attribute.

### 4.2 Address recycling, in both directions

Code addresses are recyclable: `dlclose` runs in birnam-codegen-runtime's
`discardCandidate:` and `releaseCandidate:`, and a later `dlopen` may map new
code at the same address. Two directions follow, and only the first is a
registry problem.

**False positive.** A stale entry naming recycled code would make the driver
push a frame for the wrong implementation. Prevented by ordering: an entry is
unregistered before its generation's code is unloaded, and unloading is refused
while the generation is pinned (§7).

**False negative, and it is worse.** A live vtable may hold a method address
from an unloaded generation. Block vtables are the case: generated code builds
them inline at first evaluation and installs `value:` through `birnam_method`,
and while the module-local `static` cache dies with the generation, the vtable
Oop and every block instance holding it do not. Store such a block in Globals or
a fact, activate a successor generation, and `releaseCandidate:` the old one —
it is neither active nor prepared, so nothing refuses, and `dlclose` runs. A
later send to that block calls freed code.

This is a pre-existing defect, not one 60E introduces, but 60E changes its
character: the driver checks the registry, correctly misses, concludes
"synchronous Method", and invokes the recycled address through the synchronous
barrier — the one path with no validation at all. A crash becomes a silent
misdispatch. §7 states the gate that closes both directions.

### 4.3 Registration surfaces, and the one that cannot carry a registration

Three surfaces are live installs that can carry a registration alongside them:
the Store MOP for named methods, direct `birnam_method` for block bodies, and
`Tcc install:as:on:`, which resolves a symbol from a relocated unit and calls
`ensurePrivateVtable` plus `api.method` — the path `Alien wrap:signature:as:on:`
is built on. The fourth cannot, and it is the one that breaks if registration is
treated as a side effect of installation.

**Image boot restores installs without registrations.** The image heap encoder
skips `SLOT_CODE` words outright, so no method address is serialized at all.
Addresses come back only by re-running installs: boot maps the image's compiled
code unit and lets its initializer install, through `birnam_install_method` for
named methods and `birnam_method` for the block vtables rebuilt from source.
Both signatures carry a vtable, a selector, and an address, and neither can
carry a FrameDescriptor, declared arity, generation pin, or implementation
identity. So a thunk address comes back into the vtable and no ResumableEntry
comes back with it.

Under §3.3 that is not a degraded mode, it is an abort: the first send to a
migrated selector reaches a restored thunk, finds no live entry, and raises a
60A §9 failure at a thunk boundary. Worse, a restored entry identity that
happens to match a live registry slot names the *wrong* implementation — the
§4.2 false positive arriving by a route the ordering rule does not cover,
because no unload occurred. The rebuild path is unaffected, since it
re-evaluates method definitions through the Store MOP, so mapped and rebuilt
images would differ in correctness — two routes to the same image answering
differently, which is the kind of divergence 60 §3's compatibility invariants
exist to prevent.

**Decision.** Registration is an emitted artifact operation, not a side effect
of installation. Artifact lowering emits a registration call beside each
install; image boot re-registers before any send can reach a restored thunk; and
a restored thunk with no live entry answers `resumableEntryUnrestored` as an
**image validity failure** rather than a runtime fatal on first use.

The check point has to be stated precisely, because "before execution" is not
available: restored thunks come into being *while* the mapped code unit's
initializer runs, and that initializer is ordinary program text interleaved with
installs. The check therefore runs after the code unit's initializer completes
and before the image's entry point, or as a manifest-versus-registry
cross-check that does not require executing the unit at all.

This also settles what would otherwise be an open question about the durable
projection: it must be self-describing about which methods are resumable,
because boot cannot reconstruct that otherwise.

Two costs, stated plainly. Registration needs its own emitted symbol, since
`birnam_method`'s signature cannot express an entry and widening it would break
every existing artifact — so §9.3's amendment names two symbols rather than one.
And the registration shim joins the image's hashed runtime shim list, changing
the runtime ABI hash, so images written before this lands degrade to the rebuild
path. Design 40 already designs for that degradation; it is a cost to state, not
a defect. The Codegen shim table is not hashed and does not carry that cost.

## 5. Entering from unmigrated code

60A §5 requires that a synchronous Method's send may enter resumable methods and
must drive them to completion. Every thunk invocation is that case, and it is
the path every unmigrated caller takes throughout S2 and S4.

### 5.1 Nested entry reuses the current Machine

**Decision.** When a thunk is reached while a synchronous barrier established by
an active drive is dynamically in scope on this owner thread, it does not
allocate a second Machine. It pushes the new frame onto that Machine's arena
above an **entry-barrier delimiter**, and runs a nested drive that may not pop
below that delimiter. A thunk reached with no barrier in scope starts a fresh
Machine; §5.2 explains why the predicate is barrier scope rather than thread
activity. When the nested frames
return, the thunk answers and the intervening synchronous activation returns
normally to the outer driver.

The alternative — one fresh Machine per crossing, recycled through a pool — was
rejected. It allocates a MachineState and arena per boundary crossing on a path
that is already the expensive one; it produces N unlinked Machines for one
logical call stack, which no backtrace or future debugger can walk; and pooling
Machine identity is unsound, because 60B §1 and §3 make registry-slot reuse
advance the capability generation precisely so a stale Machine Oop fails rather
than aliasing a new execution. A pool that hands the same Machine Oop back
defeats that: a FrameView retained from one drive would validate successfully
against the next and read another execution's slots through a stale arena
handle.

Sharing one Machine between stacked drives is only sound under four rules. Each
closes a defect that the naive reading permits, and none is optional.

**Epochs are monotonic and never restored.** The Machine may reuse scratch
StepAction objects (60B §5), and a nested drive rewrites the same scratch set.
If the outer drive's epoch were restored on return, an outer action Oop would
validate again while carrying payload the nested drive wrote — an ABA that
hands the driver a ReturnAction where it expected a CallAction, and a wrong slot
index with it. Every drive entry advances the epoch, so an action issued before
a nested drive is permanently stale afterward.

**The driver never re-reads an action payload across generated code.** It copies
every operand it needs into driver-local scalars before releasing its borrow to
invoke a synchronous Method, per 60D §6.2. Monotonic epochs make a violation
fail loudly; copying makes it not arise.

**Delimiter searches terminate at the innermost entry barrier.** 60A §4 lets
Abort unwind to a delimiter and 60A §6 lets Restore install at a compatible one.
With one Machine, the outer drive's delimiters are in the same chain. Unwinding
or splicing past an entry barrier would pop frames whose native activations are
still live on the C stack, returning into a driver whose current frame no longer
exists. Abort, Restore, and Capture all stop at the barrier and fail
`continuationBarrier`.

**A Return at the barrier frame ends the drive, not the Machine.** 60A §5 says a
Return with no caller completes the Machine, and a thunk-pushed frame has no
caller *frame*. Taken literally that would complete a Machine whose outer drive
is still parked in a native activation, and 60B §3's teardown would release
chunks and generation pins the outer frames still hold. MachineState therefore
carries a drive depth; Running to Completed requires depth zero.

**Arena slot generations advance on pop.** Nesting pushes and pops at the same
arena offsets repeatedly, which makes slot reuse the common case rather than a
rare one, so the generation counter is the only thing distinguishing a stale
FrameView from a live one. It must advance on every pop, not only on chunk
release.

Reusing the current Machine also makes three properties fall out of mechanisms
60B and 60A already define rather than being asserted separately:

- the entry-barrier delimiter is a delimiter record (60B §8), so capture above
  it fails with the existing `continuationBarrier` — no new kind, and 60A §7's
  requirement that the barrier be dynamically scoped and visible to capture
  validation is satisfied by construction;
- suspension requested above an entry barrier fails the same way, which is what
  60A §5 already requires of a drive entered from a synchronous activation;
- one logical execution keeps all its frames in one Machine's arena, so a
  backtrace enumerates that arena — the same mechanism tracing uses below —
  rather than reconstructing a chain across unlinked Machines.

Drive depth is tracked explicitly on MachineState rather than read off the frame
chain, because the caller chain is severed at every entry barrier: a barrier
frame's caller is a native activation, not another frame.

Tracing must follow the arena, not the frame chain. A chain walk from the
current frame terminates at an entry-barrier frame, because that frame's caller
is a native activation rather than another frame — and the outer frames it
stopped short of hold Oops that the native-stack scan does not cover, since they
live in arena chunks rather than on the C stack. 60B §6's trace hook therefore
enumerates every live frame in the arena. The scoped root provider may name one
Machine once per stacked drive, so marking must tolerate duplicates.

### 5.2 The drive stack is the one piece of ambient state

Deciding fresh-versus-nested requires knowing whether the send is dynamically
inside a drive's synchronous barrier, and the thunk holds no Machine — that is
the whole nature of the seam. The Runtime therefore owns a drive stack whose
entries are pushed by the driver when it *enters a synchronous barrier* and
popped when that barrier returns.

The predicate is barrier scope, not thread activity, and the difference is not
pedantic. A Machine may be driving while its synchronous leaf asks the host to
begin unrelated work — a reactor taking a fresh item, a Store MOP hook, a
codegen authorizer, an FFI callback — and that work reaches a thunk by ordinary
dispatch. Nesting it onto the driving Machine would give a new logical execution
no identity a scheduler could queue, no ability to suspend, a lifetime tied to
an unrelated Machine, and a backtrace interleaved with one. A thunk reached with
an active drive but no active barrier starts a fresh Machine, which is what 60D
§3 wants when it lists several Machines on one owner thread as a direct fit.

60D §4 forbids ambient execution identity, and that prohibition stands where it
was aimed: **generated StepMethod code and Machine microcode never consult the
drive stack.** They receive Machine explicitly as their receiver. The drive
stack is consulted only by the entry shim, at the one boundary where no Machine
is in hand, and 60D §4 is amended to say so rather than left to be read as
forbidding it.

Acquisition and release are point operations. No drive-stack, registry, or
MachineState borrow is held across generated code, `IdApi.send`, allocation,
collection, or a foreign callback, per 60D §6.2. The drive stack must be a
distinct borrow domain from the machine registry, or the nested case
self-deadlocks on the second entry.

The Machine has one drive epoch, not one per drive, and validation always
compares against that single current value. It advances on drive entry **and on
drive exit**. Recording a per-drive epoch and comparing against the innermost
active drive would not give the property §5.1 requires: on return the innermost
drive is the outer one again, its recorded value is its pre-nesting epoch, and
an action issued before the nested drive would validate against it while
carrying payload the nested drive overwrote. Advancing on exit is what makes the
staleness permanent.

### 5.3 What migration does and does not preserve

Migration preserves the result value, error values, and selector-determined
arity. **It does not preserve maximum call depth, and the change is not always
favorable.**

For a chain of migrated calls, native depth becomes constant where it was
proportional to Birnam depth. That is the win. For a chain that alternates
migrated and unmigrated frames, native depth stays proportional to the number of
boundary crossings and the per-crossing constant *grows*: driver, barrier,
synchronous method, `send_abi`, `bind_active` with its nested metacircular
`lookup:`, thunk, nested driver. Stated honestly:

> native depth goes from Θ(Birnam depth) to Θ(boundary crossings) × a larger
> per-crossing constant.

That is a win only where migrated regions are long runs. During S2–S4 it is not:
while `do:`, `whileTrue:`, `to:do:`, `inject:into:`, and `on:do:` remain
synchronous, every recursion passing through one of them crosses a boundary per
level, so a deep recursive tree walk can reach the native stack limit *sooner*
after migration than before.

Three consequences. First, no artificial depth bound is introduced: adding one
would convert a program that answers today into one that answers an error, and
the OS stack limit remains the bound it is today. Second, this reorders the
argument for S4: the callback-driving primitives are not a convenience
migration, they are what converts the alternating case into the long-run case.
S2 must measure and publish both axes — per-iteration entry cost in a loop, and
maximum reachable depth for a recursion through an unmigrated primitive —
because the second is the one that regresses.

Third, and least comfortable: **suspension is largely unreachable until S4.**
Capture and suspension fail above an entry barrier (§5.1), and boundary
crossings sit near the top of a program's dynamic extent — `main` reaches `do:`
reaches everything — so during S2–S4 the barrier sits below essentially all real
work. `collection do: [:each | ... ]` cannot suspend at any depth until `do:` is
migrated. S3 can therefore be demonstrated only on a fully migrated probe, and
the reactor rationale of 60D §4 is not supported by ordinary program text until
S4 lands. That is an argument for treating S3's acceptance as probe-scoped, not
for weakening the barrier: the barrier is what a live native activation
physically requires.

## 6. Arity

The floor has no per-method arity. `add_method` stores selector and address
only; `doesNotUnderstand` fires exactly when lookup misses; `send_abi`'s only
argument check is the null-with-nonzero-count invariant. Arity is carried by the
selector spelling, so an ordinary compiled send cannot mismatch. Where a
mismatch does arrive — a native caller, or DNU forwarding — a codegen'd method
today ignores the count and a Rust method answers nil.

Two different checks are easy to conflate here, and they answer differently.

**Language arity**, at frame push, is shared by the driver path and the entry
path and is variadic-tolerant: bind `min(count, arity)` slots and nil-fill the
remainder. It does not fabricate a `doesNotUnderstand` result.

**Native buffer arity**, at StepMethod invocation, is strict: a count the
FrameDescriptor does not declare is a malformed artifact and is rejected, per
60C §4.1. That buffer is produced by the driver, not by a language caller, so
tolerance there would only hide a broken artifact.

Language-arity tolerance is a behavior change, and §2's preservation claim is
correspondingly narrow. A synchronous Rust method today typically answers nil
*without running* on a count mismatch — `vector_do` returns nil when its count
is not 1 — whereas a migrated version runs its whole body with nil-filled
arguments. Ordinary compiled sends cannot reach this, since the selector fixes
the count, but native callers, reflective invocation, and DNU forwarding can.

`doesNotUnderstand:` itself is not migratable in S1–S4. `send_abi` forwards an
unbound send to it with `1 + min(n, 16)` arguments, so a resumable
`doesNotUnderstand:` declaring arity 1 would mismatch on every keyword miss;
answering DNU on mismatch would then resolve to the same implementation and
recurse. The floor already fixed this shape of bug once — a `count != 1` guard
in `object_dnu` that made every keyword and binary send answer nil instead of
erroring, corrected in "fix(error): raise doesNotUnderstand at every send
arity" — and the entry seam must not reintroduce it.

## 7. The unload gate

60B §9 forbids disposing code while a frame or continuation pins its generation.
The current paths do not implement that, and today's refusal is not even a
failure: `discardCandidate:` and `releaseCandidate:` refuse only while a
candidate is active or prepared, and on refusal call `note_error` — which
stashes
a byte string in a mutex — then return the unit itself, indistinguishable from
success.

S3 must therefore *introduce* a kinded failure here, not reuse one. Its
condition must be wider than 60B §9 states, because of §4.2's second direction:

> Disposal is refused unless the generation has zero live frames, zero live
> continuations, and no reachable vtable holds a method address in its mapped
> range.

The third clause is the honest one and it is not currently satisfiable, which
this record states rather than papers over. Vtables are allocated permanent and
pushed onto an ordered registry that has no removal path, so enumerating with
`vtableCount` / `vtableAt` / `vtableSelectorAt` — and resolving each selector
through the floor's internal lookup, since the enumeration answers selectors
rather than addresses — reports *registry membership*, not reachability. Every
generation evaluates blocks, every block evaluation installs into a vtable that
the registry then holds forever, so a membership test answers "held" for every
generation and disposal never runs again.

The consequence is that until the vtable registry can distinguish reachable from
merely registered, the gate degenerates to refusing disposal outright. That is
the safe direction — leaked mapped code rather than misdispatch into freed code
— but it is a leak per evolution cycle, and it means §4.2's false-negative
direction is *contained, not closed*. Closing it needs one of: a collectable
vtable registry, re-homing block vtables onto a surviving trampoline at unload,
or an explicit unload-time sweep that rebinds affected selectors. That work
belongs to the lifetime record (Future 60G), and the recognition path of §4.1
must not be read as depending on it.

**Staging.** Recognition ships in S1–S2 while this gate is an S3 deliverable, so
the interval is knowingly exposed. Since the gate degenerates to refusal anyway,
S1 should refuse code disposal for any generation whose code is reachable from a
registered vtable, and record the refusal as a kinded failure from the start.
That predicate is the one §7 says is not yet computable, so in practice S1
refuses whenever the generation installed a method — but the refusal must be
stated in terms of reachability, so that making the registry collectable later
narrows it rather than requiring the rule to be rewritten. Refusing disposal
must not suppress the rest of `discardCandidate:`: the author file, the
`.candidate` sidecar, and the `state discarded` fact are Store state Design 49's
reconciliation reads.

**Evidence.** Because the refusal is unconditional in practice, disposal
behavior cannot serve as evidence that generation pinning works — a stubbed pin
implementation would produce an identical observation. The S3 pin evidence must
observe pin counts directly across frame push, pop, capture, and consume. 60 §7
proof point 10 should be read that way.

### 7.1 An active frame does not consult the registry

A frame captures its StepMethod address, FrameDescriptor identity, and
generation pin when it is pushed. The registry is consulted at Call and TailCall
resolution and at entry — never per step for a frame that already exists.

Without that rule, unregistration would reach into live execution.
`Reflect defineMethod:` for a selector whose implementation is on the frame
stack would unregister its entry, and the next step of that still-live frame
would fail to resolve. Design 49's quiescence guard covers `CompiledUnit
activate`, not reflective redefinition, so the case is reachable. It would also
contradict 60A §8, 60B §9, and 60D §11, which all say an active frame continues
through its pinned implementation generation while new sends resolve late.

60C §8 step 2 is amended to match: the driver resolves a registered entry when
pushing a frame, not on every iteration of its loop.

### 7.2 Entries are released on overwrite and removal, not only on unload

Nothing about generation unload covers a method being replaced. `add_method`
overwriting a thunk, and `removeMethod:on:` removing one, must both unregister
the entry.

The mechanism is the one §8 establishes for the seal, not a hook: neither the
overwrite hook nor the Store authorizer can serve, for the reasons given there.
The dictionary entry carries a registered-entry marker, set by the stackless
runtime layer and consulted by `add_method` and `remove_method` alike. Same
per-selector shape, same layering, no new ABI. Without that, a REPL redefining
one method accumulates a live entry
per redefinition in a registry that is deliberately not a GC root set, so
nothing ever collects them.

## 8. Sealing the microcode

60C §15 Q4 asks how primitive-backed Machine methods are identified so an
arbitrary resumable method cannot displace one. The answer must be a real veto,
and the two mechanisms that look like one are not.

**The method-overwrite hook cannot refuse.** `MethodOverwriteHook` returns void,
and `Runtime::add_method` calls it and then overwrites unconditionally; its
contract permits only Store-log mutation and read-only queries. It is a single
slot, already claimed by birnam-store's mirror, so installing a sealing hook
would silently destroy the Store MOP mirror. Changing its signature would also
break a frozen ABI field offset and 60 §3 invariant 10.

**The Store method-mutation authorizer can refuse, but does not see
everything.**
`tracked_add_method` consults an authorizer and returns its answer, which is a
genuine veto — but it is also single-slot and already taken by codegen-runtime,
and it guards only the Store MOP path. Both `birnam_method` and `Tcc
install:as:on:` bypass Store entirely, which are the paths block bodies and
FFI wrappers use.

**Private vtable placement is not protection either, and the reason is worse
than overwriting.** `Reflect defineMethod:on:as:` calls `ensurePrivateVtable` on
its target, and when the target does not already own its vtable that call
*creates a child vtable* with the old one as parent and re-points the object. So
`[Reflect defineMethod: #currentPc on: aMachine as: [:self | 7]]` never touches
the sealed entry on Machine's vtable: it installs into a fresh child dictionary,
which is an add rather than an overwrite, and no hook fires because the child's
direct lookup misses. Ordinary dispatch from a StepMethod then finds the shadow
first, and the driver reports `invalidStepAction` on a healthy program — fatal
under §3.3 if any barrier is below it.

**Decision.** The seal is consulted on any install of a sealed selector into any
vtable whose parent chain reaches Machine's vtable, not only on mutation of the
record that carries the seal. Shadowing is the case that matters, because it is
the one an ordinary reflective operation reaches without touching anything
sealed.

**Decision.** The seal is a per-selector flag in the method dictionary,
consulted by every mutation path — add, overwrite, and remove alike. Removal
matters as much as overwrite: removing `currentPc` makes the next microcode send
miss, fall to `object_dnu`, and answer an error Oop where the step expected a
program counter or a StepAction, so the driver reports `invalidStepAction` on a
healthy program.

This adds no IdApi surface. Machine's vtable and its microcode are installed by
the stackless runtime layer in Rust, which sets the flag through internal
runtime API; nothing outside the floor needs to seal anything. Malleability
elsewhere on Machine — inspection, policy, drive protocol — is unaffected. The
seal covers only the irreducible frame and action operations of 60B §1, which
60C §6 already argues is the honest place for this boundary.

## 9. Amendments to the 60C proposal

These apply on acceptance of 60C, which remains an RFC. 60E does not freeze
them independently.

### 9.1 The driver argument count

60C freezes "the initial driver contract uses zero arguments" while its own §15
Q1 asks whether that is right. Zero is the wrong thing to freeze. The buffer
already exists in the frozen Method ABI and costs nothing, while recovering the
program counter through a microcode send costs a full dispatch on every step —
60C's own §7 example spends its first send doing exactly that.

**Amendment.** The driver invokes a StepMethod with a descriptor-declared
argument buffer of length 0 or 1. When the descriptor declares one, `args[0]` is
the resume token: a SmallInt program counter for the frame being entered.

This is not the rejected Option A of 60C §13.1. That option made `args` a
control array carrying an ephemeral lease — an object-shaped identity with no
Birnam protocol, borrowed from native state. A SmallInt program counter is an
ordinary Birnam value with an ordinary protocol, declared by an immutable
descriptor and validated like any other operand. The objection in §13.1 was to
smuggling native control identity through the argument buffer, not to the buffer
carrying a value.

### 9.2 Sealing

60C §15 Q4 is answered by §8: a per-selector seal in the method dictionary,
covering add, overwrite, removal, and shadowing through any vtable whose parent
chain reaches Machine's vtable.

### 9.3 Evidence item 8

60C §14's evidence list requires "generated C that uses no stackless-specific
native symbols". Generated C needs two: the entry shim a thunk calls, and the
registration shim of §4.3, since `birnam_method`'s signature cannot express a
ResumableEntry and widening it would break every existing artifact.

**Amendment.** Item 8 becomes "generated C that uses no stackless-specific IdApi
fields, and no stackless-specific native symbols other than the entry and
registration shims". The commitment worth keeping is that the independently
useful id floor does not grow; host runtime shims in the existing `birnam_*`
family are the same category as `birnam_method`, which generated code already
calls.

## 10. Tail calls across the seam

Neither 60A nor 60C says what a TailCall does when its target is not resumable,
and the answer is not benign.

A synchronous Method has no FrameDescriptor, so 60A §5's "reuses the current
frame storage when the new descriptor fits" has no meaning for it. In practice
it degenerates to Call-then-Return: the driver enters the barrier, runs the
Method to completion, writes the result, and pops. The current frame is retained
across the callee, which is not a tail call.

**Decision.** Proper tail behavior exists only for a TailCall whose resolved
implementation names a live ResumableEntry. A TailCall resolving to a
synchronous Method degrades to Call-plus-Return, and the drive counts it. 60A §5
is amended to scope its no-retention rule to a resumable target, since it cannot
hold for one that has no FrameDescriptor.

The counter is not decoration. 60 §7 proof point 2 and 60C §14 item 1 both state
their million-transition requirement unconditionally, but a mutually tail-
recursive cycle with one unmigrated member — or one that routes through
`ifTrue:ifFalse:`, which codegen emits as an ordinary send served by a Rust
primitive that sends `value` to the chosen block — grows the native stack per
iteration and overflows long before a million. Nothing else in the design
detects a partially migrated tail cycle.

The counter is per drive, not per Machine: §5.1 puts nested drives on one
Machine, so a per-Machine counter would let an unrelated crossing's degraded
tail call fail another drive's proof. The S1 probe publishes it and the S2 tail
proof asserts it is zero for the drive under test.

## 11. Cost and the budget 60C leaves unset

60C §14 defers a fast tier until measurements "fail an agreed budget", but no
budget is written anywhere, which makes the gate unfalsifiable. This record sets
a provisional one — and first disqualifies the obvious anchor.

**`sends.unary` is not the send constant.** `bench/micro.birnam` installs its
probe methods through `Reflect defineMethod:`, which installs a *trampoline*
rather than the method body: the emitted function does a `birnam_global` lookup
for the retained block — an intern through a linear symbol-table scan plus a
Globals read served by a reverse scan of the fact log — then interns `value:`
and performs a *second* send into the block. Every measured op is therefore two
sends plus the symbol and Store work, which is the reflective-defineMethod
idiom, not dispatch. On the Rust floor the case reports ~1400–1443 ns/op while
`blocks.value` reports ~91–96 ns/op.

The gap is not wholly attributable to the trampoline — an ordinary compiled send
also resolves free names through `birnam_global`, so `blocks.value` pays some of
the same cost and does not isolate it — but the doubled send and the extra
symbol/Store work are verified in the emitted trampoline, and that is enough to
disqualify the case as a dispatch anchor.

[46](46-benchmarks.md) calls `sends.unary` "the send constant itself" and pins a
release acceptance anchor to it. That reading is not disturbed for 46's purpose,
which is regression detection on a stable idiom; what this record rejects is
using it as the *denominator of a ratio budget* for an operation it does not
measure. A budget so denominated would be roughly fifteenfold too loose on the
Rust floor.

**No existing case measures what the budget needs, but a Birnam surface produces
it.** The two definition paths that install trampolines are `Reflect
defineMethod:on:as:`, whose trampoline does the global lookup and second send
described above, and artifact/image lowering, whose trampoline reads an indexed
candidate root and then sends. Neither is what `S` should measure.

`Tcc unit install:as:on:` is: it resolves a symbol from a relocated unit and
calls `ensurePrivateVtable` plus `api.method` directly, with no trampoline, no
Store MOP, and no second send. `Alien wrap:signature:as:on:` is built on it, and
`bench/micro.birnam`'s `ffi.wrapped-call` already times an ordinary unary send
to such a body — it differs from `S` only by the syscall inside it.

So S0 adds a micro case timing an ordinary unary send to a body installed
through `Tcc install:as:on:` with a trivial return. That keeps `S` inside the
same harness as the anchors below, which a one-off Rust benchmark would not.
`bench/micro.birnam` requires new cases to be appended at the end, because case
order is load-bearing for its store-backed cases. Call that constant `S`.
Budgets are multiples of it:

| Measure | Budget |
|---|---|
| Continue transition | ≤ 2 × S |
| Call/Return pair, migrated to migrated | ≤ 4 × S |
| Entry through a thunk | ≤ 6 × S |
| Microcode sends per source-level step | ≤ 4, reported per step kind |
| Maximum reachable depth through an unmigrated primitive | ≥ 0.5 × the pre-migration depth, reported with the measured native-stack capacity |

Absolute ceilings are deliberately absent until `S` exists. Fixing them against
today's numbers would set them so far above the multiples that only the
multiples ever bind, which is the appearance of a second guard rather than one.

Two anchors are recorded so a reader can sanity-check scale, both from the
Rust-floor snapshot `bench/results/v0.11.0-rust-2fe44d2-…json` that
[61A](61a-r6-performance-tradeoffs.md) accepts as evidence, run 1 of 3:
`blocks.value` 96 ns/op and `loops.to-do` 214 ns/op. Neither is `S`.
`bench/results/v0.11.0.json` must not be used: it predates the Rust floor.

The last row is the axis §5.3 says regresses, and it is stated as a budget
rather than a reporting requirement because a measurement nobody can fail is
what this section exists to correct. Its threshold is provisional in a stronger
sense than the others: §5.3's own accounting — driver, barrier, synchronous
method, `send_abi`, `bind_active`, thunk, nested driver replacing roughly four
activations — puts the design near the line before anything is measured, and the
0.5 figure is a placeholder to be set once `S` and the native-stack capacity are
both known. It belongs to S2, with the entry measure.

Its remedy is also different, and conflating the two would waste a stage.
Exceeding the first four budgets is the trigger 60C §13.3 reserves for an
optional fast tier. Exceeding the depth budget is **not**: §13.3 is the
borrowed-execution-window tier, which reduces microcode dispatch cost and
removes exactly zero native activations per crossing. The remedy for a depth
regression is S4 sequencing — migrating the callback-driving primitives so the
alternating case becomes the long-run case — as §5.3 says. The numbers are
provisional and may be
amended by an explicit record; they may not be quietly retired because a
measurement missed them.

## 12. Failure taxonomy additions

Extending 60A §9. None is an ordinary value:

- `resumableEntryUnregistered` — a **thunk** was invoked and no live entry names
  it. A *driver-side* registry miss is not a failure at all: it is how every
  synchronous Method is reached (60A §5 step 4), and §4.2's false-negative
  window is the only route by which a thunk can outlive its entry;
- `resumableEntryStaleGeneration` — the entry's generation was released;
- `sealedMicrocodeSelector` — add, overwrite, or removal of a sealed selector;
- `codeGenerationPinned` — disposal refused under the §7 gate;
- `resumableEntryUnrestored` — an image validity failure: a restored thunk has
  no live entry after boot re-registration (§4.3). Unlike the others this one is
  raised during image validation rather than at a thunk boundary.

Capture, suspension, Abort, and Restore that would cross an entry barrier all
use the existing `continuationBarrier`. No depth failure kind is introduced;
§5.3 explains why.

## 13. What this record freezes

Frozen:

- a vtable never holds a StepMethod address;
- a Call or TailCall resolving to a live ResumableEntry pushes a frame and never
  invokes a thunk; the thunk is reached only by ordinary `IdApi.send`;
- registration produces one ResumableEntry regardless of surface;
- recognition maps a resolved implementation — named by its defining vtable and
  selector, not by code address — to at most one live entry, and never aliases
  across unload;
- nested entry reuses the current Machine above an entry-barrier delimiter, and
  is chosen by barrier scope rather than by thread activity;
- a Machine has one drive epoch, advancing on drive entry and exit, and
  validation always compares against that single current value;
- Abort, Restore, and Capture terminate at the innermost entry barrier;
- a Return at a barrier frame ends the drive, not the Machine;
- frame tracing enumerates the arena rather than walking the caller chain;
- arena slot generations advance on pop, not only on chunk release;
- registration is an emitted artifact operation, and image boot re-registers
  before any send can reach a restored thunk;
- a driver failure reaching a thunk boundary is fatal, not a value;
- migration preserves result and error values for a matching argument count, and
  preserves neither mismatched-arity behavior nor maximum call depth;
- proper tail behavior requires a resumable target;
- microcode selectors are sealed against add, overwrite, removal, and shadowing
  through a private child vtable;
- an active frame captures its step address, descriptor, and generation pin at
  push, and does not consult the registry per step.

Not frozen:

- per-implementation emission versus a runtime thunk pool, and how the pool is
  sized;
- the entry shim's exact name;
- ResumableEntry layout, indexing, or registry data structure;
- whether recognition is a vtable-record side table, a tag word beside the
  dictionary entry, or another per-implementation mapping;
- arena chunk retention and trim policy;
- descriptor blob encoding;
- the §11 budget numbers, which are provisional by construction.

## 14. Acceptance tests

Each item is labelled with the stage that can first run it. S1 gates on the S1
items; the later ones gate their own stage.

Registration and entry are covered when tests demonstrate:

1. **(S1)** an ordinary send to a migrated selector answers the same Oop as
   before migration, for the same selector-determined argument count;
2. **(S1)** no vtable holds a StepMethod address, asserted by enumerating with
   `vtableCount` / `vtableAt` / `vtableSelectorAt` and resolving each selector
   through the floor's internal vtable lookup, since the enumeration answers
   selectors rather than addresses and the `lookup` primitive is not reachable
   from Birnam source. The enumeration must skip the floor's opaque internal
   dictionaries, whose values are exported Oops rather than method addresses;
3. **(S1)** a resumable method reached by ordinary send from unmigrated code
   completes and returns through the thunk without the caller observing a
   Machine;
4. **(S1)** a live frame keeps running after its selector is reflectively
   redefined and its entry unregistered, because the frame captured its step
   address and descriptor at push;
5. **(S1)** nested entry pushes onto the current Machine above an entry-barrier
   delimiter, and the Machine count does not grow with nesting depth;
6. **(S1)** a thunk reached with an active drive but no active barrier — a host
   or reactor callback — starts a fresh Machine rather than nesting;
7. **(S1)** the drive stack is a distinct borrow domain: nested entry does not
   panic or deadlock on a reentrant borrow;
8. **(S1)** an outer drive's StepAction is stale during a nested drive **and
   remains stale after it returns**, so a driver that re-read a rewritten
   scratch payload would fail rather than decode it;
9. **(S1)** a Return at the barrier frame ends the nested drive with the Machine
   still Running, and the outer drive's frames, chunks, and generation pins
   survive it;
10. **(S1)** forced collection during a nested drive preserves a value live only
    in an outer frame slot;
11. **(S1)** a FrameView taken on a nested frame is stale after that frame pops
    and an outer Call reuses its arena offset;
12. **(S1)** add, overwrite, removal, **and shadowing through a private child
    vtable** of a sealed microcode selector each fail with
    `sealedMicrocodeSelector`, leaving the original primitive installed;
13. **(S1)** every microcode selector is owned by Machine's own vtable record
    rather than inherited, asserted structurally by reflection, not by timing;
14. **(S1)** overwriting or removing a thunk unregisters its entry;
15. **(S1)** a driver failure raised inside `do:` is fatal rather than discarded
    by `vector_do`, and is not catchable by `on:do:`;
16. **(S1)** disposal of a generation that installed a method is refused with a
    kinded failure per §7's staging rule, while the author file, `.candidate`
    sidecar, and `state discarded` fact still happen;
17. **(S2)** a migrated-to-migrated Call is served by a frame push with no thunk
    invocation and no native-stack growth, asserted by a probe counter on the
    entry shim that must read zero;
18. **(S2)** the same holds when the implementation is **inherited** — a
    resumable method installed on a prototype and sent to a child resolves
    through its defining vtable to its entry, with the probe counter still zero;
19. **(S2)** a resumable **block body** registers and executes through the
    direct `birnam_method` surface, not only through the Store MOP;
20. **(S2)** live and AOT registration produce identical FrameDescriptor
    identity, declared arity, and implementation identity for the same source;
21. **(S2)** a TailCall to a synchronous Method increments the drive's
    `tailCallToSynchronous`, and the tail-recursion proof asserts it is zero for
    the drive under test;
22. **(S2)** an image saved with a migrated method boots through the mapped-code
    path, re-registers its restored thunks, and answers the same result as the
    same image booted through the rebuild path; an image whose restored thunk
    has no live entry answers `resumableEntryUnrestored` at the §4.3 check point
    rather than aborting on first send;
23. **(S3)** capture, suspension, Abort, and Restore that would cross an entry
    barrier each fail with `continuationBarrier`, and the outer drive resumes
    correctly afterward;
24. **(S3)** after `dlclose`, no registry entry names an implementation in the
    unloaded generation's mapped range;
25. **(S3)** disposal is refused with a kinded failure while a frame, a
    continuation, or a registered vtable holds the generation — the vtable case
    demonstrated with a stored block, and the refusal expected to be
    unconditional per §7. Pin correctness is evidenced separately, by observing
    pin counts across push, pop, capture, and consume, since the refusal alone
    would pass with pinning unimplemented;
26. the §11 measures are published whether or not they pass. S1 publishes the
    Continue and microcode-per-step measures using its hand-written StepMethod,
    against the `S` micro case S0 adds; the Call/Return, entry, and depth
    measures need two migrated implementations and a thunk, so they belong
    to S2.

## 15. Open questions

1. **Migration as a change event.** `lookup:` answers the thunk address, and
   `tracked_add_method` retracts the old carrier and asserts a new one on every
   install. Flipping a method's migration status therefore looks to the Store
   mirror — and to Design 49's reconciliation and rollback machinery — exactly
   like a method change, with a new address and a new fact, even when the source
   is byte-identical. What should `deriveImageMethodTargets` and
   `managedMethodChanged:on:` see?
2. **Arena trim.** Under §5.1 one Machine's arena high-water mark is a function
   of boundary crossings, which §5.3 says is unbounded for a recursion through
   an unmigrated primitive, and chunks are retained after return (60B §3). Does
   a long-lived Runtime need a trim policy, and are returned chunks zeroed?
3. **Descriptor blob home.** Should the FrameDescriptor live in the artifact as
   data, or be constructed at registration from emitted metadata? The thunk-pool
   option of §3.1 makes the data form more attractive, since it is the form a
   registrar without a code generator can supply.
4. **What shape should the durable resumable tag take?** §4.3 decides *that*
   the projection must be self-describing about which methods are resumable,
   since image boot cannot reconstruct it. It does not decide whether that is a
   fact on the existing method mirror, a section in the image, or part of the
   artifact descriptor — and the mirror never sees block vtables at all, so
   whatever is chosen must cover them by another route.
5. **Depth diagnostics.** §5.3 declines an artificial depth bound. Should the
   Machine nonetheless publish boundary-crossing depth so a native-stack
   exhaustion can be attributed to an alternating chain rather than reported as
   an opaque crash?
