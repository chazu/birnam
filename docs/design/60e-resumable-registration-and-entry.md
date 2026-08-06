# 60E — Resumable Registration and the Entry Seam

> **Status:** S0 decision record, accepted 2026-08-04; recognition, late-bound-
> microcode, same-execution-entry-scope, binding-lifetime/resolve-and-pin, and
> live-dispatch-timing amendments accepted 2026-08-05; Runtime ownership
> and code-disposition staging accepted 2026-08-06, together with the S1
> Machine-microcode protocol.
> This record closes the registration seam that
> [60A](60a-stackless-execution-semantics.md) §5 and
> [60C](60c-stepmethod-calling-abi-options.md) §4.2 both depend on and both
> defer. It decides what a vtable holds for a resumable implementation, how a
> send from unmigrated code reaches one, and how the driver recognizes one. It
> does not define compiler lowering, liveness, or source maps; those remain the
> later compiler record.

## 1. The gap this closes

60C freezes a StepMethod as an ordinary `Method` invoked with a Machine receiver
and answering a StepAction. 60A §5 then requires the driver to distinguish a
resumable target and push its Frame rather than synchronously invoking it.
Neither record says what performs registration, where the record lives, or what
an ordinary send does when it reaches a resumable selector.

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
| Driver path | A Call or TailCall uses one logical **resolve-and-pin** operation: ordinary lookup followed, under uninterrupted lifetime protection, by classification as a pinned ResumableEntry, a protected synchronous Method, or unbound. A resumable result pushes a frame and calls the StepMethod. **The thunk is not invoked.** |
| Send path | An ordinary `IdApi.send` from unmigrated code reaches the thunk, which drives the implementation and answers the ordinary result. |
| Nested entry | A thunk reached inside an inherited **same-execution entry scope** pushes onto the scoped Machine above an entry-barrier delimiter rather than allocating a second Machine. The driver establishes this scope around StepMethod and synchronous-Method invocation. Unrelated host/reactor work suppresses it and starts a fresh Machine. |
| Registration record | A Runtime-scoped, process-local **ResumableEntry** owns its unique thunk address, StepMethod address, FrameDescriptor, registration-generation handle, declared arity, and implementation identity. Resolve-and-pin acquires the separate call-generation pin transferred to a Frame. |
| Runtime authority | The active `birnam-id::Runtime` is the sole semantic authority for Machine state, Runtime-local entry recognition, binding retirement, dispatch-read protection, and generation pins. The registration shim targets the explicitly entered Runtime; TLS may route the call but is not the authority. |
| Code custody | Codegen/TCC or another process code repository owns physical mappings. It may share immutable code across isolated Runtimes, but disposal waits for every Runtime lease; it cannot unregister or unload code behind a Runtime's execution state. |
| Candidate discard | Logical candidate retirement and physical mapping disposition are separate outcomes. Logical discard may complete while the repository reports `Retained(codeGenerationPinned)`; this is not failure of the logical transition. |
| Recognition | The active Runtime maps each unique live **entry-thunk address** to at most one ResumableEntry. Lookup-through-recognition is protected as one logical operation and acquires the entry plus a generation pin before the protection ends. |
| Binding lifetime | Each direct installation owns one binding reference. Aliases own independent references; inheritance does not duplicate the defining reference. Lookup results are borrowed, and retaining a raw Method carrier requires an explicit generation lease. Retirement and address reuse wait for binding references, dispatch readers, acquired entries, Frames, Continuations, and leases to quiesce. |
| Preservation | Migration preserves result and error values for a matching argument count. It preserves neither mismatched-arity behavior (§6) nor maximum call depth (§5.3). |
| Failure surface | A 60A §9 driver failure reaching a thunk boundary is fatal, never an ordinary result value. |
| Registration surfaces | The Store MOP for named methods, direct `birnam_method` for block bodies, `Tcc unit install:as:on:` for symbols resolved from a relocated unit, and image boot, which restores installs and must re-register before any send reaches a restored thunk. All produce the same record. |
| Microcode | Machine microcode is an ordinary late-bound Birnam protocol. Its checked primitive implementations may be redefined, removed, or shadowed. A Birnam-authored replacement using public capabilities receives no raw Frame or arena pointer; incompatible changes may alter or invalidate execution, but the driver still validates the final StepAction before applying its requested control transition. Prior valid checked mutations are not transactional or rolled back. Trusted native artifacts retain native authority. |

## 3. Two paths, and only one of them is the thunk

