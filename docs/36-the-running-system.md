# docs/36 — the running system: namespacing, globals, and define-method

Status: **v1 IMPLEMENTED (2026-06-29).** Implements "referencing the running
system" now that the whole birnam grammar compiles to native (docs/35 §3c.3).
Builds on the id environment (`idapi.zig`), the rung-2 store (docs/33), and the
codegen emitter (`src/id/codegen.zig`).

**Shipped (v1):** symbol literals (`#sel`, multi-keyword); the `Globals` facade
over a reserved `__globals__` store entity (zero new store core); lexical→global
reference resolution via the `birnam_global` codegen shim (late-bound,
relocatable, undeclared→nil); `[Reflect defineMethod: #sel on: T as: aBlock]`
via a per-define Tcc trampoline installed through the store mirror (multi-keyword
selectors included); `symbolName` appended to the IdApi; type-name seeds
(`Integer`/`String`). `test-codegen` 10 → 13; the siphon defines a global + a
method referencing it and dispatches it. **Deferred (v2) unchanged:** Binding
value-slot + baked handles, scope chains, env↔store unification, relational
engine. See §8 and §9.

## 0. The decisions (locked)

- **Reference through a stable indirection, never a baked object pointer.** A
  global names a *binding*; compiled code reads the binding's current value, so
  redefining a global is seen by already-compiled native code (late binding in a
  live image).
- **Principled path:** a global binding is a store cell. Referencing = reading a
  cell; redefining = asserting it. Folds namespacing into the cell substrate
  (the North Star: get/assert over a named, historied cell store).
- **`let` is always lexical.** Globals are defined explicitly via
  `[Globals at: #name put: value]` (primitive) or `export:` sugar.
- **Namespaces are objects** — a scope is something you send lookup messages to;
  qualified access (`[SomeModule thing]`) needs no new grammar.
- **define-method** installs a block as a method via a small **trampoline**
  (the block's `self` is the block object for captures; a method's `self` is the
  receiver — the trampoline bridges them). Must support multi-keyword selectors
  (`fooWithBar:andBaz:andQuux:`).
- **Forward / undeclared references auto-create an empty binding** (Smalltalk
  "undeclared variable" trick): a reference to a not-yet-defined global resolves
  to a stable binding whose value is nil until assigned. Enables forward refs and
  incremental REPL work.
- **Keep the id environment and the store layered** (do NOT unify the env into
  the store yet). Keep the store an *informal mirror* (authoritative reps stay
  where they're fast; facts are the queryable projection — the D3 pattern).

## 1. Two registries, cleanly layered

There are deliberately two name→object maps, at different layers:

| | the **id environment** | the **Globals scope** |
|---|---|---|
| where | libid floor (`import`/`exportObj`/`isExported`) | rung-2 store, reserved entity `#__globals__` |
| holds | module facades (`Store`, `Peg`, `Lang`, `Tcc`, `Codegen`) | birnam program globals + seeded type names |
| when | bootstrap (must predate the store — the store is loaded *through* it) | after the store is up |
| mutable / historied | replace-only, no history | cell semantics: assert, opt-in history, queryable |

The id environment is the bootstrap floor and never depends on the store (same
layering necessity as the libid floor itself). The Globals scope is built on the
store. They are bridged at *read* time (§2), not merged.

## 2. Reference resolution

`emit`'s identifier case gains an `else` branch (today an unbound ident →
out-of-subset → nil). New order:

1. **Lexical** — `bound_names` (lets / block params / captures). Emitted as the
   C local `b_<name>` (unchanged).
2. **Global** — otherwise a global reference, lowered to `birnam_global(birnam_intern("name"))`.

`birnam_global(sym)` (a codegen-owned shim, see §3) resolves:

1. Globals store has `(#__globals__, sym)` → its `currentValue`.
2. else the id env exports the name → `api.import(name)` (module facades, seeded
   type names that live only in the env).
3. else **auto-create** an empty binding `assert(#__globals__, sym, nil)` and
   return nil (undeclared / forward reference).

Store-first means a user `export:` can shadow a module facade of the same name.
No capitalization rule in the grammar; "unbound lowercase ident is probably a
typo" can be a later lint, not semantics.

## 3. The indirection model (v1 → v2)

