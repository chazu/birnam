# Work Breakdown — Reaching the Destination

> The ordered work to reach [`docs/30` — The Destination](30-the-destination.md).
> Two arcs: **W7 (The Superimposed Floor)** hardens the bootstrap to
> pointer-identity-first so the eid/fact layer becomes a true superimposition;
> **W8 (Identity as Policy)** makes object identity an overridable policy.
> **Decision (2026-06-26): W7 leads** (get the foundation solid first); W8's
> first cut (D1) can ride alongside, but its research payoff waits on the
> deferred D0 gate. Background: `docs/30`, `docs/20a` (the coupling audit /
> Q2 seam), `docs/26` (dispatch), `docs/13` (object model).

## Decisions locked / deferred

- **B0 — LOCKED (2026-06-26).** The floor metaobject-graph representation is
  **dumb and fast**: a `parent: Ref` slot + a small **inline open-addressed
  `(selector-eid → method-ref)` array** in the vtable struct. **No HAMT** at layer
  0 — the floor has ~5 vtables with a few methods each; richness lives in the
  *projected* facts above the floor, not in the frozen floor itself.
- **D0 — DEFERRED.** The `identityOf:` protocol surface (`identityKey:` syntax,
  resolution hook points) is not decided yet. This gates W8 D2+ (candidate-key);
  it does **not** gate W7, nor W8 D1 (the minimal "unfreeze identity" change).

## Arc lead & critical path

```
W7.0 ──▶ A1 ──▶ C1 ──▶ A2                 (mechanical — start now, no design debt)
                 │
                 ▼
        B1 ▶ B2 ▶ B3 ▶ B4                 (the one cut — B0 locked; gate on kernel-bench)

W8 (parallel, secondary):
        D1  ............ (rides alongside; counter-default, no D0 needed)
        D0 gate ▶ D2 ▶ D3 ┄┄▶ D4          (candidate-key research — blocked on D0)
```

- **Start immediately, zero design debt:** W7.0 → A1 → C1 proves the mount /
  republish machinery on the safe writes before the hard cut.
- **D1 may ride alongside W7** opportunistically; everything past it in W8 waits
  on D0.

---

## W7 — The Superimposed Floor

