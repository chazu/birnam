# W1 — Full-Image Save/Load (scoping plan)

> **Status:** scoping, 2026-06-25. The concrete plan for **ADR-image-1 W1** —
> "store all facts; loading a full image = a booted system, no recompile; delete
> the `m == M_USER` write filter." Grounded in a read of the writer/loader
> (`image.zig`), the boot path (`shell.zig`), and the relink machinery
> (`ffi_module.resolveAllPending`, `image.rematerializeClosures`).
>
> Context: [ADR-image-1](../ROADMAP.md) (Pole A, W1–W6), the
> [cell north star](10a-north-star-cell-model.md) (native code = a cache of
> bytecode/data), [Load & Memory](19-load-memory.md), [Code-Fact Tiers](19a-code-fact-tiers-plan.md),
> [Images & AOT](2e-images-aot.md). W6 (just shipped) is the enabler — D5.1
> established identity is **relative and name-anchored**, which is exactly the
> property a full-image reload needs to be sound.

## 1. The real problem (it is not "delete one line")

The write filter is one line — `image.zig:308`, `if (entry.m != kernel_eids.M_USER) continue;`
— but removing it is the *easy* 10%. The load story is the hard 90%, and it
turns on one constraint:

**The irreducible residue is native function pointers, nothing else.**
`valueSupported` (`image.zig:127`) serializes immediates (eids, smallints,
bool/nil), strings, floats, RRB vectors, ASSOC maps, and capture specs. It
refuses **bare pointers**. Crucially:

- A **method binding** `[vtable selector mkEid(closure_eid)]` has an **eid**
  value → serializes fine. So *bindings* round-trip today already.
- A **bytecode method** carries `[m :bytecode-stream …]` / `:constant-pool` /
  `:method-captures-ptr` — all data → serialize, and `rematerializeClosures`
  (`image.zig:533`) rebuilds the closure object on load.
- A **native/primitive method** (peg/lang/json/net kernel methods, `+` on
  SmallInt, …) binds to a closure whose `.method` is a **native fn-ptr**. The
  *fact* serializes (eid value); the *closure object* (the fn-ptr) cannot. On
  load the binding points at a closure-eid with **no object-table entry** →
  dispatch misses.

So W1 = *serialize the whole fact-set* (trivial) **+ reconstruct the native
closure objects on load** (the work). This is the same shape as two mechanisms
that already exist:

- `rematerializeClosures` — rebuilds **bytecode** closures from facts.
- `ffi_module.resolveAllPending` (`ffi_module.zig:217`) — relinks **FFI wraps**
  by `:ffi-path`: `dlopen` + bind by name to the existing wrap entity. This is
  already "relink by name," already idempotent across save/load/freeze.

W1 generalizes that pattern to **native module methods**.

## 2. What the current `--boot-image` actually does (and its gaps)

`--boot-image` (`shell.zig:1024`) is a **hybrid**, not a full-image load:

1. Loads the native prefix `[peg, lang]` — full dylib init re-runs, which
   *re-creates their native closures* **and re-asserts their CODE facts with
   freshly-minted eids**. Works only because the prefix loads in a fixed order,
   so those eids land deterministically below the image's eids ("eid
   coordination," `shell.zig:1029`).
2. Replays the image — which carries only `M_USER` facts (the write filter):
   user data + pure-birnam method definitions tagged `M_USER` (commit `eb8b73e`).
3. `rematerializeClosures` + `resolveAllPending` rebuild bytecode + FFI closures.

