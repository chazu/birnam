# 74A — E0: legality of guarded Machine slot access

> **Status: proof delivered and accepted by the user 2026-09-09; E1 was then
> authorized and delivered as [Design 74B](74b-e1-guarded-slot-access-implementation.md),
> which amends §2.3/§2.5 (see 74B §5). E2's remaining rows and E3 remain
> unauthorized.**

## 0. Question and answer

Design 74 asks whether generated StepMethods may replace the late-bound
`oopAt:` and `oopAt:put:` sends to the Machine with a guarded direct access,
while preserving every current semantic and lifetime contract. The governing
rule is [Design 60F §4](60f-resumable-compiler-lowering.md): machine microcode
remains late-bound, and an optimizer may remove those sends only if it proves
the same mutation, lookup, failure, and reflection behavior under the ordinary
Birnam protocol.

**Answer: the optimization is legal without narrowing any current contract,
under the guard set in §3.** The direct path must call the same Runtime
functions the canonical Methods call (§1), so every slot, owner, lifecycle,
capability, and root check is kept; the guard removes only the metacircular
resolution and the send boundary that precede that call. The proof found two
documentation/code discrepancies that do not affect legality (§6) and one
residual class of untracked mutation that the baseline already tolerates (§2.7).

Everything below cites the checkout at `f24ff9b2` (tagged v0.15.0).

## 1. The canonical operations and their complete path

### 1.1 What generated code emits

[`resumable_c.rs`](../../rust/birnam-codegen/src/resumable_c.rs) `emit_get`
and `emit_store_prefix`/`emit_store_suffix` produce, for every logical slot
read and write:

```c
birnam_send(machine, birnam_intern("oopAt:"), (void*[]){MKV(3L)}, 1L)

void* __birnam_put_selector_7 = birnam_intern("oopAt:put:");
void* __birnam_put_slot_7 = MKV(5L);
void* __birnam_value_7 = <value expression>;
void* __birnam_put_args_7[] = {__birnam_put_slot_7, __birnam_value_7};
(void)birnam_send(machine, __birnam_put_selector_7, __birnam_put_args_7, 2L);
```

`machine` is the StepMethod's receiver argument: the driver
([`run_stackless_loop`](../../rust/birnam-id/src/runtime.rs), the loop that
copies `step_method`, `program_counter` and `driver_argument_count` out of the
current Frame's descriptor and calls `method(machine, arguments, count)` inside
`enter_stackless_scope`) always passes the Machine Oop that owns the current
Frame. `MKV` is `birnam_mkSmallInt`, which allocates a boxed Integer through
`make_integer`; `birnam_intern` resolves an existing Symbol without allocating.

### 1.2 The send path, in order

For `birnam_send(machine, oopAt:, …)` the floor performs
([`dispatch.rs`](../../rust/birnam-id/src/dispatch.rs) `send_abi`,
[`runtime.rs`](../../rust/birnam-id/src/runtime.rs) `resolve_stackless_target`,
`bind_active`):

1. `enter_dispatch_read` (dispatch-read token; protects a dictionary-proven
   Method for the invocation extent).
2. `resolve_stackless_target`, which takes a second dispatch-read token and
   calls `bind_active`:
   1. `receiver_vtable = Vtables::dispatch_vtable_of(machine)` — the vtable
      word in the Machine object's header.
   2. If `receiver_vtable == vtable_vt && selector == lookup`, use the floor's
      `lookup_method_active` directly. Otherwise **send `lookup` to
      `receiver_vtable`** with the selector as argument. That inner send
      resolves through `dispatch_vtable_of(receiver_vtable)` (the vtable's own
      vtable, `vtable_vt` unless the vtable has been given a private vtable)
      and normally reaches the floor primitive, which walks the vtable's
      dictionary and parent chain (`Vtables::lookup`).
   3. The answer is decoded as a `MethodAddress`.
3. `classify_resumable(method)`: a hash lookup in `resumable_by_thunk`. The
   canonical `machine_oop_at` is not a registered resumable thunk, so the
   target is `StacklessTarget::Synchronous(method)`.