**Framing (siphon, `docs/30` §9):** W7 is the first step of decomposing the
monolithic `bootstrap.zig` (`→ siphon.zig`) into an ordered tower the **siphon**
mounts: id → fact store → gc → peg → compiler. Track B is what makes "id is the
first module" literally true; Track C's `mountIdentityLayer` is the store mount.
The destination form of the boot seed is **`seed.birnam`** (human-authored
fact-assertions parsed by peg's first hand-coded grammar), replacing the imperative
Stages 4–17 — a later step on the same line, once peg is mounted.

**Goal / acceptance for the whole arc:** boot the id closure with the fact store
*unmounted*; prove a real dispatch (`[SMALLINT_VT lookup: #+]` and an actual
method send) works on **pointer identity + struct slots alone, no facts in
existence**; then `mountIdentityLayer` projects the struct graph into EAVT/AVET
and reflection/query see the same graph as facts. `kernel-bench` shows no
dispatch regression. All suites green.

**Status (2026-06-27) — struct-primary floor SHIPPED + INVERTED; the floor is
factless.** Commits W7.0 `6a10c03` · A1/C1 · B repr+proof `f41c193` · B4 methods ·
B4 delegation `a9293bb`. Done:
- **A1 ✅** defer `:name` to the mount seam. **C1 ✅** `mount.mountIdentityLayer`.
- **B1 ✅** `floor.zig` — vtable structs carry parent + kernel-method dict (B0
  layout). **B2 ✅** `send.resolve` = fact-first, floor-struct fallback.
- **B4 (methods) ✅** the 5 kernel methods are struct-resident — **no facts**.
- **B4 (delegation) ✅** the floor's `:delegate` is struct-resident — **no facts**.
  `siphon.delegate` sets the struct parent at the assert site for materialized
  floor vtables (~34 facts dropped); the ~7 *non-materialized* kernel vtables
  (STRING/RANGE/CELL/XF/CALLBACK/STRUCT_TYPE/BYTECODE_METHOD — no struct) keep
  their facts. The mount-time sync is gone.
- **Proof ✅** `siphon_test`: OBJECT_VT.init AND VTABLE_VT.delegate are factless at
  boot, yet `[OBJECT_VT init]` dispatches. W7.0 probe back to write-only (1
  read-before-mount). Green cold + warm + lang_module 84.

**The floor metaobject graph now carries ZERO facts.** The id closure dispatches
its delegation + methods on structs alone; the fact store is a genuine
superimposition over it.

Remaining (smaller, optional):
- **Materialize the 7 non-materialized kernel vtables** (give them floor structs)
  to drop their `:delegate` facts too — `BYTECODE_METHOD_VT` needs care (it's on
  the method-dispatch fast path).
- **B3 (query projection)** — project the struct graph into EAVT/AVET for
  relational reflection over the floor (dispatch already needs no projection).
- **A2 (`:vtable` drop)** — `vtableEidOf` already reads the −1 slot first
  (projection exists); dropping the facts needs the same query-projection.
- **C2 (eid retrofit)** — N/A in the current shape (floor objects keep their
  kernel eids); revisit only for a genuinely pointer-identity-first boot.
- **`kernel-bench` gate** — hot path (mcache) is untouched; run to confirm.

### W7.0 — Instrument the boot path — ✅ DONE (2026-06-26)

`boot_probe.zig` (throwaway) + two nullable `FactStore.debug_on_write/_on_read`
hooks + a `bootstrap_test.zig` probe test (`zig build test-bootprobe`). Buckets
every boot write by attribute and flags every fact written-then-read-back before
module load.

**Measured (supersedes the ~250 estimate):** **324 writes** — reflection 215
(66%: `:name` 171 + `:vtable` 44) · metaobject graph ~50 (`:delegate` 42 + method
bindings) · fact-layer feature/config ~55. **The boot is essentially write-only:
exactly ONE read-before-mount** — `:promote-threshold` (config, Stage 11). `:name`,
`:vtable`, `:delegate` are read-before-mount **zero** times. The test guards this
with `totalRbm() == 1`. **Conclusion: A1, A2, and Track B carry no boot-ordering
risk** — much stronger than the ~46 ordering-sensitive writes first feared; the
graph cut (B) is about dispatch steady-state primacy, not boot timing. Delete the
probe (file + 2 fields + 2 call sites) once W7 lands.

### Track A — Defer the reflection shadow (~200 writes, mechanical)

| Item | Scope | Files | Size | Risk |
|---|---|---|---|---|
| **A1** | `:name` publication becomes lazy. Interner bijection is already fact-free (W6); stop eager `[eid :name "str"]` in `interner.seed`, record pending publications, flush at mount (C1). | `interner.zig`, `bootstrap.zig` | M | Low — gated by W7.0; the few boot-time `:name` readers convert to interner-map lookups. |
| **A2** | `:vtable`-of-a-vtable becomes a projection of the −1 header slot (eid → object_table pointer → read −1). Drop eager `:vtable` asserts for struct-backed vtables. | `send.zig` (`vtableEidOf`, ~:215), `object.zig` | M | Low — singleton-eid vtables (NIL/TRUE/FALSE/PEG/LANG, ~5) have no struct, so their `:vtable` facts **stay** (bucket 3). |

### Track C — The mount seam (co-develops with A)

| Item | Scope | Files | Size |
|---|---|---|---|
| **C1** | `mountIdentityLayer(store, table)`: (a) retrofit eids onto floor objects, (b) project the struct graph into EAVT/AVET (B3), (c) flush deferred `:name` (A1). Initially called right after the floor boots; the win is that it's a *named boundary*. | new `mount.zig`, `bootstrap.zig` | M |
| **C2** | Retrofit policy — eager-at-mount for the floor (small fixed set), lazy for user objects. | `mount.zig`, `object_table.zig` | S |

*Acceptance (A+C):* moving the `mountIdentityLayer` call **later** in boot still
yields a working, green system — the proof the fact layer is superimposed, not
baked.

### Track B — Struct-primary floor (the Q2 dispatch seam; B0 locked)

Relocates delegation + method dictionary from EAVT into the vtable struct so the
floor dispatches with no facts; the store projects them on mount.

| Item | Scope | Files | Size | Risk |
|---|---|---|---|---|
| **B1** | Extend the vtable struct (today 0 user slots, "fact-shaped") to carry the **B0** layout: `parent: Ref` + inline open-addressed `(sel→method)` array. | `object.zig`, `bootstrap.zig` (`allocVtStruct`) | M | Med — touches the object-header convention. |
| **B2** | `bind` / `vtableLookupImpl` cold path reads struct slots **first**, falls back to facts. **This is the seam.** | `send.zig` (:131/:143/:162/:178), `bootstrap.zig` (`vtableLookupImpl`) | M | **High — hot path. Mandatory `kernel-bench` gate.** |
| **B3** | On mount, project the struct graph into EAVT/AVET; keep coherent (struct authoritative, projection rebuilt). | `mount.zig`, `fact_store.zig` | M | Med |
| **B4** | Convert eager `:delegate` + `installMethod` asserts to struct-slot writes; facts now appear via B3. | `bootstrap.zig` | S | Low |

---

## W8 — Identity as Policy (parallel, secondary; D0 deferred)

**Goal / acceptance (D1–D3):** declare a candidate key on a user type; assert the
same key twice → the **same eid**; reflection/query see the entity; a uniqueness
violation returns a kinded `#duplicateKey`; the keyless case still mints
surrogates unchanged.

| Item | Scope | Files | Size | Gate |
|---|---|---|---|---|
| **D1** | Unfreeze identity: route minting through an `identityOf:` send with the **mechanical counter as default**. The minimal "identity is a policy" change; works on today's architecture. | `object_table.zig`, `send.zig`, `id_api.zig`, `kernel_eids.zig` (+`SEL_IDENTITY_OF`) | M | none — may ride with W7 |
| **D0** ⚠️ | **DEFERRED** — decide the protocol surface (`[T identityKey: #[…]]`, resolution hook points). | design | S | unblocks D2 |
| **D2** | Candidate-key policy: on mint-for-a-keyed-entity, consult the **AVET unique-index**; found → existing eid, not-found → mint + bind key. | new `identity.zig`, `fact_store.zig` | M | needs D0 |
| **D3** | eid-as-immutable-worldline discipline: key = public identity + uniqueness constraint; duplicate-key → kinded error; define key-mutability semantics (lean: **immutable keys first**, Date's default). | `identity.zig`, `fact_store.zig`, error model | M | needs D2 |
| **D4** | *(stretch/research)* object_table slot as **cache-of-resolution** (`docs/30` §4) — navigation routes through resolution, slot memoizes; kills OID-navigation at the user layer. | `send.zig`, `object_table.zig` | L | after D3 |

---

## Risks / gates (whole programme)

1. **B2 is the hot path.** `kernel-bench` before/after is mandatory; dispatch is
   the thing that cannot regress (`docs/18`). Mind the benchmarking-is-memory-
   dangerous harness caveat.
2. **W7.0 must precede A1.** Any `:name`/`:vtable` read-before-mount beyond the
   predicted set needs a targeted conversion first.
3. **D3 key-mutability** is a genuine semantic fork — "immutable keys only" vs
   "key change = identity migration." Decide explicitly; lean immutable-first.
4. **Test harness:** run each kernel test file directly with `</dev/null`;
   `zig build test` deadlocks in background runs.

## Relationship to prior plans

- **Subsumes** `docs/20a` Q2 (cuts #3/#4): Track B *is* the dispatch-seam
  resolution, now with the primacy call made (`docs/30` §6: struct-primary floor).
- **Adjacent to** ADR-image-1's "seed-as-data" move (ROADMAP §ADR-image-1): an
  embedded `.bim` seed could *replace* the imperative bootstrap entirely. W7 makes
  the floor self-sufficient on structs first; seed-as-data is a later unification
  (boot = load the seed image = load any image), not a prerequisite here.
