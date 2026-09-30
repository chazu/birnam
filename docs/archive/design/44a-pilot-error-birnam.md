# 44a — Pilot: migrate the error *protocol* to birnam

> **Status:** SHIPPED (2026-07-09). Phase 1 of
> [44 — Toward Self-Hosting](44-toward-self-hosting.md). Purpose: prove the
> **delete-Zig / recompile-from-source / stay-green** loop end-to-end on a small,
> low-risk target, and validate the from-zero bootstrap invariant in practice.
>
> **Implemented with one refinement over the original spec.** The spec used a
> `Object>>includes: → false` trick to make multi-kind matching safe without a type
> test. That was replaced by the *right* thing: a **mirror-based type-predicate
> family**. `Reflect` (the informal Bracha mirror — a stratified meta-interface,
> not base-object protocol) gained one floor primitive `[Reflect vtableOf: x]`
> (`codegen.zig`), and `isSymbol:`/`isString:` are now **pure birnam on `Reflect`**,
> comparing an object's vtable to an exemplar's. This also surfaced and fixed a real
> object-model wart: **`Symbol` was the lone atomic kind exported as its raw vtable**
> instead of a prototype object; it is now refounded as a proper prototype
> (`id.zig`, `Symbol` + `SymbolVt`, mirroring `Vector`/`VectorVt`). See §11.

## 1. Why error is the pilot

It splits cleanly along the floor seam (§2), the migrated half is pure block
orchestration needing **no new primitive and no new export**, and its full-stack
behavior is *already* covered by `examples/errors.birnam`. It touches the one scary
thing — the `doesNotUnderstand:` swap on `Object` — but leaves that keystone in
Zig, so the risky part never moves. Small, honest, and it exercises the whole loop.

## 2. The seam

`error.zig` is 286 LOC. Split it: **representation + the DNU swap + factory stay in
Zig**; **control-flow protocol + formatting move to birnam**.

### Stays in `error.zig` (the core, ~174 LOC — floor-adjacent, permanent)

- The `Err` struct, `mkError`, `setLayout`, `isErrorValue` — the native
  representation. birnam cannot mint the 2-slot native payload; it only orchestrates.
- `errDnu` (error `doesNotUnderstand:` → self, the propagation heart) on `error_vt`.
- `objDnu` — **THE SWAP** on `object_vt`: mint a `#doesNotUnderstand` error naming the
  missed selector. Stays because it needs `symbolName` + `std.fmt` to format the
  message, and because it *is* the dispatch keystone.
- `errIsError`→true / `objIsError`→false; `errKind` / `errMessage` accessors.
- The factory: `kind:message:` / `kind:` / `message:` (they `alloc` the native struct).
- The exports: `ErrorVt`, `ErrorFactoryVt`, `Error`.

### Moves to `stdlib.birnam` (deleted from `error.zig`, ~112 LOC)

`on:do:`, `kindMatches:`, `ensure:`, `ifError:`/`ifNotError:` (both `Object` and error
variants), `printString`, and `String` `error`/`error:`. Plus one enabling one-liner
(`Object>>includes:`). All are block orchestration over the core primitives.

## 3. The verified enabling facts (why this works with no new machinery)

| Need | Provided by | Evidence |
|---|---|---|
| Install methods on **error instances** from birnam | `ErrorVt` is already exported, and `ensurePrivateVtable` accepts a raw vtable (`if (cur == vtable_vt) return obj`) — so `on: ErrorVt` installs directly on `error_vt`, which all `alloc(error_vt)` instances share | `error.zig:273`; `id.zig` `ensurePrivateVtableC` |
| Render a kind as `#name` | Symbols have a birnam-visible `printString` | `prelude.zig:728` |
| `printString` string assembly | `String>>,` (concat) | `collection.zig:1341` |
| nil-message branch in `printString` | `isNil` on `nil`/`Object` | `collection.zig:1333,1336` |
| Safe multi-kind probe without an `isSymbol` test | add `Object>>includes: → false` ("a non-collection includes nothing"); collections override it | `stdlib.birnam:89` (Collection override already present) |
| Read an error's fields | `[e kind]` / `[e message]` stay as core accessors | `error.zig:250-251` |
| Raise from birnam | `[Error message: s]` / `[Error kind: k message: s]` factory stays core | `error.zig:256-258` |

No new IdApi surface, no new kernel export, no new codegen. The one *addition* is a
principled stdlib one-liner (`Object>>includes:`), noted explicitly so it isn't a
silent semantic change.

## 4. The birnam code (append to `src/id/stdlib.birnam`, after the Collection block)

