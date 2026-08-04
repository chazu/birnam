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
| Driver path | A Call or TailCall whose resolved address names a live ResumableEntry pushes a frame and calls the StepMethod. **The thunk is not invoked.** |
| Send path | An ordinary `IdApi.send` from unmigrated code reaches the thunk, which drives the implementation and answers the ordinary result. |
| Nested entry | A thunk reached while a drive is already active on this owner thread pushes onto the **current Machine** above an entry-barrier delimiter. It does not allocate a second Machine. |
| Registration record | A process-local **ResumableEntry** owns the StepMethod address, FrameDescriptor, generation pin, declared arity, and implementation identity. |
| Recognition | Stated as a requirement, not a mechanism: a resolved implementation maps to at most one live entry, and recognition never aliases an address across unload. |
| Preservation | Migration preserves result value, error values, and selector-determined arity. It does not preserve maximum call depth; §5.3 states what changes. |
| Failure surface | A 60A §9 driver failure reaching a thunk boundary is fatal, never an ordinary result value. |
| Registration surfaces | Named methods register through the Store MOP; block methods register through the direct `birnam_method` path. Both produce the same record. |
| Microcode | Microcode selectors are sealed in the method dictionary against add, overwrite, and removal alike. |

## 3. Two paths, and only one of them is the thunk

This is the distinction the rest of the record depends on, so it is stated once,
explicitly:

| Caller | Resolution | Cost |
|---|---|---|
| Migrated: a StepMethod issuing a Call or TailCall | The driver resolves the address (60A §5.2), finds a live ResumableEntry, and pushes a frame directly | One frame push. No thunk, no native frame, no Machine acquisition. |
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
produce. It is **not** required. A single shared thunk that recovers its entry
from the receiver's vtable and the active selector satisfies §4 equally, and
that option must remain available: it is what lets a resumable implementation be
registered without running a code generator, which 60C §13.4 preserves for a
future non-native or sandboxed artifact format and which an interpreter tier
would need.

### 3.2 Thunk behavior

`birnam_enter_resumable` performs:

1. resolve the ResumableEntry and validate its generation is still live;
2. consult the Runtime drive stack (§5.1) to decide fresh or nested entry;
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
hazard in a new place, and the floor would swallow them. `vector_do` in
birnam-collection discards its block's result entirely, so a driver failure
raised inside `do:` would vanish and the loop would continue; `block_while_true`
and `integer_to_do` in birnam-prelude do check for errors and would abort. The
same runtime fault would be fatal in `whileTrue:`, fatal in `to:do:`, and
invisible in `do:`. Worse, because this language has no non-local return and
errors are ordinary values, `on:do:` would happily catch a driver failure and
continue running against a Machine the driver has just declared invalid.

**Decision.** A 60A §9 failure that reaches a thunk boundary is a fatal
invariant, reported through the same host path `send_abi` already uses when
`doesNotUnderstand:` is unbound. It is not returned. If a later record needs a
recoverable subset, it must name those kinds explicitly and give them a kind
that `kindMatches:` will not match against `#error`, and it must amend 60A §9,
which today says none of its kinds are in-band.

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

> Recognition must map a resolved implementation to at most one live
> ResumableEntry, and must never associate an address with an entry from a
> different generation.

An address-keyed table, a side table on the vtable record, and a tag word all
satisfy this. What none of them may do is outlive the code they name.

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

## 5. Entering from unmigrated code

60A §5 requires that a synchronous Method's send may enter resumable methods and
must drive them to completion. Every thunk invocation is that case, and it is
the path every unmigrated caller takes throughout S2 and S4.

### 5.1 Nested entry reuses the current Machine

**Decision.** When a thunk is reached while a drive is already active on this
owner thread, it does not allocate a second Machine. It pushes the new frame
onto the current Machine's arena above an **entry-barrier delimiter**, and runs
a nested drive that may not pop below that delimiter. When the nested frames
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

Reusing the current Machine also makes three properties fall out of mechanisms
60B and 60A already define rather than being asserted separately:

- the entry-barrier delimiter is a delimiter record (60B §8), so capture above
  it fails with the existing `continuationBarrier` — no new kind, and 60A §7's
  requirement that the barrier be dynamically scoped and visible to capture
  validation is satisfied by construction;