4. `invoke_resolved_send_target` calls `method(receiver, args, count)` under
   the outer dispatch-read token.
5. If step 2 answered no method: DNU redispatch (`doesNotUnderstand:`).

### 1.3 The canonical Methods

[`execution.rs`](../../rust/birnam-id/src/execution.rs) `machine_oop_at` and
`machine_oop_at_put` are static floor functions installed on the canonical
Machine vtable by `install_execution_objects` (`runtime.rs`, the
`(oop_at, machine_oop_at)` / `(oop_at_put, machine_oop_at_put)` pairs). Each:

1. runs inside `with_active` (the owner-thread active Runtime);
2. answers **nil** on `count != 1` (or `2`), a null argument buffer, or a
   non-Integer / negative index (`nonnegative_index`);
3. calls `Runtime::stackless_oop_at(receiver, index)` /
   `stackless_oop_at_put(receiver, index, value)`, mapping **every** `Err` to
   nil — no failure kind, no abort.

`stackless_oop_at` performs:

- `stackless_machine_capability(machine)` → `capability_for_object`: the
  execution objects are installed; the receiver is a heap object whose payload
  word 0 is `MACHINE_CAPABILITY_KIND_V1`; words 1–2 decode to a registry
  `(index, generation)`; the registry slot is live for the owner and its
  `machine` Oop is this receiver (identity, so a copied payload is rejected).
- `read_current_oop(owner, capability, index)`: the Machine record for the
  owner; `require_driving` (owner matches, lifecycle `Running`, `drive_depth
  != 0`); the arena's current Frame handle (else `StaleHandle`); then
  `Arena::read_oop`: `require_live` (arena not destroyed, handle index in
  range, arena identity and Frame generation match, slot live), descriptor
  `slot_kind(index)` must be `Some(Oop)` (`None` → `InvalidSlot`, `Raw` →
  `WrongSlotKind`), and the word is returned with exposed provenance.
- `write_current_oop` is the same checks, then stores the address and answers
  the stored value.

Neither function allocates, sends, or invokes a callback. No Rust borrow
outlives the call. This is the complete required check set; §3 keeps all of it.

## 2. Everything that can invalidate equivalence

Equivalence means: the direct path answers exactly what the send would have
answered, performs exactly the effects the send would have performed, and no
observer can distinguish the two except through Runtime-private dispatch
counters (Design 74 §7.4 permits that difference).