```birnam
// ── error protocol (migrated from error.zig, docs/44a) ───────────────────────
// The representation, the doesNotUnderstand: swap, and the factory stay in the
// Zig core (error.zig). What lives here is pure block orchestration over the core
// primitives [e isError]/[e kind]/[e message] and the [Error …] factory.

// Type predicates live on the MIRROR (Reflect), not on base objects — a type
// query is asked THROUGH the mirror (Bracha stratification). `[Reflect vtableOf: x]`
// is the one floor primitive; each predicate compares x's vtable to an exemplar's.
[Reflect defineMethod: #isSymbol: on: Reflect as: [:self :x | [[self vtableOf: x] == [self vtableOf: #s]]]];
[Reflect defineMethod: #isString: on: Reflect as: [:self :x | [[self vtableOf: x] == [self vtableOf: ""]]]];

// Does a caught error's kind satisfy an on:do: spec? wildcard #error, an exact kind
// symbol, or any collection of kinds. A single symbol short-circuits to false; only
// a non-symbol spec (a collection) is probed with `includes:`.
[Reflect defineMethod: #kindMatches: on: ErrorVt as: [:self :want |
   [[[want = #error] | [want = [self kind]]]
      ifTrue:  [| true]
      ifFalse: [| [[Reflect isSymbol: want] ifTrue:  [| false]
                                            ifFalse: [| [want includes: [self kind]]]]]]]];

// [[block] on: spec do: handler] — run the protected block; if it yields an error
// whose kind matches spec, run the handler with it; else pass the value through.
[Reflect defineMethod: #on:do: on: Object as: [:self :spec :handler |
   [let val := [self value];
    [[val isError]
       ifTrue:  [| [[val kindMatches: spec] ifTrue:  [| [handler value: val]]
                                            ifFalse: [| val]]]
       ifFalse: [| val]]]]];

// [protected ensure: cleanup] — always run cleanup, answer protected's result.
[Reflect defineMethod: #ensure: on: Object as: [:self :cleanup |
   [let val := [self value]; [cleanup value]; val]]];

// ifError:/ifNotError: — dispatch on whether the receiver is an error.
[Reflect defineMethod: #ifError:    on: Object  as: [:self :h | self]];
[Reflect defineMethod: #ifNotError: on: Object  as: [:self :h | [h value: self]]];
[Reflect defineMethod: #ifError:    on: ErrorVt as: [:self :h | [h value: self]]];
[Reflect defineMethod: #ifNotError: on: ErrorVt as: [:self :h | self]];

// printString — "error(#kind: message)"; kind is always a symbol, message may be nil.
[Reflect defineMethod: #printString on: ErrorVt as: [:self |
   [let k := [[self kind] printString];
    let m := [[[self message] isNil] ifTrue: [| ""] ifFalse: [| [": " , [self message]]]];
    [[["error(" , k] , m] , ")"]]]];

// Raise from a message string: ["boom" error] → #error; ["boom" error: #kind].
[Reflect defineMethod: #error  on: String as: [:self | [Error message: self]]];
[Reflect defineMethod: #error: on: String as: [:self :kind | [Error kind: kind message: self]]]
```

