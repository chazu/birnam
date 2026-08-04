# 60E — Resumable Registration and the Entry Seam

> **Status:** S0 decision record, proposed for acceptance, 2026-08-04.
> This record closes the registration seam that
> [60A](60a-stackless-execution-semantics.md) §5 and
> [60C](60c-stepmethod-calling-abi-options.md) §4.2 both depend on and both
> defer. It decides how a resumable implementation is installed, how an ordinary
> send reaches it, and how the driver recognizes it. It does not define compiler
> lowering, liveness, or source maps; those remain the later compiler record.

## 1. The gap this closes

60C freezes a StepMethod as an ordinary `Method` invoked with a Machine receiver
and answering a StepAction. 60A §5.3 then says the driver pushes a resumable
frame "if the address is registered as resumable". Neither record says what
performs that registration, where the record lives, or what an ordinary send
does when it reaches a resumable selector.

That omission is not cosmetic. The floor installs methods as bare addresses:
`birnam_install_method` (`rust/birnam-codegen-runtime/src/lib.rs:662`) reaches
the Store MOP `addMethod:on:with:`, which transmutes the carrier and calls
`api.method` (`rust/birnam-store/src/lib.rs:502`). Lookup answers that address
and `send_abi` calls it immediately (`rust/birnam-id/src/dispatch.rs:57`).
Nothing on that path can consult a registration record.

So if a StepMethod address were installed in a vtable under its own selector,
every ordinary send to that selector — from unmigrated Rust methods, the REPL,
`modules/*.birnam`, or reflection — would invoke it with a non-Machine receiver
and a nonzero argument count. Per 60C §4.1 the entry answers an AbortAction, and
**the caller receives that AbortAction as its ordinary result value.** A wrong
answer, not a failure. This record removes that possibility by construction.

## 2. S0 decisions

| Area | Decision |
|---|---|
| Vtable contents | A vtable never holds a StepMethod address. It holds an ordinary Method: either a synchronous implementation or a generated **entry thunk**. |
| Entry thunk | One thunk per resumable implementation. It obtains a Machine, drives the implementation to completion, and answers the ordinary result Oop. |
| Registration record | A process-local **ResumableEntry** owns the StepMethod address, FrameDescriptor, generation pin, and declared arity. It is keyed by thunk identity, never by a raw code address alone. |
| Driver recognition | The driver tests the resolved address against the resumable registry. A hit pushes a frame and calls the StepMethod; a miss enters the synchronous barrier of 60A §7. |
| Transparency | Migrating a method from synchronous to resumable changes no observable result, error value, or arity behavior of an ordinary send. |
| Registration surfaces | Named methods register through the Store MOP; block methods register through the direct `birnam_method` path. Both produce the same ResumableEntry. |
| Microcode | Machine microcode selectors are sealed against overwrite and resolve in Machine's own vtable record. |
| Unloading | Code disposal is gated on zero live pins, which is also what makes address-keyed recognition sound. |

## 3. The entry thunk

### 3.1 Shape

The thunk is an ordinary Method and carries no new ABI:

```c
static Oop example_entry(Oop receiver, const Oop *arguments, size_t count) {
    return birnam_enter_resumable(EXAMPLE_ENTRY_INDEX, receiver, arguments, count);
}
```

`birnam_enter_resumable` is a host-provided runtime symbol in the same family as
`birnam_method` (`rust/birnam-tcc/src/lib.rs:815`) and `birnam_install_method`.
It is not an IdApi field, so the 60C commitment that the id floor does not grow
for stackless execution is preserved.

`EXAMPLE_ENTRY_INDEX` is a compile-time constant naming the ResumableEntry
created when the module registered the implementation. The thunk therefore needs
no lookup of its own and no static mutable state.

### 3.2 Behavior

`birnam_enter_resumable` performs:

1. resolve the ResumableEntry and validate its generation is still live;
2. check `count` against the entry's declared arity, answering the ordinary
   `doesNotUnderstand` result on mismatch so arity behavior is unchanged;
3. acquire a Machine for this entry (§5);
4. push the initial frame with the receiver and arguments copied into traced
   frame slots;