This is the distinction the rest of the record depends on, so it is stated once,
explicitly:

| Caller | Resolution | Cost |
|---|---|---|
| Migrated: a StepMethod issuing a Call or TailCall | The driver resolves and pins the ordinary lookup result (60A §5), then pushes a resumable frame directly or invokes the protected synchronous result | One protected resolution plus one frame push for a resumable target. No thunk invocation, native frame, or Machine acquisition on that path. |
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
out either: inherited or custom metacircular lookup need not identify that
receiver vtable as the binding's home.

What preserves the no-code-generator path is a **pool of distinct runtime-owned
thunks**, one bound per registration. Each pooled address is unique, so it
identifies its entry without a selector, and none of them has to be compiled at
registration time. That is what lets a resumable implementation be registered
without running a code generator, which 60C §13.4 preserves for a future
non-native or sandboxed artifact format and which an interpreter tier would
need. Whether the pool is pre-built, grown, or exhausted is not decided here.

The uniqueness is also what lets the **driver** recognize the same resolved
address without invoking the thunk. Ordinary sends and driver calls therefore
share the complete metacircular lookup result; only what they do with that
result differs.

### 3.2 Thunk behavior

`birnam_enter_resumable` performs:

1. resolve the thunk's ResumableEntry in the active Runtime and acquire its live
   generation pin;
2. consult the Runtime entry-scope stack (§5.2) to decide fresh or nested entry;
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
      unique entry-thunk address  (what ordinary lookup answers)
      StepMethod address           (what the driver calls)
      FrameDescriptor identity     (60B §4)
      generation identity and registration handle  (60B §9)
      declared arity
      implementation identity      (for FrameView, source maps, and 49)

The registry belongs to one Runtime, is owner-thread-checked in the first
delivery, and is not a GC root set. Its recognition key is `(Runtime identity,
entry-thunk address)`, so the same mapped code may be registered independently
by future in-process Runtimes without sharing execution state. Entries are also
validated by index plus generation when held as capabilities, following the
Machine registry pattern of 60B §1.

The authoritative registry and its retirement/read-epoch state live with
dispatch in `birnam-id::Runtime`, not in the compiler or loader. Codegen's
process state is a Codegen context, not a second Birnam Runtime. The emitted
registration shim uses the explicitly entered Runtime and fails when none is
active. A process code repository may own the physical mapping and aggregate
leases from several isolated Runtimes; the Runtime retains the safety decision
about whether its entries, bindings, Frames, Continuations, or readers can still
reach that generation.

### 4.1 Resolve and pin is one logical operation

The driver must not hold a bare Method address across a lifetime gap. Its
logical `resolve-and-pin` operation begins before ordinary metacircular lookup
and ends only after the result has become one of:

- a strong ResumableEntry reference plus a call-generation pin;
- a synchronous Method whose code generation remains protected through its
  immediate invocation; or
- an unbound result that follows the existing DNU path.

This does not collapse the two paths of §3. A Machine driver classifies a
registered thunk and bypasses it. Ordinary `IdApi.send` retains the same
read-side lifetime protection through immediate invocation of the Method that
lookup returned; when that Method is a thunk, the thunk acquires its
ResumableEntry under the still-active protection and drives normally. No new
argument or ambient Machine is added to the Method ABI.

For a resumable result, a successful Frame push transfers the call-generation
pin to the Frame; a failed push drops it. The ResumableEntry retains its own
registration-generation handle until retirement completes. A synchronous
result needs the same lifetime principle because concurrent unload of an
ordinary Method between lookup and invocation would be the same use-after-free
without a ResumableEntry.

Resolution remains independent of how lookup found the address. An inherited
dictionary binding and a custom metacircular lookup that returns the same live
registered thunk both remain resumable. No second parent-chain walk,
defining-vtable reconstruction, or exception to ordinary late binding is
required. A custom lookup result is a borrowed dispatch result: retaining its
raw Method carrier beyond that resolution requires an explicit generation
lease. Native code that retains an unleased carrier already violates the same
code-lifetime contract for synchronous Methods.

Within one Runtime, a live thunk address names at most one ResumableEntry and
each entry owns a unique thunk address. A raw StepMethod address is never
installed or returned by lookup. An address never known as a live or retired
thunk receives the existing synchronous-Method interpretation. A known retired
thunk presented by a reader begun after retirement is a stale carrier failure,
not a synchronous miss; returning a raw StepMethod under the synchronous
interpretation is malformed native behavior, just as calling any Method address
under the wrong semantic contract is today.