**v1 (now): name-keyed runtime resolution.** A global ref emits
`birnam_global(birnam_intern("name"))`. The binding is the stable indirection;
we find it by name each use. Properties: late-bound (re-asserting updates the
fact `birnam_global` reads), **relocatable** (no baked pointers — survives a
future `tcc_output_file` → on-disk `.dylib`), correct, simple. Cost: one intern +
one `currentValue` (a fact scan) per reference. Fine for globals (cold relative
to locals).

**v2 (later, optional perf): baked binding handle + value slot.** Add a Binding
object with an authoritative value slot (O(1) read) kept in sync by `set:`, which
also projects the fact (the D3 one-write-two-representations pattern, now for
values). Memoize per (scope, name) so the handle is stable, bake it as an extern
symbol (`tcc_add_symbol`, via the step-1 `addSymbol:value:` path — declare
`extern char __bind_name[];` so the symbol's *address* is the binding, no deref),
and emit a raw slot load. Faster, still late-bound, still relocatable through the
linker. Deferred until profiling says globals are hot.

The shims that touch the store (`birnam_global`, `birnam_setGlobal`) live in the
**codegen** module (which already depends on the store), NOT in `tcc.zig` (which
stays store-free). codegen registers them per-compile via the staged Tcc
`addSymbol:value:` window — the exact use case step 1 was built for — alongside
`tcc.bindRuntime`'s floor shims.

## 4. Defining globals

- Primitive: `[Globals at: #name put: value]` — an ordinary keyword send to the
  `Globals` facade (itself a seeded global), which calls
  `[store assert: #__globals__ attr: #name value: value]`.
- Read back: `[Globals at: #name]`; enumerate: `[Globals names]` (reuses the
  store's `attrCount:` / `attrAt:index:`); membership: `[Globals includes: #name]`.
- `export:` sugar — a thin convenience (`[value exportAs: #name]` or a top-level
  `export: #name value: v`); resolves to the same assert. Optional.
- `let` is unchanged (lexical C local). To persist, you assert into Globals.

## 5. Symbol literals (grammar addition)

To *name* a selector in source (`#double`, `#plus:`, `#fooWithBar:andBaz:andQuux:`,
`#+`) we add a `symbol` rule to `lang.zig`:

```
symbol  = '#' ( (ident ':')+ / ident / binop+ )
primary = number / string / symbol / ident
```

codegen: `r_symbol` → strip the leading `#`, emit `birnam_intern("<rest>")`.
This is generally useful (symbols appear well beyond define-method).

## 6. Namespaces as objects

v1 is flat: one `Globals` scope + the env fallback. Because a namespace is just
an object you send messages to, `[SomeModule thing]` already works with zero
grammar — `SomeModule` resolves as a global, `thing` is a unary send.

v2: a scope *chain*. A `Scope` responds to `at:` / `at:put:` / `parent`;
module-local scopes keep a module's internals out of the root; qualified lookup
is message sends down the chain. A name is then a candidate key into the object
store (the F6 identity seam — "look up by name" = "look up by candidate key"),
which is where namespacing rejoins the extensional-identity vision (docs/30).
Deferred.

## 7. define-method

Surface (hosted on a reflect facade to respect D5 — meta protocol lives on the
mirror, not on base objects):

```
[Reflect defineMethod: #fooWithBar:andBaz:andQuux: on: Integer as: [:self :bar :baz :quux | … ]]
```

The block takes `self` as its first parameter, then the message arguments.

**Mechanism (trampoline).** A primitive method is a bare fn ptr with no
per-installation data (D3: methods stay fn ptrs, no method-objects), and the
Method ABI doesn't pass the selector — so a single generic trampoline can't
recover *which* block to call. Therefore define-method compiles a tiny
**per-define trampoline** via Tcc:

```c
void* __tramp_M(void* self, void** args, long n){
  void* a[K+1]; a[0]=self; long i; for(i=0;i<K;i++) a[i+1]=args[i];
  return birnam_send(<the block, via a baked binding>, birnam_intern("value:…(K+1)…"), a, K+1);
}
```

- `K` = selector arity (count of `:` for keyword, 0 unary, 1 binary) → drives
  both the arg copy and the `value:value:…` selector. Multi-keyword falls out
  for free.
- The block is referenced through a binding (§3) — reuse, not a new mechanism.
- Installed through the store's tracked mirror `[store addMethod: sel on: T
  with: __tramp_M]` → authoritative in the vtable dict, mirrored as a queryable
  `[T sel tramp]` fact (the informal-mirror principle, exactly as dispatch
  already works).

**Layering.** define-method lives at rung 3 (in codegen or a sibling `reflect`
module) because it needs both Tcc (compile the trampoline) and the store (mirror
install). The store does NOT gain a Tcc dependency. The store only provides the
install primitive it already has.

**Optimization (later).** A *literal, non-capturing* block passed to
`defineMethod:` can be compiled directly as a method (self = receiver, params =
args) with no trampoline and no closure. The trampoline path remains the general
case (runtime blocks, capturing blocks).

**Alternative considered, rejected for now.** Passing the selector into the
Method ABI would let one generic per-arity trampoline recover the block from the
dispatch mirror by `(vtableOf(self), selector)` — no per-define compilation. But
it changes every method's signature (a floor ABI break). Out of scope; noted.

## 8. Store rundown — keeping the store up to speed (no relational engine)

What the store needs to support all of the above, staged so the relational engine
is NOT on the critical path:

**v1 — now (near-zero store core change):**
- **Globals ride existing primitives.** `birnam_global` = `[store currentValue:
  #__globals__ attr: sym]`; `[Globals at:put:]` = `[store assert: …]`;
  enumeration = `[store attrCount:]` / `[store attrAt:index:]`. No new store core
  methods. Globals get fact storage + opt-in history *for free* (the property we
  want to preserve), default history OFF (D6).
- **Add a thin `Globals` facade object** over those primitives (`at:`, `at:put:`,
  `names`, `includes:`). It lives in the store module (namespacing is cell/
  relational territory) and is exported into the env at the store's bringUp, so
  birnam can resolve `Globals`. Seed type-name globals (`Object`, `Integer` →
  smallint_vt, `String`, …) and `Reflect` here too.
- **define-method reuses the dispatch mirror** (`addMethod:on:with:`) unchanged.

**v2 — later, optional (perf + principled):**
- A **Binding** object: authoritative value slot (O(1) read) + fact mirror on
  `set:` (the D3 pattern generalised from dispatch to values), memoized per
  (scope, name) for a stable bakeable handle. Unlocks the baked-handle reference
  path (§3 v2).
- **Scope chain** objects (§6 v2) for module-local namespaces.

**Explicitly NOT now (deferred to the relational engine arc):**
- Joins / datalog / rules / query planning — namespacing needs none of it.
- Unifying the id environment into the store — kept layered per the caution
  above; revisit when the relational engine matures and can absorb the env as a
  projection.

**Tidiness invariants (id ↔ store):**
- id env = bootstrap floor registry; never depends on the store.
- store = cells + the Globals scope; depends on id only (NOT on Tcc).
- reflect / define-method = rung 3; depends on store (mirror) + Tcc (trampoline).
- the store stays an informal mirror: dispatch stays fn-ptr-authoritative +
  fact-mirrored; in v2, binding values stay slot-authoritative + fact-mirrored.

## 9. Work breakdown / sequencing

1. **Symbol literals** — `lang.zig` grammar rule + `codegen.zig` `r_symbol` emit.
   (Small; unblocks naming selectors.)
2. **Globals facade + seeds** — store module: `Globals` object over
   assert/currentValue on `#__globals__`; export `Globals`, `Reflect`, and type
   names. (Small.)
3. **Global reference resolution** — `codegen.zig` `r_ident` else-branch →
   `birnam_global`; implement `birnam_global` / `birnam_setGlobal` shims (codegen-
   owned, store-backed) and register them via the staged `addSymbol:value:` path;
   auto-create undeclared bindings. (Core of v1.)
4. **define-method** — reflect facade `defineMethod:on:as:`; per-define
   trampoline compiled via Tcc with the block bound through a binding; install
   via the store mirror; arity from the selector (multi-keyword). (Largest item.)
5. **Siphon + tests** — a birnam program that defines a global, references it
   from compiled native code, redefines it (proving late binding), and defines a
   multi-keyword method on a type then dispatches it.

Deferred (v2): Binding value-slot + baked handles; scope chains; literal-block-
as-method optimization.

## 10. Open flags

- `export:` exact spelling (sugar shape) — pick when implementing §4.
- Whether to default global history ON (audit who rebound what) vs OFF (D6
  consistency). Recommend OFF + opt-in, consistent with everything else.
- Error model (F7) for an undeclared/nil global read and wrong-arity block sends —
  currently nil / panicking DNU; settle with F7.