- suspension requested above an entry barrier fails the same way, which is what
  60A §5 already requires of a drive entered from a synchronous activation;
- one logical execution remains one Machine with one contiguous frame chain, so
  depth is read off that chain rather than from a side counter.

### 5.2 The drive stack is the one piece of ambient state

Deciding fresh-versus-nested requires knowing whether a drive is active, and the
thunk holds no Machine — that is the whole nature of the seam. The Runtime
therefore owns a drive stack naming the active drive and its epoch.

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

Each drive invocation records its own epoch. Action validation compares against
the epoch of the innermost active drive, so an action issued by an outer drive
is correctly stale while a nested drive runs, and the outer epoch becomes
current again when the nested drive returns.

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

Two consequences. First, no artificial depth bound is introduced: adding one
would convert a program that answers today into one that answers an error, and
the OS stack limit remains the bound it is today. Second, this reorders the
argument for S4: the callback-driving primitives are not a convenience
migration, they are what converts the alternating case into the long-run case.
S2 must measure and publish both axes — per-iteration entry cost in a loop, and
maximum reachable depth for a recursion through an unmigrated primitive —
because the second is the one that regresses.

## 6. Arity

The floor has no per-method arity. `add_method` stores selector and address
only; `doesNotUnderstand` fires exactly when lookup misses; `send_abi`'s only
argument check is the null-with-nonzero-count invariant. Arity is carried by the
selector spelling, so an ordinary compiled send cannot mismatch. Where a
mismatch does arrive — a native caller, or DNU forwarding — a codegen'd method
today ignores the count and a Rust method answers nil.

**Decision.** The arity check happens at frame push, shared by the driver path
and the entry path, and it is variadic-tolerant: bind `min(count, arity)` slots
and nil-fill the remainder, matching what the floor does today. It does not
fabricate a `doesNotUnderstand` result.

`doesNotUnderstand:` itself is not migratable in S1–S4. `send_abi` forwards an
unbound send to it with `1 + n` arguments, so a resumable `doesNotUnderstand:`
declaring arity 1 would mismatch on every keyword miss; answering DNU on
mismatch would then resolve to the same implementation and recurse. The floor
already fixed this shape of bug once — a `count != 1` guard in `object_dnu` that
made every keyword and binary send answer nil instead of erroring — and the
entry seam must not reintroduce it.

## 7. The unload gate

60B §9 forbids disposing code while a frame or continuation pins its generation.
The current paths do not implement that, and today's refusal is not even a
failure: `discardCandidate:` and `releaseCandidate:` refuse only while a
candidate is active or prepared, and on refusal call `note_error` — which stashes
a byte string in a mutex — then return the unit itself, indistinguishable from
success.

S3 must therefore *introduce* a kinded failure here, not reuse one. Its
condition must be wider than 60B §9 states, because of §4.2's second direction:

> Disposal is refused unless the generation has zero live frames, zero live
> continuations, and no vtable in the heap holds a method address in its mapped
> range.

The third clause is checkable with the `vtableCount` / `vtableAt` /
`vtableSelectorAt` enumeration the Store already uses to backfill its mirror. It
is what covers block vtables, which hold addresses no frame and no continuation
pins.

### 7.1 Entries are released on overwrite and removal, not only on unload

Nothing about generation unload covers a method being replaced. `add_method`
overwriting a thunk, and `removeMethod:on:` removing one, must both unregister
the entry. Without that, a REPL redefining one method accumulates a live entry
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

**The Store method-mutation authorizer can refuse, but does not see everything.**
`tracked_add_method` consults an authorizer and returns its answer, which is a
genuine veto — but it is also single-slot and already taken by codegen-runtime,
and it guards only the Store MOP path. The direct `birnam_method` path bypasses
Store entirely, which is exactly the path block vtables use.

**Private vtable placement is not protection either.** `Reflect
defineMethod:on:as:` calls `ensurePrivateVtable` on its target too and resolves
to the same record, so it overwrites in place.

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
covering add, overwrite, and removal.

### 9.3 Evidence item 8

60C §14's evidence list requires "generated C that uses no stackless-specific
native symbols". The entry thunk calls one: `birnam_enter_resumable`.