5. drive to completion;
6. answer the completed value as an ordinary Oop, or the runtime failure value
   defined by 60A §9 if the drive fails.

Step 6 is the transparency rule: a completed drive is indistinguishable from a
synchronous return. A caller that never mentions Machine never sees one.

### 3.3 What the thunk is not

The thunk is not a suspension point. A drive entered through a thunk must
complete before the thunk returns, exactly as 60A §5 requires of any synchronous
activation. Suspension reaches the host only through the explicit S3 operation
driving a Machine directly, never by unwinding through a thunk.

## 4. The ResumableEntry record

    ResumableEntry
      entry thunk address          (what a vtable holds)
      StepMethod address           (what the driver calls)
      FrameDescriptor identity     (60B §4)
      generation identity and pin  (60B §9)
      declared arity
      implementation identity      (for FrameView and source maps)

The registry is process-local, owner-thread-checked, and is **not** a GC root
set. It follows the Machine registry pattern of 60B §1: entries are validated by
index plus generation, and releasing an index advances its generation.

### 4.1 Keying and the address-reuse hazard

The driver holds a resolved address and must decide whether it is resumable.
Keying that decision on a raw code address is only sound if an address cannot be
recycled while a record still names it. Code addresses *are* recyclable here:
`dlclose` runs at `rust/birnam-codegen-runtime/src/lib.rs:6605` and `:6656`, and
a subsequent `dlopen` may map new code at the same address.

The rule is therefore ordered, not incidental:

1. a ResumableEntry is unregistered before its generation's code is unloaded;
2. unloading is refused while any frame or continuation pins that generation
   (§7);
3. therefore no address in the registry can belong to unloaded code, and an
   address hit cannot alias a different implementation.

An implementation that keys recognition differently — a side table on the vtable
record, a tag word beside the method address — is permitted, but it must satisfy
the same ordering. The hazard is the aliasing, not the data structure.

## 5. Entering a Machine from synchronous code

60A §5 requires that a synchronous Method's send may enter resumable methods and
must drive them to completion. Every entry thunk is that case. This is the path
every unmigrated caller takes during S2 and S4, so its cost is a migration
property, not an implementation detail.

**Decision.** Each Runtime owns a free list of entry Machines. A drive that
completes returns its Machine to the free list with its arena capacity retained
(60B §3). A drive that suspends does not: ownership passes to the suspension
handle, and the free list allocates a replacement.

Nested entry is legal and bounded. A synchronous Method invoked from inside a
drive may itself reach a thunk, which acquires a *different* Machine from the
free list; the two Machines are separate logical executions on one owner thread,
and the inner one is a capture barrier for the outer per 60A §7. The runtime
counts nesting depth and fails with `resumableEntryDepthExceeded` rather than
growing the native stack without bound — native depth still grows one
synchronous activation per nesting level, which is precisely the cost the
stackless path removes only for *migrated* calls.

### 5.1 The migration cliff this exposes

While `whileTrue:`, `to:do:`, and the collection callbacks remain synchronous
Rust methods (`rust/birnam-prelude/src/lib.rs`), a resumable block body invoked
per iteration enters through a thunk on every iteration. Machine acquisition is
therefore on the hot path of every unmigrated loop, which is the concrete reason
60 §6 orders S4 as it does.

S2 must measure entry cost per iteration and publish it. If pooled entry cannot
reach the budget of §8, that is evidence to move S4's control primitives earlier,
not evidence to weaken the barrier.

## 6. Two registration surfaces, one record

Named methods and block methods reach the vtable by different paths today, and
both must produce a ResumableEntry.

| Surface | Path today | Resumable form |
|---|---|---|
| Named method | `birnam_install_method` → Store MOP `addMethod:on:with:` → `api.method` | registers the entry, installs the **thunk** through the same MOP |
| Block body | `birnam_method` emitted inline into the block's lazily built private vtable (`rust/birnam-codegen/src/lib.rs:1171`) | registers the entry, installs the **thunk** through the same direct call |