The protection covers **lookup through classification and pin acquisition**;
atomic recognition relative to unregistration alone is insufficient. Otherwise
one executor can resolve the old thunk, a mutator can retire and recycle it,
and the executor can then misclassify or invoke the recycled address.

This is a semantic linearization contract, not a commitment to one lock. The
owner-thread S1 Runtime serializes physical access, but metacircular lookup may
re-enter mutation before returning. Its read protection may therefore be a
plain owner-thread epoch/depth token, amortized across the current Machine turn
or top-level send. The fast path must add no mutex, atomic read-modify-write,
heap allocation, or reference-count operation per ordinary resolution.
Registry classification remains a non-allocating lookup, and the minimal
generation handle is acquired only when a resumable Frame is pushed. S1 must
implement the API in this shape rather than scattering bare-address lookups, so
later protection does not require rewriting the driver.

Before shared-Runtime parallelism or concurrent code evolution is enabled, 60D
requires a real read-side protocol. An epoch/RCU-style token is preferred: it
may be amortized across a Machine turn or top-level host send, nested resolution
reuses it, and it is not a Rust borrow or lock held across metacircular code.
Immutable snapshots or another reviewed mechanism remain permitted. Mutation
may publish immediately, but reclamation waits for older readers to drain; the
lookup itself is never retried merely because an epoch changed, since custom
lookup may have effects.

### 4.2 Address recycling, in both directions

Code addresses are recyclable: `dlclose` runs in birnam-codegen-runtime's
`discardCandidate:` and `releaseCandidate:`, and a later `dlopen` may map new
code at the same address. Two directions follow, and only the first is a
registry problem.

**False positive.** A stale vtable may retain a thunk address after its registry
entry is removed. If a later generation reused that address and registered a
new entry, the driver would push a frame for the wrong implementation. Ordering
registry removal before `dlclose` is not sufficient: address reuse is forbidden
until the old generation has no registration handle, binding reference,
pre-retirement dispatch reader, acquired entry, Frame, Continuation, or explicit
generation lease that can still produce or invoke the address. The conservative
first delivery therefore refuses unload for every generation that installed a
method (§7).

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
`Tcc unit install:as:on:`, which resolves a symbol from a relocated unit and
calls
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
60A §9 failure at a thunk boundary. The rebuild path is unaffected, since it
re-evaluates method definitions through the Store MOP and recreates the
Runtime-qualified address mapping, so mapped and rebuilt images would differ in
correctness — two routes to the same image answering differently, which is the
kind of divergence 60 §3's compatibility invariants exist to prevent.

**Decision.** Registration is an emitted artifact operation, not a side effect
of installation. Artifact lowering emits a registration call beside each
install. The entry and its registration-generation handle exist before the
binding becomes visible; successful publication adds that direct binding's
reference, while rejected or failed installation rolls back any unpublished
registration. Image boot re-registers before any send can reach a restored
thunk; and a restored thunk with no live entry answers
`resumableEntryUnrestored` as an **image validity failure** rather than a runtime
fatal on first use.

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

**Decision.** When a thunk is reached inside a same-execution entry scope
established by an active drive, it does not allocate a second Machine. It pushes
the new frame onto that Machine's arena above an **entry-barrier delimiter**, and
runs a nested drive that may not pop below that delimiter. The driver establishes
the scope around both StepMethod and synchronous-Method invocation, because
either native activation may make an ordinary send and must remain live until
that send returns. A thunk reached with no inherited scope starts a fresh
Machine; §5.2 explains why the predicate is logical scope rather than thread
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

### 5.2 The entry-scope stack is the one piece of ambient state

Deciding fresh-versus-nested requires knowing whether the send belongs to a
native activation of the current logical drive, and the thunk holds no Machine
— that is the whole nature of the seam. The Runtime therefore owns an
entry-scope stack. The driver pushes a same-execution scope immediately before
invoking a StepMethod or synchronous Method and pops it when that activation
returns. A resumable microcode replacement reached by the StepMethod therefore
uses the same Machine and the same entry-barrier rules as a resumable send made
by an unmigrated synchronous Method.