**Amendment.** Item 8 becomes "generated C that uses no stackless-specific IdApi
fields, and no stackless-specific native symbol other than the entry shim". The
commitment worth keeping is that the independently useful id floor does not grow;
a host runtime shim in the existing `birnam_*` family is the same category as
`birnam_method`, which generated code already calls.

## 10. Tail calls across the seam

Neither 60A nor 60C says what a TailCall does when its target is not resumable,
and the answer is not benign.

A synchronous Method has no FrameDescriptor, so 60A §5's "reuses the current
frame storage when the new descriptor fits" has no meaning for it. In practice
it degenerates to Call-then-Return: the driver enters the barrier, runs the
Method to completion, writes the result, and pops. The current frame is retained
across the callee, which is not a tail call.

**Decision.** Proper tail behavior exists only for a TailCall whose resolved
address names a live ResumableEntry. A TailCall resolving to a synchronous
Method degrades to Call-plus-Return, and the Machine counts it.

The counter is not decoration. 60 §7 proof point 2 and 60C §14.1 both state
their million-transition requirement unconditionally, but a mutually tail-
recursive cycle with one unmigrated member — or one that routes through
`ifTrue:ifFalse:` — grows the native stack per iteration and overflows long
before a million. Nothing else in the design detects a partially migrated tail
cycle. The S1 probe must publish `tailCallToSynchronous`, and the S2 tail proof
must assert it is zero for the cycle under test.

## 11. Cost and the budget 60C leaves unset

60C §14 defers a fast tier until measurements "fail an agreed budget", but no
budget is written anywhere, which makes the gate unfalsifiable. This record sets
a provisional one — and first disqualifies the obvious anchor.

**`sends.unary` is not the send constant.** It reports 2078 ns/op in
`bench/results/v0.11.0.json`, but `bench/micro.birnam` installs its probe
methods through `Reflect defineMethod:`, which installs a *trampoline*: each
measured op interns through a linear symbol-table scan, reads a global through a
reverse scan of the entire fact log, and then sends into a block. It measures
the reflective-trampoline idiom, not dispatch. Two checks confirm it:
`blocks.value` costs 79 ns/op through the same `send_abi` path, and
`sends.dnu-mint` at 1888 ns/op is *faster* than `sends.unary` despite walking
the chain to the root, missing, and allocating an error — impossible if chain
scanning dominated. A budget denominated in that number would be roughly 26×
too loose, and it drifts with loaded-module footprint, so it can be passed by
loading more modules.

**Budgets are stated against the operation each one replaces**, with an absolute
ceiling so the gate cannot be met by regressing the denominator:

| Measure | Compared against | Budget |
|---|---|---|
| Continue transition | `blocks.value` (79 ns at v0.11.0) | ≤ 2× and ≤ 200 ns |
| Call/Return pair, migrated to migrated | `blocks.value` | ≤ 4× and ≤ 400 ns |
| Entry through a thunk | `loops.to-do` per iteration (247 ns) | ≤ 2× and ≤ 500 ns |
| Microcode sends per source-level step | — | ≤ 4, reported per step kind |

S0 must add a micro case measuring an ordinary send to a compiled method with no
reflective trampoline, since no existing case is that measurement; `blocks.value`
is the closest proxy until it exists. Exceeding a budget is the trigger 60C
§13.3 reserves for an optional fast tier. The numbers are provisional and may be
amended by an explicit record; they may not be quietly retired because a
measurement missed them.

## 12. Failure taxonomy additions

Extending 60A §9. All are out-of-band under §3.3:

- `resumableEntryUnregistered` — a resolved address names no live entry;
- `resumableEntryStaleGeneration` — the entry's generation was released;
- `sealedMicrocodeSelector` — add, overwrite, or removal of a sealed selector;
- `codeGenerationPinned` — disposal refused under the §7 gate.

Capture and suspension above an entry barrier use the existing
`continuationBarrier`. No depth failure kind is introduced; §5.3 explains why.

## 13. What this record freezes

Frozen:

- a vtable never holds a StepMethod address;
- a Call or TailCall resolving to a live ResumableEntry pushes a frame and never
  invokes a thunk; the thunk is reached only by ordinary `IdApi.send`;
- registration produces one ResumableEntry regardless of surface;
- recognition maps a resolved implementation to at most one live entry and never
  aliases across unload;