Block registration is the one that is easy to miss: block vtables are built by
generated code at first evaluation, cached in a module-local static, and never
pass through the Store. A resumable block whose thunk is not registered would
fail as an unregistered address rather than as a compile error, so S2 must cover
a block body explicitly and not infer coverage from a named-method test.

The Store mirror asserted by `tracked_add_method` records the address actually
installed — the thunk. That is correct and intended: the durable projection
should name the vtable's real entry point, and the StepMethod address is
process-local like every other execution capability under 60B §11.

## 7. Generation pinning has to reach the unload path

60B §9 forbids disposing code while a frame or continuation pins its generation.
The current disposal paths do not implement that: `discardCandidate:` and
`releaseCandidate:` refuse only while a candidate is `active` or `prepared`
(`rust/birnam-codegen-runtime/src/lib.rs:6580`–`6667`) and then call `dlclose`.

S3 must add a pin count to that refusal, with the same kinded failure the other
refusals use. §4.1 depends on this ordering for recognition soundness, so it is
not solely an S3 memory-safety concern.

## 8. Cost, and the budget 60C leaves unset

60C §14 defers a fast tier until "measurements fail an agreed budget", but no
budget is written anywhere, which makes the gate unfalsifiable. This record sets
a provisional one.

The anchor is the recorded send constant. `bench/results/v0.11.0.json` reports
`sends.unary` at 2078 ns/op in-suite, and `bench/micro.birnam:26` documents the
standalone range as ~1.5–1.8 µs. That constant is large because dispatch scans
selector vectors linearly per vtable record (`rust/birnam-id/src/vtable.rs:45`)
and `bind_active` performs a metacircular `lookup:` send before the call
(`rust/birnam-id/src/dispatch.rs:23`), so the cost tracks the receiver's chain
length and the loaded-module footprint.

Two consequences follow.

**Microcode placement is load-bearing.** Machine microcode selectors must
resolve in Machine's own vtable record, ahead of any long inherited chain. A
microcode protocol that resolves through Object would make every step cost scale
with image size. This is a constraint on the S1 implementation, not a later
optimization.

**Provisional budget.** Against the send constant `S` recorded for the machine
under test:

| Measure | Budget |
|---|---|
| One Continue transition | ≤ 1 × S |
| One resumable Call/Return pair | ≤ 3 × S |
| Entry through a thunk, pooled Machine | ≤ 2 × S |
| Microcode sends per source-level step | ≤ 4, reported per step kind |

Exceeding these is the trigger 60C §13.3 reserves for an optional fast tier. The
budget is provisional and may be amended by an explicit record; it may not be
quietly retired because a measurement missed it.

## 9. Two amendments to the 60C proposal

### 9.1 60C §11: the driver argument count

60C freezes "the initial driver contract uses zero arguments" while its own §15
Q1 still asks whether that is right. Zero is the wrong thing to freeze. The
argument buffer already exists in the frozen Method ABI and costs nothing, while
recovering the program counter through a microcode send costs a full send —
against §8's constant, that is the single most expensive avoidable operation in
a step.

**Amendment.** The driver invokes a StepMethod with a descriptor-declared
argument buffer of length 0 or 1. When the descriptor declares one, `args[0]` is
the resume token: a SmallInt program counter for the frame being entered. The
entry still validates its receiver and count, and a descriptor mismatch remains
a malformed-artifact failure.

This changes no function shape and no loader behavior. It keeps the door open
for a descriptor to declare more operands later without another ABI record.

### 9.2 60C §15 Q4: sealing microcode

Q4 asks how primitive-backed Machine methods are identified so the compiler
cannot mistake an arbitrary resumable send for microcode. The runtime already
permits overwriting any method (`AddMethod::Overwrote`,
`setMethodOverwriteHook`), so today a Birnam program could install a method over
`currentPc` and the driver would execute arbitrary code inside a step — breaking
the regress path 60C §6 depends on.