**The gaps fall straight out of the hybrid:**
- **json `parse:` broken** — json builds a *stateful* PEG grammar at dylib init
  (`peg newGrammar` / `defineRule:`), creating CODE facts under json's
  provenance (not `M_USER`). They are neither serialized (filter) nor
  reconstructed (json isn't in the `[peg, lang]` prefix). The grammar vanishes.
- **net not re-inited** — a Zig FFI module, skipped by the prefix.
- **The `eb8b73e` bug class** — "this CODE fact was secretly per-object data,
  re-tag it `M_USER`." Hand-patching the wrong boundary.

W1 dissolves all three by making the **image carry the facts** and load
reconstruct **only** the native fn-ptrs.

## 3. Design fork

- **Model A — full-fact image + native relink (RECOMMENDED).** Drop the filter;
  serialize every fact. On load: replay all facts, then relink native closures
  by name (generalized `resolveAllPending`). Native dylibs are loaded **only to
  supply fn-ptrs**, not to re-assert facts or mint eids. The image *is* the
  system; this is true Pole A and sets up Q1 ("boot = load the seed image") and
  W5 (per-module artifacts).
- **Model B — widen the prefix (band-aid, rejected).** Add json/net to the
  boot-image native prefix so their init re-runs. Smaller, but keeps the
  recompile-at-boot, the `M_USER` taxonomy, and the redundant classification the
  ADR set out to delete. Not W1.

**Take Model A.** It is the only one that removes the boundary instead of
patching it.

### The load-eid question (where D5.1 pays off)

Under Model A a native dylib must bind its exports to the **image's** method
eids, not freshly-minted ones. Two ways:
- **(deterministic coordination)** — keep relying on fixed load order so the
  dylib re-mints the same eids the image holds. Less code; brittle (any reorder
  diverges; W2's measured boundary).
- **(relink by name)** — the dylib's methods are bound to existing eids by
  resolving **interned names** → eids through the interner, exactly like FFI
  `:ffi-path`. Principled, robust to reorder, and **the D5.1 mechanism**: names
  are the durable anchor, eids are handles. Same hook W5's linker will use.

**Recommend relink-by-name**, incrementally (keep coordination working until the
relink hook lands, then switch). This makes W1, W5, and D5.1 one mechanism.

## 4. Staged implementation (green checkpoint at each stage)

**Stage 0 — characterize the residue (measurement, no behavior change). DONE
2026-06-25.** Added `image.characterizeResidue` + a `BIRNAM_RESIDUE_DUMP`
shell hook (env-gated, exits after dumping) that walks the full fact-set of a
real default boot (peg+lang+json+net+stdlib). Result:

```
total facts:                8903
  M_USER (written today):   5570
  non-M_USER (filtered):    3333   <- what W1 newly writes
    would serialize:        3324
    RESIDUE (native ptrs):     9   <- the irreducible part
residue by attribute:  function-impl (7), reader-impl (2)
```

**Findings — a clean go-signal for Model A:**
- The residue is **9 facts**, far smaller than feared, and *not* what §1 first
  guessed. Method bindings, bytecode, AST, vtables, **and json's `defineRule:`
  grammar** are all among the **3324 that serialize cleanly** — so a full image
  carries the grammars, closing the json gap by construction.
- The 9 residue facts are exactly two native-impl attributes —
  **`ATTR_FN_IMPL`** (datalog predicate native impls; `predicate.zig:115`
  asserts `mkPointer(box)`) and **`ATTR_READER_IMPL`** (reader-macro native
  impls) — bare boxes wrapping C function pointers. No surprise kinds.
- Note: the *object-table* native-method closures (behind eid-valued
  `[vt sel closure_eid]` bindings) are **not facts**, so they don't appear here;
  their reconstruction is the other half of Stage 2 (relink, alongside
  `rematerializeClosures` for bytecode), not a fact-serialization concern.

**Consequence for the plan:** Stage 2's fact-value residue is tiny and uniform —
9 native-impl pointers on 2 attributes, relinked by name exactly like FFI wraps.
The diagnostic stays in-tree as a Stage-1+ verification tool.

**Stage 1 — serialize the full fact-set (writer). DONE 2026-06-25.** Added
`image.writeFull` (and shared `writeImpl(full)`); `write` stays M_USER-only so
the default `:save`/boot path is **unchanged**. A full image stamps
`FULL_FORMAT_VERSION = 7` (byte-identical layout to v6; the version is a
load-time signal "self-contained, relink don't recompile" for Stage 3). Reader
accepts v5..v7. *Tests:* unit round-trip — `write` drops a non-M_USER fact,
`writeFull` round-trips it (eid value intact), versions stamped 6 vs 7
(image_test 18/18). End-to-end on the real boot (`BIRNAM_RESIDUE_DUMP`):
**8894 facts written, 9 skipped (residue), 314 KB** = exactly `8903 − 9` — the
whole tower (code + data) in ~307 KB. v6 path + all examples unaffected (lang
79/79).

**Stage 2 — load a full image into a working system. PARTIAL 2026-06-25 — core
works; one gap found + ADR diagnosis corrected.** Added `BIRNAM_SAVE_FULL=<path>`
(writes a v7 image of the live system) and tested loading it via `--boot-image`.

**What works (the core W1 thesis, demonstrated):** a v7 full image loads — 8894
facts replay, 198 bytecode methods rematerialize — and yields a working system
**equal to normal boot** for: native/kernel methods (`[3 + 4] => 7`), pure-birnam
collection methods (`inject:into: => 10`), and user data. Crucially, the feared
**native-relink ABI hook is NOT needed**: native fn-ptrs come from the kernel
bootstrap (always runs) + the peg/lang dylibs (already loaded by `--boot-image`);
the 9 residue facts (`function-impl`/`reader-impl`) are re-created by bootstrap.
So Stage 2 needs *less* new machinery than the plan assumed.

**The remaining gap (precisely isolated, and the ADR was wrong about it):**
`Json parse:` returns `nil` under boot-image (both v6 and v7). The ADR blamed
"the stateful PEG grammar." **It is not the grammar** — `Json stringify:` (which
uses global `jStr`, *no grammar*) fails identically. Root cause: globals live on
`SCOPE_VT` entities as M_USER facts on stable eids (`TOP_SCOPE_EID=185`) and *do*
round-trip — but json's methods are defined at module file-in and **close over**
module-level `let` globals (`jg`/`jStr`). After load the methods rematerialize
yet their **captured-global linkage is not reconnected**, so those refs resolve
to nil. The defect is in **closure-capture rematerialization across image load**,
not provenance or the grammar.

**Root cause, pinned by black-box bisection (2026-06-25):** the defect is
**rematerialized standalone block closures stored as global values**, not the
grammar, not captures-into-vtable-methods, not provenance. Evidence under v7
boot-image:
- plain entity globals round-trip: `jg` → `<eid:26594>`, identical to normal;
- the grammar works: `[jg parse: "[1,2,3]" from: "doc"]` → same CST eid as normal;
- **freshly-compiled** blocks work: `[[:x | [x printString]] value: 5]` → `"5"`;
- but the **rematerialized** block global fails: `[jStr value: 5]` → `nil`
  (normal → `"5"`), even on the `v isNumber → [v printString]` path that touches
  no other global. So the rebuilt closure does not *execute* correctly — the VM,
  dispatch, and fresh-closure creation are all healthy.

`jStr` etc. are top-level `let` blocks (`json.birnam:57–108`) created via
`MAKE_BLOCK` at file-in, capturing their environment into a transient
`BytecodeClosure`. `rematerializeClosures` rebuilds the wrapper
(`data = mkEid(method_eid)`) but the **captured environment is neither persisted
nor reconstructable** (no live parent frame exists at load). A block stored as a
durable value (a global binding, or a `method:do:` body) therefore comes back
inert.

**Root cause — DEFINITIVELY PINNED (VM trace, 2026-06-25).** `[jStr value: 5]`
returns nil for *every* input (nil/5/"x"/true), including the first branch that
needs no captures — so the body never runs. A gated trace in
`bytecodeDispatcherImpl` **never fired** for the call: `value:` on jStr never
reaches the closure dispatcher, yet returns nil cleanly (no doesNotUnderstand).

The mechanism: a block `let jStr := [:v | …]` is **two entities** — a
*block-method* entity (carries `:bytecode-stream`) and a *closure-wrapper* entity
(`[vt=CLOSURE_VT][method=bytecodeDispatcherImpl][data=BytecodeClosure]`, holding
the method link + captures + parent-self). `jStr`'s binding value is
`mkEid(wrapper_eid)`. On save, the wrapper's binding round-trips and its
`[wrapper :vtable CLOSURE_VT]` fact round-trips — **but the wrapper's runtime
object lives only in the object table (a `BytecodeClosure` pointer), which is not
facts and is never serialized.** `rematerializeClosures` only rebuilds entities
that carry `:bytecode-stream` (the *block-method*, not the *wrapper*). So after
load the wrapper has a `:vtable` fact but **no object-table entry** → `value:`
finds no closure object to run → nil. (Plain entities like `jg` and vtable
methods round-trip because they need no wrapper object; that is why they work.)

**The fix (the genuine Stage-2 core):** `rematerializeClosures` must also rebuild
**closure-wrapper** entities, which requires persisting per wrapper: its
block-method eid, parent-self, and captures — none of which are facts today
(they live in the `BytecodeClosure`). Design tension to resolve first: **which
closures to persist.** Persisting *every* closure as facts is too costly — a
block created in a hot loop would assert capture facts every iteration (write
amplification). The right scope is **durable** closures only (bound to a
global/method, surviving file-in), not transient activation blocks. Options:
- **(a)** at save, walk the object table for `CLOSURE_VT` wrappers reachable from
  durable bindings/methods; serialize `(block_method_eid, parent_self, captures)`
  in a dedicated image section; rebuild in `rematerializeClosures`. Localizes the
  cost to save/load; needs a reachability pass + format addition.
- **(b)** mark a closure durable when it is stored into a scope binding / method
  and assert its linkage as facts at that point (not at every `MAKE_BLOCK`); they
  then round-trip and rematerialize like any entity. Cleaner steady-state, but
  needs the durable/transient distinction in the binder.

*Test for either:* `[jStr value: 5]` → `"5"` and `Json parse:`/`stringify:` work
from a v7 image alone.

**Status:** Stages 0–1 done; Stage 2 load-core proven (native/kernel/pure-birnam
methods, grammars, plain entities, user data all round-trip); the one remaining
blocker is durable closure-wrapper rematerialization, root-caused above. Tree
clean/green (trace reverted; no speculative fix committed).

**Status:** Stages 0–1 done; Stage 2 load-core proven; this closure-capture
defect is the one remaining blocker and is scoped above. Tree is clean/green
(no speculative fix committed).

**Stage 3 — make full-image boot first-class + default.** Promote the
boot-image path: on startup, if a valid image (matching `abi_hash`) exists, load
it (Stage 1+2 path) and **skip module recompile**; else fall back to
recompile-from-source (no image / version mismatch / first run). Fold
`--boot-image` into this. *Test:* `json parse:` and `net` work from the image
alone (the §2 gaps closed); a deliberately-stale `abi_hash` falls back to
recompile cleanly.

**Stage 4 — delete the dead boundary + docs.** Remove the `M_USER` write-filter
special-casing and the `eb8b73e`-style re-tag hacks (now unnecessary — `m` stays
a provenance *tag*, not a persistence gate). Rewrite `docs/2e-images-aot.md` and
`VISION.md §Image`. *Test:* full suite green; examples (incl. logue/minesweeper)
run from a saved image.

## 5. Risks & mitigations

- **ABI version.** A full image binds to the kernel eid layout; bump
  `FORMAT_VERSION`→v7 and gate on `abi_hash` (already in the manifest). Mismatch
  → fall back to recompile, never load-into-wrong-kernel.
- **Determinism.** W2 measured: eids stable for fixed module set + load order;
  they renumber on reorder. Stage 2's relink-by-name removes the load-order
  dependency for *named* entities; anonymous data eids are replayed verbatim
  (D5.1 tier 3). Re-confirm W2's kernel-edit case empirically before relying on
  it (still inferred).
- **Image size growth.** The image now carries the full tower (the ADR accepts
  this — mitigated by strip for shipping: mode-3 = 11 MB). Log the dev-image
  size; it is not the ship artifact.
- **Relink ordering.** Native relink must run after fact replay and before first
  dispatch; co-locate with the existing `rematerializeClosures` +
  `resolveAllPending` sequence (already the proven post-load order).
- **Partial residue.** If Stage 0 surfaces pointer-valued facts *other* than the
  three known kinds, stop and classify before Stage 1 — an unhandled residue
  silently drops state.

## 6. What this unblocks

- **Q1** — Stage 1+3 make "boot = load an image"; the kernel seed-as-embedded-
  image (Q1 move #1) then rides the same path.
- **W5** — Stage 2's relink-by-name *is* the fact linker's core (resolve names →
  eids, bind across independently-numbered artifacts); per-module precompiled
  artifacts become an extension, not new machinery.
- **Native-as-cache** — Stage 2 reifies "native code is a cache relinked on
  load," the cell-model framing of the residue.

## 7. Recommended first move

Stage 0 (measurement) — it is cheap, has no behavior risk, and its output
(the exact residue set) determines whether Stage 2 is "the three known kinds" or
something larger. Do not touch the writer until the residue is characterized.