- nested entry reuses the current Machine above an entry-barrier delimiter;
- a driver failure reaching a thunk boundary is fatal, not a value;
- migration preserves result, error values, and selector-determined arity, and
  does not preserve maximum call depth;
- proper tail behavior requires a resumable target;
- microcode selectors are sealed against add, overwrite, and removal.

Not frozen:

- per-implementation versus shared thunk emission;
- the entry shim's exact name;
- ResumableEntry layout, indexing, or registry data structure;
- whether recognition is an address table, a vtable-record side table, or a tag
  word;
- arena chunk retention and trim policy;
- descriptor blob encoding;
- the §11 budget numbers, which are provisional by construction.

## 14. Acceptance tests

Registration and entry are ready for S1 when tests cover:

1. an ordinary send to a migrated selector answers the same Oop as before
   migration, for the same selector-determined argument count;
2. no vtable holds a StepMethod address, asserted by enumerating the installed
   dictionary through `vtableCount` / `vtableAt` / `vtableSelectorAt`;
3. a migrated-to-migrated Call is served by a frame push with no thunk
   invocation and no native-stack growth, asserted by a probe counter on the
   entry shim that must read zero;
4. a resumable method reached by ordinary send from unmigrated code completes
   and returns through the thunk without the caller observing a Machine;
5. nested entry pushes onto the current Machine above an entry-barrier
   delimiter, and the Machine count does not grow with nesting depth;
6. capture and suspension above an entry barrier fail with
   `continuationBarrier`, and the outer drive resumes correctly afterward;
7. an outer drive's StepAction is stale while a nested drive runs, and the outer
   epoch is current again after it returns;
8. the drive stack is a distinct borrow domain: nested entry does not panic or
   deadlock on a reentrant borrow;
9. a resumable **block body** registers and executes through the direct
   `birnam_method` surface, not only through the Store MOP;
10. after `dlclose`, no registry entry names an address in the unloaded
    generation's mapped range;
11. disposal is refused, with a kinded failure, while a frame, a continuation,
    or a heap vtable holds the generation — the vtable case demonstrated with a
    stored block;
12. overwriting or removing a thunk unregisters its entry;
13. add, overwrite, and removal of a sealed microcode selector each fail with
    `sealedMicrocodeSelector`, leaving the original primitive installed;
14. every microcode selector is owned by Machine's own vtable record rather than
    inherited, asserted structurally by reflection, not by timing;
15. live and AOT registration produce identical FrameDescriptor identity,
    declared arity, and implementation identity for the same source;
16. a driver failure raised inside `do:` is fatal rather than discarded by
    `vector_do`, and is not catchable by `on:do:`;
17. a TailCall to a synchronous Method increments `tailCallToSynchronous`, and
    the S2 tail-recursion proof asserts the counter is zero;
18. the §11 measures are published whether or not they pass. The entry and
    Call/Return measures require a thunk, so they belong to S2; S1 publishes the
    Continue and microcode-per-step measures using its hand-written StepMethod.

## 15. Open questions

1. **Migration as a change event.** `lookup:` answers the thunk address, and
   `tracked_add_method` retracts the old carrier and asserts a new one on every
   install. Flipping a method's migration status therefore looks to the Store
   mirror — and to Design 49's reconciliation and rollback machinery — exactly
   like a method change, with a new address and a new fact, even when the source
   is byte-identical. What should `deriveImageMethodTargets` and
   `managedMethodChanged:on:` see?
2. **Arena trim.** A deep alternating recursion grows one Machine's arena to its
   high-water mark. Chunks are retained after return (60B §3). Does a long-lived
   Runtime need a trim policy, and are returned chunks zeroed?
3. **Descriptor blob home.** Should the FrameDescriptor live in the artifact as
   data, or be constructed at registration from emitted metadata? The shared-thunk
   option of §3.1 makes the data form more attractive.
4. **Does the Store mirror need the resumable tag?** The mirror records the
   thunk, which is correct for named methods and never sees block vtables at
   all. A tag would make a durable image self-describing about which methods
   were resumable, at the cost of a projection change.
5. **Depth diagnostics.** §5.3 declines an artificial depth bound. Should the
   Machine nonetheless publish boundary-crossing depth so a native-stack
   exhaustion can be attributed to an alternating chain rather than reported as
   an opaque crash?