**Amendment.** Microcode selectors are installed on Machine's own private vtable
(`ensurePrivateVtable`) and marked sealed. The existing overwrite hook rejects
replacement with a kinded `sealedMicrocodeSelector` failure. Malleability
elsewhere on Machine — inspection, policy, drive protocol — is unaffected: the
seal covers only the irreducible frame and action operations named in 60B §1.

This is a deliberate, checked, and small malleability boundary, which 60C §6
already argues is the honest place to put it.

## 10. Failure taxonomy additions

Extending 60A §9:

- `resumableEntryUnregistered` — a resolved address names no live entry;
- `resumableEntryStaleGeneration` — the entry's generation was released;
- `resumableEntryArityMismatch` — thunk arity check failed before Machine
  acquisition;
- `resumableEntryDepthExceeded` — nested entry exceeded the configured bound;
- `sealedMicrocodeSelector` — overwrite of a sealed Machine microcode selector;
- `codeGenerationPinned` — code disposal refused while frames or continuations
  pin the generation.

## 11. What this record freezes

Frozen:

- a vtable never holds a StepMethod address;
- every resumable implementation is reached through an ordinary entry thunk;
- registration produces one ResumableEntry regardless of surface;
- recognition may not alias a recycled code address;
- entry through a thunk always drives to completion;
- migration to resumable is observationally transparent to ordinary senders;
- microcode selectors are sealed and resolve in Machine's own record;
- the driver argument buffer is descriptor-declared, length 0 or 1.

Not frozen:

- thunk emission strategy or the runtime symbol's exact name;
- ResumableEntry layout, indexing, or registry data structure;
- entry-Machine pool size, growth, or nesting bound;
- descriptor blob encoding;
- whether recognition is an address table, a vtable-record side table, or a tag
  word;
- the budget numbers in §8, which are provisional by construction.

## 12. Acceptance tests

Registration and entry are ready for S1 when tests cover:

1. an ordinary send to a migrated selector answers the same Oop as before
   migration, with the same arity and `doesNotUnderstand` behavior;
2. a StepMethod address is absent from every vtable, asserted by reflection over
   the installed dictionary;
3. an entry thunk invoked with the wrong argument count answers the ordinary
   `doesNotUnderstand` result rather than an AbortAction;
4. a resumable method reached from an unmigrated Rust method completes and
   returns through the thunk without the caller observing a Machine;
5. nested entry from inside a drive acquires a distinct Machine and is a capture
   barrier for the outer drive;
6. entry Machines are returned to the pool on completion and not on suspension;
7. the nesting bound fails deterministically instead of exhausting the native
   stack;
8. a resumable **block body** registers and executes through the direct
   `birnam_method` surface, not only through the Store MOP;
9. unregistering precedes unload: no registry entry survives its generation's
   `dlclose`, and disposal is refused while a pin is held;
10. an address recycled by a later `dlopen` is never recognized as the earlier
    implementation;
11. overwriting a sealed microcode selector fails with `sealedMicrocodeSelector`
    and leaves the original primitive installed;
12. microcode selectors resolve in Machine's own vtable record, demonstrated by a
    lookup cost that does not grow when unrelated modules are loaded;
13. live and AOT registration produce identical ResumableEntry contents for the
    same source, per 60 §S2's parity requirement;
14. the §8 measures are published by the S1 probe whether or not they pass.

## 13. Open questions

1. **Thunk identity in reflection.** `lookup:` answers the thunk address, so a
   program comparing method identity across a migration sees a change. Is that
   acceptable, or should reflection answer an implementation identity that is
   stable across the synchronous/resumable transition?
2. **Pool sizing.** Is a per-Runtime free list correct, or should entry Machines
   be per-call-site to keep arena capacity warm for hot loops?
3. **Descriptor blob home.** The FrameDescriptor is immutable per generation.
   Should it live in the artifact as data, or be constructed at registration
   from emitted metadata?
4. **Arity check placement.** §3.2 checks arity in the thunk. Should the check
   instead be part of frame push, so the driver's CallAction path and the entry
   path share one implementation?
5. **Does the Store mirror need the resumable tag?** §6 records the thunk in the
   durable projection. A tag would make a durable image self-describing about
   which methods were resumable, at the cost of a projection change.