The predicate is inherited logical scope, not thread activity, and the
difference is not pedantic. A Machine may be driving when the host or reactor
begins unrelated work — a fresh reactor item, Store reconciliation, codegen
authorization, or an unrelated FFI callback — and that work may reach a thunk by
ordinary dispatch. The host entry operation explicitly suppresses any inherited
same-execution scope. Nesting that work onto the driving Machine would give a
new logical execution no identity a scheduler could queue, no ability to
suspend, a lifetime tied to an unrelated Machine, and a backtrace interleaved
with one. A thunk reached with an active drive but no inherited scope starts a
fresh Machine, which is what 60D §3 wants when it lists several Machines on one
owner thread as a direct fit.

60D §4 forbids ambient execution identity, and that prohibition stands where it
was aimed: **generated StepMethod code and Machine microcode never consult the
entry-scope stack.** They receive Machine explicitly as their receiver. The
entry-scope stack is consulted only by the entry shim, at the one boundary where
no Machine is in hand, and 60D §4 is amended to say so rather than left to be
read as forbidding it.

Pushing and popping the dynamic scope are point operations. The scope record
remains installed while the native activation runs, but no entry-scope-stack,
registry, or MachineState borrow is held across generated code, `IdApi.send`,
allocation, collection, or a foreign callback, per 60D §6.2. The entry-scope
stack must be a distinct borrow domain from the machine registry, or the nested
case self-deadlocks on the second entry.

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

60B §9 forbids disposing code while a Frame or Continuation pins its generation.
The current paths do not implement that, and today's refusal is not observable
as a result: `discardCandidate:` and `releaseCandidate:` refuse only while a
candidate is active or prepared, call `note_error`, then return the unit itself.

S1 must introduce a structured physical-disposition outcome before it registers
the first resumable method. The code repository reports either `Disposed` or
`Retained(reason)`, with `codeGenerationPinned` as the conservative-retention
reason defined here. This process-local outcome is exposed through the Codegen
protocol or its discard receipt, not by expanding IdApi. The disposal condition
is wider than 60B §9 states, because of §4.2's second direction:

> Disposal is refused unless the generation has zero registration handles,
> direct binding references, pre-retirement dispatch readers, acquired
> ResumableEntries, live frames, live continuations, and explicit generation
> leases, and no reachable vtable holds a method address in its mapped range.

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

**Staging.** The gate ships with recognition in S1. Since reachable vtables are
not yet computable, S1 retains every generation that installed a method; a
generation that installed none may still answer `Disposed` once its ordinary
mapping leases drain. The conservative rule is stated in terms of reachability
so that making the registry collectable later narrows it rather than rewriting
it. S3 adds Continuations to the holder accounting. Future 60G may answer
`Disposed` for a formerly method-bearing generation only after it closes the
vtable-reachability problem and every Runtime has released its leases.

Logical discard is a separate transition. A retained mapping must not suppress
removal of the author file and `.candidate` sidecar or assertion of the
`state discarded` fact, which are Store state Design 49's reconciliation reads.
The discard result or receipt records both the successful logical transition
and its `Disposed` or `Retained(reason)` physical disposition. Expected
retention is not a Machine driver failure and must not make a self-hosted
compiler retry a logical discard that already completed.

S1 also exposes retained-generation count and retained mapped bytes. These are
required telemetry, not proof of pin correctness. Conservative retention is a
known leak per evolution cycle, and continuous self-hosted compilation is not a
production-ready mode until Future 60G closes reclamation or accepts and enforces
a bound.

**Evidence.** Because the refusal is unconditional in practice, disposal
behavior cannot serve as evidence that generation pinning works — a stubbed pin
implementation would produce an identical observation. S1 must observe the
minimal registration and Frame pin across resolve, push, failed push, pop, and
binding retirement. S3 adds direct evidence across capture and consume. These
are 60 §7 proof points 8 and 11 respectively.

For a future multithreaded Runtime, retirement and disposal are ordered against
§4.1 protected resolution. Mutation first detaches or replaces the binding and
retires its reference; reclamation then waits for read-side epochs, entry and
Frame pins, Continuations, binding references, and explicit leases. The choice
of snapshot, reference count, epoch, or RCU is not frozen, but an address may
not return to the recognition map for a new generation before the old
generation reaches that quiescent state. A process mapping shared by several
Runtimes needs process-shared generation lifetime accounting even though each
Runtime keeps an independent recognition map.

### 7.1 An active frame does not consult the registry

A frame captures its StepMethod address, FrameDescriptor identity, and
generation pin when it is pushed. The registry is consulted at Call and TailCall
resolution and at entry — never per step for a frame that already exists.