| # | Source of divergence | Mechanism today | Guard consequence |
| --- | --- | --- | --- |
| 2.1 | Machine header vtable differs from the canonical Machine vtable | `ensure_private_vtable(machine)` (used by `stepmethod_sends_every_microcode_message_with_a_resumable_replacement`) gives one Machine a private vtable whose parent is the canonical one; `heap.set_vtable` from image restore or any floor path | Read the header word per access; require identity with the canonical Machine vtable. A private vtable is ineligible even if it holds no override (conservative, no parent walk). |
| 2.2 | Canonical Machine vtable's direct entry for `oopAt:` / `oopAt:put:` replaced or removed | `Vtables::add_method` (Overwrote), `remove_method`, image-restore rebuild, generation retirement replacing a binding | Verify both entries are the floor's `machine_oop_at` / `machine_oop_at_put` addresses whenever the mutation epoch changed (§3). |
| 2.3 | Custom metacircular lookup on the Machine vtable | The vtable object given a non-canonical metavtable (its `dispatch_vtable_of` ≠ `vtable_vt`) carrying a `lookup` override. *Amended by 74B §5: `ensure_private_vtable` on a vtable whose metavtable is already `vtable_vt` answers the vtable itself, so this can arise only through bootstrap or image code.* | Require `dispatch_vtable_of(canonical Machine vtable) == vtable_vt`. |
| 2.4 | `lookup` redefined on `vtable_vt` (global custom lookup) | `add_method(vtable_vt, lookup, …)`; the `bind_active` fast path only bypasses the send when `receiver_vtable == vtable_vt`, so the Machine send would run the override | Require `vtable_vt`'s direct `lookup` entry to be the floor primitive. |
| 2.5 | `vtable_vt` itself given a non-canonical metavtable | Bootstrap or image code only (see the 2.3 amendment) | Covered by 2.4's verification reading through `dispatch_vtable_of(vtable_vt)`; require it to be `vtable_vt`. |
| 2.6 | Parent-chain edits | `set_parent` on the canonical Machine vtable | Irrelevant while 2.2 holds (direct entries are found before any parent walk); bump the epoch anyway for simplicity. |
| 2.7 | Raw memory writes | `word_vt` `poke` (Design 41 §F4) can rewrite a vtable header or dictionary word | Untracked in the baseline too: it can already corrupt ordinary dispatch. The header read in 2.1 is re-done per access; dictionary poke is accepted residual risk identical to today's, stated in the eligibility rules rather than hidden. |
| 2.8 | Custom lookup that returns the same Method but logs or mutates | Same as 2.3/2.4 | Ineligible by construction: the guard never inspects the *result* of a custom lookup; any non-primitive lookup fails 2.3/2.4 and the ordinary send runs (Design 74 §6.2). |
| 2.9 | GC and finalizers between accesses | `MKV` → `make_integer` → `maybe_collect_before_allocation`; collection runs `Finalizing` with allocation forbidden (`require_allocation_allowed` aborts) but a native finalizer may call IdApi `addMethod`/`removeMethod` | The guard is re-evaluated **after** operand boxing and immediately before the access (§3.3); a finalizer's mutation bumps the epoch. |
| 2.10 | Mutations inside the same StepMethod between two accesses | The StepMethod's own `callSite:` / `continueAt:` sends and any `birnam_send` it makes; a private vtable install by the driver-invoked callee happens only across a Call boundary, which ends the StepMethod invocation | Per-access guard (§3.3), never per-StepMethod. |
| 2.11 | Machine state changes: not driving, wrong owner, stale capability, popped Frame, wrong slot kind, index out of range | Checked by `stackless_oop_at` (§1.3) and answered as nil | Unchanged: the direct path calls the same function and maps `Err` → nil identically. |
| 2.12 | Pending layout-v2 transition | `begin_v2_transition` sets `pending_edge`; `Arena::read_oop`/`write_oop` do **not** consult it (§6.1) | Unchanged: same function, same behavior. If a pending check is later added to the canonical functions, the direct path inherits it automatically because it calls them. |
| 2.13 | Code lifetime | The canonical Methods are static floor functions; `classify_resumable` never sees them as resumable; no generation pin is involved | The direct path caches no Method address in generated code and holds no lease; nothing to retire. |
| 2.14 | Nested drive / re-entrant `enter` | `with_active` and `enter` accept re-entry for the same Runtime on its owner thread | Same functions; no new re-entry path. |
| 2.15 | Foreign thread | `assert_owner` in `stackless_machine_capability` | Same check. |
| 2.16 | DNU | Only reachable if 2.2 fails | Ineligible; ordinary send performs the redispatch. |
| 2.17 | Reflection / FrameView / counters | FrameView reads the same arena storage; dispatch counters (`enter_dispatch_read` depth, send counts) differ on a hit | Storage identical; counter difference is the permitted diagnostic difference (Design 74 §7.4). |

## 3. Guard: dependencies, lifetime, and where it repeats

### 3.1 Eligibility predicate

`E(machine)` holds when all of the following are true:

1. `dispatch_vtable_of(machine) == machine_vt` (the canonical Machine vtable
   recorded in the execution objects);
2. `machine_vt`'s **direct** dictionary entry for `oopAt:` is `machine_oop_at`
   and for `oopAt:put:` is `machine_oop_at_put`;
3. `dispatch_vtable_of(machine_vt) == vtable_vt`;
4. `dispatch_vtable_of(vtable_vt) == vtable_vt` and `vtable_vt`'s direct
   `lookup` entry is the floor lookup primitive.