Ends without a trailing `;` (last statement of the file — matches the current
file's convention).

## 5. The Zig deletions (`error.zig` 286 → ~174)

Delete the method bodies + their `api.method(...)` install lines + the now-unused
interns:

- Bodies: `errPrintString` (93–114), `errIfError`/`errIfNotError` (115–125),
  `kindMatches`/`objOnDo`/`objEnsure`/`objIfError`/`objIfNotError` (164–209),
  `strError`/`strErrorKind` (211–221).
- Installs (in `bringUp`): the `printString`, `ifError:`, `ifNotError:` lines on
  `error_vt` (252–254); the `on:do:`, `ensure:`, `ifError:`, `ifNotError:` lines on
  `object_vt` (263–266); the `error`/`error:` lines on `string_vt` (269–270).
- Interns now unused by the remaining core (`errDnu`/`objDnu`/factory/accessors use
  none of them): `s_value`, `s_value1`, `s_wild`, `s_includes` decls (46–49) + inits
  (227–230).

Everything else stays exactly as-is. `error.zig` remains a kernel module; it just
installs fewer methods.

## 6. The test seam moves with the code seam

The kernel `test-error` step builds `error.zig` **in isolation** (no stdlib), so any
test there for a migrated method would fail after the move. Split the tests the same
way:

- **Keep in `error_test.zig` (core, isolated):** the DNU swap → `#doesNotUnderstand`
  (67), propagation through a chain (75), `isError` partition (83),
  raising-representation via `kind:message:`, `[e kind]`/`[e message]` accessors, and
  `#zeroDivide` from `/0` (134). These test the core that stays.
- **Remove from `error_test.zig` and rely on full-stack coverage:** `on:do:` match /
  wildcard / multi-kind (93, 103, 182), `ensure:` (167), `ifError:`/`ifNotError:`
  (113), `printString` (146), String raising sugar (124). These are already exercised
  by **`examples/errors.birnam`** (single + multi-kind `on:do:`, `ensure:` on
  value+error, `printString`, `kind`) on the full boot where stdlib is loaded.

If tighter assertions than the example's `println` are wanted, add a small
`stdlib`-level test file that runs on the full stack (the honest home for
self-hosted-protocol tests) — but the example already proves the behavior.

## 7. Ordering & correctness notes

- **Boot window.** `error.zig` (kernel) loads before `stdlib.birnam`. Between the two,
  an error's `printString` falls back to `Object>>printString` ("anObject"); nothing
  critical prints an error during boot, and `examples`/tests run post-stdlib. After
  stdlib loads, `ErrorVt>>printString` is in place.
- **`stdlib` placement.** Append after the Collection block (`stdlib.birnam:89`) so
  `Collection>>includes:` already exists (it overrides the new `Object>>includes:`
  for real collections). `Error`/`ErrorVt` are kernel exports, available before
  stdlib runs. No change to stdlib's `requires: #[]`.
- **Eager `|`.** All three `kindMatches:` operands are Booleans, so eager `|`
  (birnam's `|` evaluates both sides) is correct and side-effect-free — the wildcard
  case still short-circuits *semantically* (true dominates) even though C is
  evaluated. No `or:`-laziness needed.
- **The swap stays first.** `error_vt`'s own `doesNotUnderstand:` (→ self) must keep
  shadowing the `object_vt` swap — untouched here, since both stay in core.

## 8. Verification — the loop, end to end

1. `zig build test-error </dev/null` — the reduced core suite passes (run foreground;
   see the test-harness gotcha).
2. `zig build` then `./zig-out/bin/birnam examples/errors.birnam` — the migrated
   protocol behaves identically (the `println`s match the inline `// →` comments).
3. Run the example suite (`modules/run-tests.birnam` / the examples) — nothing
   regressed.
4. **The bootstrap proof:** with the dylib stdlib absent (`rm zig-out/lib/libbirnam_stdlib.dylib`,
   or `BIRNAM_NO_STDLIB` unset but dylib stale), confirm `loadStdlib` **recompiles
   `stdlib.birnam` from embedded source** via the retained Zig compiler and the error
   protocol still works — this is the from-zero loop the whole exercise validates.
5. `zig build stage1` — the AOT path recompiles the enlarged `stdlib.birnam` to a
   dylib cleanly (exercises the `__amblk` salt path with the new methods).

## 9. Rollback

Local and contained: `git checkout src/id/{error.zig,error_test.zig,stdlib.birnam,
codegen.zig,id.zig,main.zig}` then `zig build && zig build stage1`. No IdApi/ABI
change and no image-format change. The one deliberate *addition* is `Reflect
vtableOf:` (a floor-reflection method on the mirror) plus the `Symbol`/`SymbolVt`
export split — both principled and independently useful.

## 10. Honest accounting

`error.zig` **286 → 179** (−107 Zig); `stdlib.birnam` +55; `error_test.zig` trimmed
to the 4 core tests. The mirror work added **+11 Zig** (`codegen.zig`, `Reflect
vtableOf:`) and **+9 Zig** (`id.zig`, Symbol refounding) — floor reflection, not
library code — while removing the `Object>>includes:` semantic wart entirely. Net
production Zig for the whole change: **~−90**, and it self-hosted a reusable
type-predicate family (`isSymbol:`/`isString:`) in the bargain.

## 11. The mirror decision + the Symbol refounding

Two things the pilot surfaced, both improvements over the original spec:

- **Type predicates belong on the mirror, not the base object.** `[x isSymbol]`
  would bake a reflective query into every object's protocol (anti-stratification,
  Bracha). Instead `Reflect` — the informal fact-store-backed mirror that already
  owns `defineMethod:on:as:` — gained ONE floor read, `[Reflect vtableOf: x]`, and
  `isSymbol:`/`isString:` are pure birnam on `Reflect` comparing vtables to an
  **exemplar** (`[self vtableOf: #s]`). Exemplar comparison is deliberately
  independent of how a kind's prototype is exported, so it survives the fix below.
- **`Symbol` was the one atomic kind exported as its raw vtable** (`exportObj(
  "Symbol", oop(symbol_vt))`) — every other kind (`Object`/`Integer`/`String`/
  `Vector`…) is a prototype object whose vtable is exported separately. Refounded to
  match: `Symbol` is now a prototype (`oop(intern("Symbol"))`, `symbol_vt.owner`
  set so `on: Symbol` patches in place) and `SymbolVt` exports the vtable, mirroring
  `Vector`/`VectorVt`. `main.zig`'s re-import self-check moved `Symbol`→`SymbolVt`;
  `json.birnam`'s `on: Symbol` was unaffected (resolves through the owned vtable).