Without that rule, binding retirement would reach into live execution.
`Reflect defineMethod:` for a selector whose implementation is on the frame
stack would retire its binding, and a driver that re-resolved on the next step
would fail. Design 49's quiescence guard covers `CompiledUnit
activate`, not reflective redefinition, so the case is reachable. It would also
contradict 60A §8, 60B §9, and 60D §11, which all say an active frame continues
through its pinned implementation generation while new sends resolve late.

60C §8 step 2 is amended to match: resolve-and-pin is used at Call or TailCall
frame push, while a running Frame invokes its captured StepMethod without
consulting dispatch again.

### 7.2 Bindings retire entries; one overwrite does not unregister an alias

Nothing about generation unload covers a method being replaced, but removing
one binding cannot unregister a thunk that another selector, vtable, or explicit
lease can still produce. A unique thunk address identifies one implementation;
it does not imply that the implementation has only one binding.

Each direct installation therefore owns one binding reference to its
ResumableEntry. Installing the same thunk under another selector or vtable owns
another reference. Inheritance reuses the defining binding and adds no
reference. The authoritative `add_method` and `remove_method` mutation paths
already receive the previous Method address, so they increment the new direct
binding and retire only the replaced or removed direct binding. No defining-
vtable reconstruction, selector sealing, or change to lookup order is required.

A custom lookup result is borrowed for the protected resolution of §4.1. A
custom lookup or native cache that must retain the carrier independently of a
direct binding must hold an explicit generation lease. Without that lease, an
address returned after its final binding has retired is stale native behavior,
not an implicit permanent root.

When the last binding reference retires, the entry moves to a retirement set;
it is not immediately erased. Readers that began before retirement may still
convert the address into a pin. Reclamation and unregistration occur only after
those readers, acquired entries, Frames, Continuations, and explicit leases
have drained. A Runtime may then release the registration-generation handle,
and the mapping-generation owner may unload or recycle the address only after
the wider §7 gate also passes.

S1 has owner-thread serialization and an unload refusal for method-bearing
generations, so its binding counts and retirement set use ordinary mutable
state. It must nevertheless implement correct alias counts and the same state
transitions now. Turning every removed binding into an immortal entry would
hide the bug behind an unbounded code-generation leak; immediately erasing the
entry would preserve the alias and lookup race. Neither shortcut is accepted.

## 8. Keeping the microcode late-bound

60C §15 Q4 asks how primitive-backed Machine methods are identified so an
arbitrary resumable method cannot displace one. The accepted answer is that
they are not privileged bindings: Machine microcode is an ordinary Birnam
protocol and uses the same late-bound lookup as every other send.

The initial implementations are synchronous Rust primitives because private
Frame storage needs a small regress breaker. Each canonical primitive validates
the Machine capability, owner, lifecycle, drive epoch, descriptor, slot bounds,
and slot kind before touching that storage. No Birnam-authored replacement using
the public protocol receives a Frame pointer, arena pointer, or unchecked
storage capability merely by occupying the same selector. The outer driver
independently validates the canonical StepAction returned by the StepMethod
before changing Machine state.

These checks are local, not transactional. A Birnam-authored replacement may
perform a valid mutation through a checked primitive and then return a malformed
StepAction. The malformed action prevents the requested driver transition and
moves the Machine to Failed; it does not roll back that earlier valid mutation.
Transactional Frame journaling or copy-on-write is not part of S1.

Add, overwrite, removal, inherited replacement, a private-child shadow, and a
custom metacircular lookup all retain their ordinary meanings. Their effects
are visible at the next resolve-and-pin, including one issued by a Frame that
was already active across the mutation's turn boundary. The Frame's currently
captured StepMethod and descriptor are not rewritten. Under later concurrent
mutation, resolve-and-pin linearizes the send as observing either the old or new
binding. An incompatible change may alter the program, return an unexpected Oop, fall
through `doesNotUnderstand:`, or ultimately cause a typed driver failure. The
runtime promises storage safety under such a change; it does not promise that
an active computation remains semantically correct after its execution
protocol is incompatibly redefined.

This is the same malleability boundary as replacing any foundational protocol.
Native artifacts remain trusted native code and can already violate process
memory safety independently of method lookup; sealing selectors would not form
a security boundary. A future versioned or generation-pinned microcode protocol
may improve compatibility diagnostics, but S1 adds neither a mutation veto nor
an exception to ordinary late binding.

## 9. Amendments incorporated into 60C

These amendments were incorporated when 60C was accepted. They remain recorded
here because the entry seam supplies their rationale.

### 9.1 The driver argument count

60C freezes "the initial driver contract uses zero arguments" while its own §15
Q1 asks whether that is right. Zero is the wrong thing to freeze. The buffer
already exists in the frozen Method ABI and costs nothing, while recovering the
program counter through a microcode send costs a full dispatch on every step —
60C's own §7 example spends its first send doing exactly that.

**Amendment.** The driver invokes a StepMethod with a descriptor-declared
argument buffer of length 0 or 1. When the descriptor declares one, `args[0]` is
the resume token: a SmallInt program counter for the frame being entered.

A zero-argument descriptor is limited to a single-entry StepMethod that does not
inspect its current PC; every multi-PC StepMethod declares the resume token.

This is not the rejected Option A of 60C §13.1. That option made `args` a
control array carrying an ephemeral lease — an object-shaped identity with no
Birnam protocol, borrowed from native state. A SmallInt program counter is an
ordinary Birnam value with an ordinary protocol, declared by an immutable
descriptor and validated like any other operand. The objection in §13.1 was to
smuggling native control identity through the argument buffer, not to the buffer
carrying a value.

### 9.2 Late-bound microcode

60C §15 Q4 is answered by §8: Machine microcode remains an ordinary late-bound
protocol. Its checked primitive implementations own the initial private-storage
mechanism, while replacement remains normal Birnam behavior and the driver
retains the final StepAction validation boundary.

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

**Before S0 acceptance no existing case measured what the budget needs, but a
Birnam surface produces it.** The two definition paths that install trampolines
are `Reflect defineMethod:on:as:`, whose trampoline does the global lookup and
second send described above, and artifact/image lowering, whose trampoline
reads an indexed candidate root and then sends. Neither is what `S` should
measure.

`Tcc unit install:as:on:` is: it resolves a symbol from a relocated unit and
calls `ensurePrivateVtable` plus `api.method` directly, with no trampoline, no
Store MOP, and no second send. `Alien wrap:signature:as:on:` is built on it, and
`bench/micro.birnam`'s `ffi.wrapped-call` already times an ordinary unary send
to such a body — it differs from `S` only by the syscall inside it.

S0 adds `sends.compiled-unary`, a micro case timing an ordinary unary
send to a body installed through `Tcc unit install:as:on:` with a trivial
return. That keeps `S` inside the same harness as the anchors below, which a
one-off Rust benchmark would not.
`bench/micro.birnam` requires new cases to be appended at the end, because case
order is load-bearing for its store-backed cases, and its exit gate asserts a
fixed case count that the new case must bump. Call that constant `S`. Budgets
are multiples of it:

| Measure | Budget |
|---|---|
| Continue transition | ≤ 2 × S |
| Resolve-and-pin plus synchronous leaf invocation | ≤ 2 × S |
| Call/Return pair, migrated to migrated | ≤ 4 × S |
| Entry through a thunk | ≤ 6 × S |
| Microcode sends per source-level step | ≤ 4, reported per step kind |
| Maximum reachable depth through an unmigrated primitive | ≥ 0.5 × the pre-migration depth, reported with the measured native-stack capacity |

The S1 owner-thread resolve-and-pin path has structural performance gates in
addition to the ratio above: zero heap allocations, mutex acquisitions, atomic
read-modify-write operations, or reference-count operations per ordinary
resolution. Its read protection is a plain owner-thread epoch/depth token,
amortized over a Machine turn or top-level send; registry classification is a
non-allocating lookup. The benchmark must exercise the actual driver path to a
synchronous leaf, not a helper that bypasses metacircular lookup or
classification. A resumable hit separately reports registry probes and
generation-pin operations per Frame push.

Before concurrent code evolution is enabled, the concurrency follow-up adds
uncontended and contended read-epoch measurements: cost per Machine turn and
top-level host send, nested-token reuse, throughput with dispatch mutation, and
retirement-queue drain latency. It must prefer an amortized read token over a
mutex, global atomic increment, heap allocation, or `Arc` clone on every send.
Any implementation choosing a different mechanism must publish same-run
evidence that it meets or improves that shape.

The S0 acceptance run used an isolated ReleaseSafe Rust prefix and quick harness
mode and measured `S = 377 ns/op` on the 2026-08-04 development machine. That
number is scale evidence, not a durable release baseline: each S1/S2 budget is
evaluated against `sends.compiled-unary` from the same benchmark run so host and
layout noise divide out. Fixed absolute ceilings are deliberately absent.

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

- `resumableEntryUnregistered` — a **thunk** was invoked and no live or retiring
  entry names it. A driver-side registry miss is not a failure only when the
  address was never known as a thunk; that is how synchronous Methods are
  reached (60A §5 step 4);
- `resumableEntryRetired` — a known retired thunk was returned by a reader begun
  after its retirement epoch, usually an unleased native or custom-lookup
  cache. It is never reinterpreted as a synchronous Method;
- `resumableEntryStaleGeneration` — the entry's generation was released;
- `resumableEntryUnrestored` — an image validity failure: a restored thunk has
  no live entry after boot re-registration (§4.3). Unlike the others this one is
  raised during image validation rather than at a thunk boundary.

Capture, suspension, Abort, and Restore that would cross an entry barrier all
use the existing `continuationBarrier`. No depth failure kind is introduced;
§5.3 explains why.

`codeGenerationPinned` is instead the reason carried by the §7
`Retained(reason)` physical-disposition outcome. A low-level operation that
specifically requires immediate unmapping may treat it as a typed refusal, but
a completed logical candidate discard does not become a runtime failure.

## 13. What this record freezes

Frozen:

- a vtable never holds a StepMethod address;
- a Call or TailCall resolving to a live ResumableEntry pushes a frame and never
  invokes a thunk; the thunk is reached only by ordinary `IdApi.send`;
- registration produces one ResumableEntry regardless of surface;
- one `birnam-id::Runtime` owns semantic execution state and Runtime-local entry
  registration; compiler and loader state never own Machines or recognition;
- an emitted registration operation targets the explicitly entered Runtime and
  is artifact ABI rather than an IdApi field;
- a process code repository may own and share immutable mappings, but disposal
  waits for every isolated Runtime's leases and Runtime-local execution pins;
- logical candidate discard is independent of physical mapping disposition;
  S1 reports `Disposed` or `Retained(reason)`, conservatively retains every
  method-bearing generation, and exposes retained-generation count and bytes;
- S3 adds Continuations to the S1 holder accounting, while Future 60G owns
  actual reclamation of formerly method-bearing mappings;
- lookup through classification and pin acquisition is one logical protected
  `resolve-and-pin` operation; it returns a pinned ResumableEntry, a lifetime-
  protected synchronous Method, or unbound, never a bare address whose code may
  retire before use;
- recognition is independent of inherited or custom metacircular lookup, and a
  registry miss retains the synchronous-Method interpretation;
- each direct binding owns a binding reference; aliases own independent
  references, inheritance does not duplicate one, and retaining a borrowed
  custom-lookup result requires an explicit generation lease;
- retirement detaches only the affected binding and delays unregistration and
  reclamation until older dispatch readers, binding references, acquired
  entries, Frames, Continuations, and explicit leases are quiescent;
- a thunk address cannot be reused until that complete quiescence condition
  holds across every Runtime sharing the mapped generation;
- nested entry reuses the scoped Machine above an entry-barrier delimiter, and
  is chosen by inherited same-execution entry scope rather than by thread
  activity; the driver installs that scope around StepMethod and synchronous-
  Method invocation, while unrelated host/reactor work suppresses it;
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
- Machine microcode remains ordinarily late-bound and unsealed. A Birnam-
  authored replacement using public capabilities receives no raw Frame or arena
  pointer, and incompatible replacement cannot bypass final driver validation;
  prior valid checked primitive mutations are not rolled back. Trusted native
  artifacts retain their existing native authority;
- an active frame captures its step address, descriptor, and generation pin at
  push, and does not consult the registry per step.

Not frozen:

- per-implementation emission versus a runtime thunk pool, and how the pool is
  sized;
- the entry shim's exact name;
- ResumableEntry layout, indexing, or registry data structure;
- the synchronization mechanism a future multithreaded Runtime uses to provide
  protected resolve-and-pin, except that it may not hold a lock or Rust borrow
  across metacircular lookup and must meet §11's performance evidence;
- arena chunk retention and trim policy;
- descriptor blob encoding;
- the §11 budget numbers, which are provisional by construction.

## 14. Acceptance tests

Each item is labelled with the stage that can first run it. S1 gates on the S1
items; the later ones gate their own stage.

Registration and entry are covered when tests demonstrate:

Microcode memory-safety evidence stays at its owning boundaries rather than
being duplicated here: 60B §12 test 8 covers checked primitive validation before
each primitive mutation, and 60A §10 test 9 covers malformed final-action
failure before the requested driver transition. Neither test promises rollback
of an earlier valid checked mutation. Item 12 below proves only the late-binding
integration seam.

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
   redefined and its binding retired, because the frame captured its step
   address, descriptor, and generation pin at push;
5. **(S1)** nested entry from both a synchronous Method and a resumable
   microcode replacement pushes onto the scoped Machine above an entry-barrier
   delimiter; the Machine count does not grow, the waiting outer Frame survives,
   and the nested work cannot suspend or capture across the delimiter;
6. **(S1)** a thunk reached by unrelated host or reactor work with an active
   drive but a suppressed entry scope starts a fresh Machine rather than
   nesting;
7. **(S1)** the entry-scope stack is a distinct borrow domain: nested entry does
   not panic or deadlock on a reentrant borrow;
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
12. **(S1)** add, overwrite, removal, inherited replacement, private-child
    shadowing, and custom lookup of a microcode selector retain ordinary late-
    binding behavior. Mutation occurs after a StepMethod returns while its Frame
    remains active; that Frame's next resolve-and-pin observes the new binding,
    while its captured StepMethod and descriptor remain unchanged;
13. **(S1)** installing one thunk under two direct bindings gives it two binding
    references; overwriting or removing one retires only that reference and the
    other binding remains resumable. After the final reference retires, a
    deterministic pause between lookup and classification still resolves to an
    acquired entry plus directly observed generation pin, never a synchronous
    misclassification; failed Frame push releases that pin. Runtime-local
    registries may independently recognize the same mapped thunk address without
    sharing entry state. A custom-lookup cache retaining the carrier with an
    explicit lease remains valid through binding retirement; the same cache
    without a lease receives `resumableEntryRetired`, never synchronous
    invocation;
14. **(S1)** a driver failure raised inside `do:` is fatal rather than discarded
    by `vector_do`, and is not catchable by `on:do:`;
15. **(S1)** discarding a generation that installed no method completes with a
    `Disposed` physical disposition once its leases drain. Discarding one that
    installed a method completes the author-file, `.candidate`-sidecar, and
    `state discarded` logical transitions but reports
    `Retained(codeGenerationPinned)`. Retained-generation count and bytes rise
    observably. Registration and Frame pin counts are observed directly across
    resolve, push, failed push, pop, and binding retirement rather than inferred
    from conservative retention;
16. **(S2)** a migrated-to-migrated Call is served by a frame push with no thunk
    invocation and no native-stack growth, asserted by a probe counter on the
    entry shim that must read zero;
17. **(S2)** the same holds when the implementation is **inherited** and when a
    custom metacircular lookup returns its registered thunk address, with the
    probe counter still zero in both cases;
18. **(S2)** a resumable **block body** registers and executes through the
    direct `birnam_method` surface, not only through the Store MOP;
19. **(S2)** live and AOT registration produce identical FrameDescriptor
    identity, declared arity, and implementation identity for the same source;
20. **(S2)** a TailCall to a synchronous Method increments the drive's
    `tailCallToSynchronous`, and the tail-recursion proof asserts it is zero for
    the drive under test;
21. **(S2)** an image saved with a migrated method boots through the mapped-code
    path, re-registers its restored thunks, and answers the same result as the
    same image booted through the rebuild path; an image whose restored thunk
    has no live entry answers `resumableEntryUnrestored` at the §4.3 check point
    rather than aborting on first send;
22. **(S3)** capture, suspension, Abort, and Restore that would cross an entry
    barrier each fail with `continuationBarrier`, and the outer drive resumes
    correctly afterward;
23. **(60G)** after actual reclamation of a method-bearing mapping, no Runtime
    registry or reachable vtable names an implementation in the unloaded
    generation's mapped range;
24. **(S3)** a live Continuation contributes a directly observable generation
    pin and the repository reports `Retained(codeGenerationPinned)` while it is
    held. Pin counts are observed across push, pop, capture, consume, and
    finalization; conservative method-generation retention alone is not accepted
    as evidence that Continuation pinning works;
25. the §11 measures are published whether or not they pass. S1 publishes the
    Continue, synchronous resolve-and-pin, and microcode-per-step measures using
    its hand-written StepMethod, against the `sends.compiled-unary` value `S`,
    and asserts the zero-allocation/lock/atomic/refcount structural fast path.
    The Call/Return, entry, and depth measures need two migrated implementations
    and a thunk, so they belong to S2. Concurrent evolution later publishes the
    read-epoch and mutation-contention measures before it can be accepted.

## 15. Open questions

1. **Migration as a change event.** lookup answers the thunk address, and
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