Conditions 2–4 depend only on dictionary and header state of two permanent
vtables. Condition 1 depends on one heap object's header.

### 3.2 Making it O(1): a dictionary-mutation epoch

Add a Runtime-private monotonic `u64` `dispatch_mutation_epoch`, incremented
on every `Vtables::add_method`, `remove_method`, `add_weak_opaque_binding`,
`remove_dead_weak_bindings` (when it removed anything), `set_parent`,
`create`/`create_collectable` of a vtable whose parent is `machine_vt` or
`vtable_vt` (not needed for correctness but cheap), `heap.set_vtable` on a
vtable object (private vtable of a vtable), and on image restore. Conservative
over-bumping is acceptable (Design 74 §6.2); missing a site is not. The
authoritative list is the set of `pub(crate)` mutators in
[`vtable.rs`](../../rust/birnam-id/src/vtable.rs) plus `Heap::set_vtable`; a
unit test should assert that every mutator bumps the epoch.

The Runtime caches `(verified_epoch, verified: bool)` for conditions 2–4.
Per access the guard is: load `dispatch_mutation_epoch`; if it differs from
`verified_epoch`, re-evaluate 2–4 by direct dictionary reads (private,
non-allocating, non-effectful: `direct_lookup` on two permanent vtables and
two header loads) and store the result; then evaluate condition 1 (one header
load and compare). Wrap-around of a `u64` counter is not reachable; if it were,
saturating at `u64::MAX` and treating it as "always re-verify" fails safe.

### 3.3 Where the guard repeats

Per slot access, after operand boxing and immediately before the direct
access. Not per StepMethod invocation, not per basic block: between two slot
accesses in one StepMethod there can be an `MKV` allocation (a collection with
native finalizers) and other microcode sends. The guard is two loads and two
compares on the hit path, so hoisting is not worth its proof burden in E1.

### 3.4 Lifetime rules

- The guard reads no method address into generated code and creates no
  reference that outlives the access; `dispatch_mutation_epoch` and the cached
  verification live in `RuntimeState`.
- The direct path executes inside the same `with_active`/owner-thread extent
  as the canonical Method would. It does not need a dispatch-read token
  because it invokes a static floor function, not a dictionary-proven Method;
  nothing can be retired under it. (E1 may keep the token for symmetry; it
  changes nothing semantically.)
- Frame and Machine roots are unchanged: the direct path reads through the
  capability registry exactly as today and never holds a Frame pointer across
  generated code (Design 60B §1 and §9 are untouched).
- Epoch and cached verification are process-local, disposable state: never
  serialized into artifacts or images (Design 74 §6.4).

## 4. Fallback boundary

On a guard miss, generated code performs the ordinary send with the operands it
has already produced, exactly once:

```text
box = MKV(slot)                     ; allocation happens before the guard, as today
if guard hits:  value = direct_oop_at(machine, slot)      ; §1.3 checks, Err → nil
else:           value = birnam_send(machine, oopAt:, {box})
```

Retained across the boundary: the Machine Oop (receiver argument), the boxed
slot index (rooted by being a C local only for the duration of the call, as the
current emission already does), and for a store the already-evaluated value.
No argument is re-evaluated, no lookup is run twice, no effect is repeated. The
miss path is byte-for-byte the current emission, so its failure behavior is the
baseline's. The hit path returns nil in exactly the cases the canonical Method
returns nil, because it is the canonical Method's body minus dispatch.

The store form keeps the current publication order: `MKV(slot)` and the value
expression are evaluated before either the guard or the send, matching
`emit_store_prefix`'s "prepare every potentially allocating operand before the
rooted send" rule.

## 5. Code-generation sketch

Two private, development-only exports of the floor dylib, alongside the
existing `birnam_send`/`birnam_intern`/`birnam_mkSmallInt` symbols generated
code already binds to:

```c
/* Returns the slot value, or BIRNAM_INELIGIBLE (a distinguished non-Oop
   sentinel owned by the floor) when the guard misses. Never allocates. */
void* birnam_dev_frame_oop_at(void* machine, void* boxed_index);
void* birnam_dev_frame_oop_at_put(void* machine, void* boxed_index, void* value);
```

Emission for a read (E1 candidate; the baseline artifact is unchanged):

```c
void* __birnam_get_box_9 = MKV(3L);
void* __birnam_get_9 = birnam_dev_frame_oop_at(machine, __birnam_get_box_9);
if (__birnam_get_9 == BIRNAM_INELIGIBLE)
  __birnam_get_9 = birnam_send(machine, birnam_intern("oopAt:"),
                               (void*[]){__birnam_get_box_9}, 1L);
```

Inside the floor:

```text
birnam_dev_frame_oop_at(machine, box):
  with_active(runtime):
    if !runtime.guard_machine_slot_access(machine): return INELIGIBLE   ; §3.1–3.3
    index = nonnegative_index(box) else nil                             ; same decode as §1.3
    return runtime.stackless_oop_at(machine, index).unwrap_or(nil)      ; same checks as §1.3
```

Properties: no persistent raw Frame pointer (the floor resolves the Frame from
the capability every time); no borrow across a send (the function returns
before generated code continues); the boxed index is created once and shared
by both paths, so allocation, collection points, and rooting are identical to
the baseline; the sentinel never escapes into a slot because the miss branch
always overwrites it. The put form answers the stored value on both paths.

Artifact discipline (Design 74 §7.1): the candidate is a development-only
artifact selected explicitly (for example an environment-gated emitter switch
in `resumable_c.rs` that the production build never enables), never published
or admitted as an existing production artifact kind. The two exports are
private carriers in the sense of [`birnam-abi/src/private.rs`](../../rust/birnam-abi/src/private.rs):
not part of the frozen IdApi struct, not persisted.

## 6. Findings that do not affect legality but need separate review

### 6.1 Pending-transition reads and writes

[Design 60B §1](60b-stackless-machine-and-frames.md) and
[Design 60F §10.3](60f-resumable-compiler-lowering.md) state that checked Frame
mutation while a layout-v2 transition is pending fails with
`frameTransitionPending`. In the checkout, `Arena::read_oop` and `write_oop`
do not consult `pending_edge`; only `begin_v2_transition` rejects a second
pending transition, and the symbol `frameTransitionPending` does not occur in
`rust/birnam-id/src`. The direct path inherits whichever behavior the canonical
functions have, so equivalence holds either way; the discrepancy itself is a
documentation-or-implementation question for the 60B/60F owners.

### 6.2 Selector interning per access

`birnam_intern("oopAt:")` is evaluated on every access. Design 60F §5 permits
preparing selector interning earlier when it has no Birnam-visible effect;
`oopAt:` always exists, so hoisting is legal. It is a separate micro-change and
is not part of the E1 candidate, which must isolate dispatch savings.

## 7. Verdict and what E1 may now assume

E0 passes. The optimization needs no narrowed contract. E1 may implement the
§5 sketch under these fixed terms:

- keep operand boxing and its placement (isolates dispatch/control savings);
- guard per access, after boxing, as §3.3;
- the hit path calls `stackless_oop_at` / `stackless_oop_at_put` unchanged;
- counters (eligible, hit, miss, fallback send) collected in an instrumented
  build only;
- the E2 matrix cases in Design 74 §7.4 map to §2 rows: replace `step:`
  (2.10 — unaffected, source sends still cross the driver), override slot
  access before/during execution (2.1/2.2), same-target logging lookup (2.8),
  remove/restore/inheritance/activate/rollback (2.2/2.6/epoch), GC and nested
  callback (2.9/2.14), suspend/resume (2.11/2.13), barrier and foreign owner
  (2.11/2.15), invalid input (2.11), repeated invalidation (§3.2 epoch).

Nothing here authorizes E1; that remains the user's decision after reviewing
this proof.
