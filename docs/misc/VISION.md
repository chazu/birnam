# Birnam Vision

A **system** is a binary that runs as an executable OR loads as a library, and which exposes a complete, fully malleable computing environment in the COLA / idst / Piumarta tradition — built on a relational fact store so that history, reflection, queries, and snapshots collapse into one mechanism.

> **⚠️ Status (2026-07-08): partially superseded on the *primacy* axis.** This is
> still the richest statement of the design, but its ontology was **reversed** by
> the object-canonical pivot. Tenet 0 below ("the image is a database … everything
> else follows from this") and the Object Model section's "canonical form: fact-set
> in the store" are no longer the ground truth: the **object environment is
> canonical**, and the relational / fact-store reading is a *derived* mirror of it.
> Read [`docs/design/42-object-canonical-relational-derived.md`](../design/42-object-canonical-relational-derived.md)
> for the current direction. Treat the tenets below as the *mechanism* (still
> accurate — the fact store, indexes, no-NULL discipline, execution modes all
> hold) rather than the *ontology* (inverted).

## Core Tenets

0. **The image is a database.** Code, data, history, modules, and schema all live as facts `[entity attribute value tx op]` in persistent (HAMT/RRB) indexes. Everything else in this document follows from this.
1. **Everything is an object.** Vtables, selectors, modules, source, bytecode, method caches, history records — all are entities, all reachable via `send`.
2. **Bootstrap is minimal and visible.** A small kernel (id model + HAMT) brings itself up; everything else loads on top as modules-of-facts.
3. **Modules assemble the system at runtime.** A birnam image is a set of fact subsets tagged by module, not a monolith. Module load = merge facts. Module unload = retract.
4. **Live mutation is first-class.** Adding/replacing a method = asserting a new fact. The REPL is an object whose `run` method dispatches messages to other objects.
5. **History is free, prunable, and queryable.** Every assert/retract carries a tx. Time-travel is `as-of tx`. Pruning is excise. No bolt-on log.
6. **Reflection is query.** "What methods does X respond to?" "Who calls foo:?" "What changed between tx_a and tx_b?" — all Datalog.
7. **Three execution modes:** live image (full indexes + history + compiler), bytecode VM (portable, frozen indexes, no compiler), native AOT (stripped binary). Same source, same fact store, different switches. See §Execution Modes.
8. **Alien code is absorbable.** Any `.so`/`.dylib` can be loaded, introspected, and incrementally wrapped. Wraps are facts — persistent, queryable, diffable.

## Data Model

### Fact

```
[ e   a   v   tx  op  m ]
  |   |   |   |   |   |
  |   |   |   |   |   source module EID
  |   |   |   |   assert | retract
  |   |   |   monotonic u64
  |   |   typed: ref | int | str | bytes | keyword | bool
  |   attribute (also an entity, schema is in db)
  entity id (monotonic u64, decoupled from memory address)
```

The `m` (module) column makes module ownership a first-class property of every fact. Unloading a module = MEAT walk for `m = <module-eid>` (the module-keyed index), retract returned facts. Survives reload, cross-module overrides, and ownership transfers cleanly.

### Reification gap

The 6-column tuple records the **source** of each fact but does not make facts themselves into addressable entities. You cannot yet assert metadata *about* a fact (provenance beyond source-module, justification, belief markers, CRDT-merge annotations).

Full reification — facts as first-class entities with their own EIDs — is an **additive** future change. When it lands, the `m` column becomes a regular `[fact_eid :module foo]` assertion alongside whatever else needs saying about that fact. Until then, source-module is the only metadata captured.

This gap is acceptable because:
- Phase 9 (alien .so absorption) provenance fits in the `m` column (wrap origin = source module).
- Phase 10+ per-method-reload tracking fits in the `m` column.
- Belief/justification semantics are not in scope for the current roadmap.
- Path to full reification is non-breaking when we want it.

### Indexes

Build the covering set. Edge bidirectionality is architectural — on by default,
not a per-query afterthought.

- **EAVT** — entity walks (`what does this entity look like?`)
- **AEVT** — schema walks (`all instances of attribute X`)
- **AVET** — sort/uniqueness (`who has value V for attribute A?`)
- **VAET** — reverse-ref walks (`who points at this entity?`)
- **MEAT** — module-ownership walks (`every fact owned by module M`; the `m`
  column made a first-class index)

All indexes are persistent HAMTs. Structural sharing across tx → cheap snapshots.

**Not every fact pays for every index.** AST-node and bytecode metadata are
write-once and never reverse-ref-walked, so they are dropped from VAET and from
per-fact history — the Datomic `:db/index false` / `:db/noHistory` shape, applied
to code attributes. The bidirectional/full-history default holds for all *data*
facts; *code* facts shed only the index and the history that nothing reads. (Five
fact indexes, not four: MEAT joins the Datomic set. See the code-fact tiers in
`docs/11a-the-six-indexes.md` §7 and `docs/19a-code-fact-tiers-plan.md`.)

### Schema

Attributes are entities. Schema itself is facts. Self-describing. Selectors double as attributes — already interned, free.

Value primitives: `ref`, `int`, `str`, `bytes`, `keyword`, `bool`. Six. Adequate.

### No-NULL Discipline

Following Codd and CJ Date: the fact store contains no NULL values. Ever.

- **Absence of a fact** is how birnam models "no value known." Querying for an entity's attribute returns either a value or an empty result; never a NULL marker.
- **NIL the entity** (well-known EID 0) is a real, addressable object you can dispatch to (`[nil isNil] → true`). It is the singleton "nothing object," not absence.
- **NIL the value of a fact** — `[e :a NIL]` — is forbidden. Assert is a no-op in release, panics in debug, if asked to store a NIL in the `v` column.
- **Materialized slots** only exist for attributes that have asserted facts. There are no uninitialized slots, no "missing field" sentinels in object layout.

Why this is feasible (and cheap): SQL has NULL because tabular rows are fixed-shape. EAV fact stores have no fixed shape — an attribute is asserted or it isn't. Date's principle reinforces what the substrate already wants.

Why this is worth it: queries stay sound 2VL, diffs are exact (no spurious NULL→value transitions), reflection queries return only what was asserted, and the relational layer carries no hidden 3VL trap.

API discipline — **nil is a value; absence is not.** The sin is letting one token mean both (nil-as-not-found-sentinel = nil-as-NULL = the 3VL trap). So:
- **nil-the-object is freely storable and returnable as a value.** `{a: nil}` stores the EID-0 ref — a real 2VL object, allowed. A lookup that legitimately holds nil returns nil.
- **Absence rides a separate channel**, never a value: empty result (zero rows), an `ifAbsent:` block, or a raise. `at:ifAbsent:` is the primary keyed-access form; `at:` alone = `at:ifAbsent:[raise]` (absence is not a value, so refusing to manufacture one is the correct default). Because absence routes through `ifAbsent:` while nil stays a real value, **stored-nil vs absent remains exactly distinguishable** — that exactness is the Date payoff (sound diffs, no 3VL).
- Out-of-bounds on a sequence is a domain error (index outside `0..size`), not absence-of-fact → raise; `at:ifAbsent:` is the checked variant. No nil sentinel.
- Zig API surface uses `?Ref` for the `not-found` case — already the HAMT's contract; the `null` there is Zig's optional, not a stored fact value.

### Persistent Collections

Two persistent structures form the kernel substrate:

- **HAMT** (Bagwell 2000 / Steindorfer's CHAMP shape) — all fact indexes, all maps/sets, the object table. 32-way branching, popcount-bitmap-compressed nodes.
- **RRB-Tree** (Bagwell & Rompf 2011) — all sequence-shaped values: vectors, ropes, buffers, source text, history-log random access. Clojure/Scala/Immer-proven. O(log₃₂) lookup/update/concat/split.

No bespoke structures (no Ents, no enfilades). Two well-understood, battle-tested persistent structures everywhere. ~500 LOC each in Zig.

## Object Model

### Identity and References

Object identity = **EID** (monotonic u64), indexing into an **object table** (HAMT-shaped, stable-sparse — same shape as a page table). Decoupled from memory address. Buys compaction, relocation, snapshot relocation, AOT freeze.

A **reference** is one machine word with a tagged-pointer encoding. Low 3 bits = tag:

| Tag | Meaning | Use |
|-----|---------|-----|
| `000` | raw pointer to materialized libid struct | hot objects |
| `001` | SmallInteger (idst tradition) | unboxed ints |
| `010` | EID (cold object — query the fact store) | demoted objects |
| `011` | keyword / immediate symbol | future |
| `100-111` | reserved | future immediates |

Pointer alignment is 8-byte on 64-bit → low 3 bits free. Standard Smalltalk/idst tagging extended with one code for EID.

### The -1 Trick (Preserved)

For any materialized object (tag `000` reference), `obj[-1]` is the vtable. One backward indirection, no header word, no envelope. Object **IS** its slot vector. Piumarta's bargain intact.

Cold objects have no slot vector — they are facts in the store. The equivalent surface for "poke any bit" is "assert/retract any fact." Same power, different shape.

### Promotion / Demotion

- **Canonical form**: fact-set in the store. Always valid.
- **Materialized form**: libid struct allocated, slots laid out as uniform `[Ref]`, vtable at -1. Reference becomes a raw pointer.
- **Promotion**: triggered by hot + stable-shape heuristic (Phase 8+; Phase 4 ships manual only). Allocation/layout mechanics live in the Zig kernel (frozen).
- **Demotion**: triggered by shape mutation, snapshot, pruning, or memory pressure. Struct freed, reference re-tagged as EID.

### Materialized Layout (Uniform-Ref)

User-defined objects materialize as `[vt][Ref_0][Ref_1]...[Ref_{N-1}]` with vtable at offset -1.

- N = attribute count for that object's shape.
- Slot order = attributes sorted by EID (deterministic, no per-instance metadata table).
- Every slot is one tagged word (`Ref`). SmallInts stored inline via tag `001`; bytes/strings as Ref to heap-entity.
- Shape (attribute list) derivable from the vtable's facts — no separate shape-table cache.
- Slot access = one indirect load. Bytecode: single `slot_at <idx>` opcode.
- Shape mutation = grow/shrink the slot array + re-assert facts. No layout-cache invalidation.

The kernel structs (Vtable, Closure, Selector, Vector, Assoc) keep their fixed extern-struct C-ABI shapes — they are the only fixed-layout materializations. User-defined objects are uniform-Ref.

FFI interop with alien C code is wrap-and-marshal (Phase 9 shims), not direct struct-memory exposure, so the loss of typed-slot packing doesn't cost interop.

### Dispatch

`bind(receiver, selector)` resolves via:

1. **Hot path**: tag-check reference; if raw pointer, load `vt = obj[-1]`; check mcache for `(vt, sel) → method`; tail-call.
2. **Cold path**: cache miss or EID reference → query EAVT for `[receiver_vt selector ?method]`; populate mcache; dispatch.
3. **Mcache invalidation**: every assert/retract on `(vt, sel)` invalidates the entry.

Two-tier dispatch: pointer-fast for hot stable code, query-uniform for everything else. ~1 cycle for the tag-check, ~5ns mcache hit, ~30ns cold-path index walk. Smalltalk-VM ballpark.

## Execution Modes

| Mode | Use | Indexes | History | Compiler | Parser/AST |
|------|-----|---------|---------|----------|-----------|
| **Mode 1 — dev** | Development, REPL, exploration, source compile | full | full | yes | yes |
| **Mode 2 — AOT (default)** | AOT-baked image, full malleable tower kept | full | full | yes | yes |
| **Mode 2 + `--strip=malleable`** | AOT, no source compile at runtime; method install via dylib still works | full | full | no | no |
| **Mode 2 + `--strip=history`** | AOT, time-travel queries dropped, indexes RO | RO | none | yes | yes |
| **Mode 2 + both strips** | Smallest end-user AOT binary, pure VM over frozen facts | RO | none | no | no |

Mode 2 sub-variants: `--aot=static` (kernel + modules + data all linked into one executable) and `--aot=dynamic` (kernel + data baked; modules `dlopen`ed at runtime from baked manifest). Both work with or without strips.

All modes share the same fact store, object model, bytecode format, and `.bim` data layout. AOT = wrap-Mode-1-image-as-executable; same data bytes underneath.

## Architecture

```
+--------------------------------------------------------+
|                       shell (exe)                      |
|   - loads modules, wires export/import table           |
|   - file watcher (dev only)                            |
+-----------------------+--------------------------------+
                        |
   +--------------------+--------------------+
   |        id module (libbirnam_id)         |   <- bootstrap kernel
   |  HAMT, fact store, indexes, tx engine,  |
   |  vtable+send/bind+mcache, selectors,    |
   |  prototype object, allocation           |
   +--------------------+--------------------+
                        |
   +------+------+------+------+------+------+
   |      |      |      |      |      |      |
  peg  query  language  vm    ffi  image   repl
  AST  pat-   parser+   exec  alien snap+  object
  rules sends interp           .so  history
```

Every module imports `id_api` from the kernel and exports its own API. Modules can be replaced live (retract+assert in one tx).

## Memory

- **Arenas for index segments.** Persistent structures allocate into per-segment arenas (named: index, strings, boxes, bytecode, captures, struct, sig). Old epochs dropped wholesale on prune. *(Substrate built in Phase 12.5 / Phase 15 — pre-Phase-15 the kernel allocates everything from one GPA; the segmented arenas are the Phase 15 reclamation substrate.)*
- **Materialized struct arena**: separate region for promoted objects. Demotion frees individual structs (`promotion.demote` — the one reclaim path that exists pre-Phase-15); full reset on snapshot reload.
- **Three reclamation concerns, not one — and only ONE of them is "GC."** Treating the whole story as garbage collection is the trap: it works for box-heap but mis-frames the fact layer, which is a temporal database, not an object graph.
  - **Value-heap reclaim (GC, Phase 15 — shipped core).** Real garbage collection of language *values*: float/ffiptr boxes, strings, closures, FFI Sigs. Reachability **is** liveness here — these are ordinary objects. Non-moving mark-sweep over per-type arenas; honors FFI pinning (`ffi_pause`). This is the only layer where "GC" is the right word.
  - **Index-structure reclaim (epoch arena, Phase 15.I).** Pure plumbing, no user semantics. The persistent HAMT/RRB path-copies on every mutation, abandoning old spine nodes; `compact()` rebuilds the index fresh. **Epoch model:** index nodes allocate into an arena epoch; a rebuild swaps to a new epoch and frees the old one *wholesale* — never reason about per-node structural sharing. **Critical:** the free is gated on the **oldest retained basis-T** (see retention, below), not unconditional — an epoch is freed only when no retained snapshot still references it. Today (current-only) that reduces to "free on compact," but the gate is what keeps cheap time-travel from being silently foreclosed.
  - **Fact/history retention (policy, NOT GC).** Which *facts* survive is a temporal/semantic decision, not a reachability one. History accretes; `prune` is "collapse history before basis-T," a user-meaningful op (Datomic-shaped). See §Retention.
  - Persistent indexes + retention remove most value-heap *pressure*; they do not replace value-heap GC. Earlier drafts said "No GC" — that referred only to the logical/fact layer and was superseded when sustained-live-image use made value-heap reclaim mandatory (2026-06-03). The "GC for a database" mislabel was corrected 2026-06-05.

### Retention, snapshots, excision (the Datomic bargain)

The persistent substrate's superpower is that **an index root pointer IS a complete zero-copy snapshot of the whole store at its tx** (structural sharing). Reclamation must not throw this away. So the fact-layer model is retention + snapshot-pinning + excision, **tiered so it stays invisible until you reach for history:**

- **Default (current-only): invisible.** A user who only wants "now" never sees retention. History collapses continuously (auto-prune cadence), index epochs free on compact, value-heap GC runs in the background. No DBA surface. This is today's behavior.
- **Snapshots are values.** Capturing a db-value (`as-of tx`, or `db now`) yields an immutable handle that **pins the storage it needs** — exactly like any held object pins its heap. A retained snapshot holds its index epoch alive; the epoch-arena free skips any epoch at-or-after the oldest retained basis-T. Reachability-of-roots, not reachability-of-nodes.
- **Retention horizon (policy, opt-in):** keep-last-N-txs / keep-since-T / keep-reachable-from-roots. Sets the floor below which history is collectible. Surfaced only when a user opts into time-travel.
- **Excision (explicit, rare):** hard-delete of named entities or tx ranges (GDPR-shaped). Distinct from prune — it removes facts *within* the retained window, on purpose, and is the one op that can invalidate held snapshots. Always user-invoked, never automatic.
- **On-disk cold-history tier (runtime).** History older than the live retention horizon but still inside the retention *policy* spills to disk **during runtime**, not just at image-save: warm/current indexes stay in memory; cold tx epochs are written to segment files and dropped from RAM, faulted back only when an `as-of` query reaches that far. Keeps a long-lived image's resident set bounded by the working set, not by total history. The single-file page-aligned image layout (§Image) is the substrate; segments are epoch-granular so a cold epoch is one contiguous write/evict.

- **AOT prune = the most aggressive variant:** collapse all history into a single latest-tx snapshot, drop the `tx` and `op` columns, freeze indexes into read-only segments, demote all objects to EID form (or pre-materialize all-and-freeze, depending on AOT mode). This is retention-horizon = "now" made permanent.

## Bootstrap

1. Zig kernel allocates raw HAMT + RRB-Tree machinery (no facts yet).
2. Initialize object table (HAMT u64 → ref). Initialize fact store: empty EAVT/AEVT/AVET/VAET indexes, tx counter = 0.
3. Assert seed facts: schema entities for `lookup:`, `methodAt:put:with:`, etc. at well-known EIDs.
4. Assert vtable_vt, object_vt, selector_vt, closure_vt, vector_vt as entities with `[vt sel method-ptr]` facts.
5. Pre-materialize the kernel vtables (they will be hot forever) — install libid structs in the materialized arena, register raw-pointer references in the object table.
6. Wire `bind` to: tag-check → mcache → EAVT query. Enable `send`.
7. Smoke test: `send(metavtable, lookup:, sel) → method` via the new path.
8. Hand off to module loader. id_api exports `assert`, `retract`, `query`, `intern`, `send`, `bind`, `promote`, `demote`.

HAMT + RRB + fact store must work before the first method install. Kernel substrate ~1500 LOC of frozen Zig. One time.

## Query Surface — Pattern Sends

Datalog disappears into the message-send syntax. A clause IS a bracket-literal send-form containing one or more pattern variables.

- `[recv sel: arg]` — normal send, evaluated.
- `[?vt lookup: ?m]` — any `?var` present → quoted clause for the query engine, not evaluated.
- Multi-arity: `[?p add: ?x to: ?y]` is a three-place clause.

Query example:

```
[db findAll: ?m where: #(
    [?vt lookup: ?m]
    [?vt name: 'Point']
)]
```

### Reserved prefixes / literals

| Form | Meaning |
|---|---|
| `?name` | pattern variable (query) |
| `:name` | block argument |
| `#foo` | literal selector / keyword |
| `"foo"` | string literal |
| `#( ... )` | literal array |

### Why

One syntax for code and queries. No special Datalog reader. Selectors double as attributes (already locked); a triple `[e a v]` already reads as `[e a: v]`. Reflection becomes "send-with-holes."

### Negation / aggregation

Keyword selectors on `Query` or `db`, conventional Smalltalk:

```
[db count: ?m where: #( [?vt lookup: ?m] )]
[db findAll: ?vt where: #( [?vt name: ?n] ) notHaving: #deprecated]
```

Designed in detail when Phase 7 starts; the surface syntax is locked.

## Image and Snapshot

- **Image** = serialized index roots + segment files + tx counter + interned-attribute table + object table.
- **Snapshot** is cheap: persistent structures share with prior snapshots; only new segments written.
- **A snapshot is a first-class value, and holding it pins its storage.** Capturing `as-of tx` / `db now` yields an immutable db-value = the index roots at that tx. While held, its index epoch(s) are reachable, so the epoch-arena free (§Memory) skips them and the cold-history tier keeps (or faults in) their segments. Dropping the last reference to a snapshot makes its now-unreferenced epochs collectible at the next compact. This is the reachability question for the fact layer — over retained *roots*, not over nodes — and it is what makes the index-structure reclaim safe in the presence of time-travel.
- **Load** = restore roots, re-establish mcache (empty, lazy fill). *(Today replay-based — facts re-asserted into fresh indexes on load. The mmap-segment fast path is a Phase 12.5 perf optimization gated on data, not yet built; the single-file page-aligned layout already supports it later.)*
- **A full image IS the booted system (W1).** The default boot is an automatic **compile-cache**: a valid full image (every fact, gated by a kernel-ABI-+-source `boot_validity` hash) is loaded instead of recompiling the tower; a miss recompiles and writes a fresh cache. `m` is a *provenance tag*, not a persistence gate — full images carry every `m`; the legacy M_USER-only save remains for lightweight user-data overlays. Native modules relink their fn-ptrs onto image-restored eids **by interned name** (the durable anchor). See [docs/2e](../archive/legacy-kernel/2e-images-aot.md).
- **Module unit** is the `m` column on every fact (see §Fact). A module is the set of facts with a given `m` EID. No separate per-image module-bookkeeping required.

## Compilation Path

Source → PEG parse → AST facts → desugar (asserts more facts) → IR facts → bytecode facts.

- VM consumes bytecode facts directly.
- AOT = bytecode VM + frozen image, packaged as standalone binary.
- Native lowering = **birnam → C → `libtcc`** for live mode 1 (re-JIT on redefine = malleable AND native), `zig cc`/`clang` for the mode-3 ship — one codegen pass, two compilers (decision locked in [docs/10a](../archive/legacy-kernel/10a-north-star-cell-model.md); supersedes the earlier "no tinycc" stance). Native code is a *cache of bytecode*: the `:bytecode-stream` fact stays canonical, native is regenerated on load/redefine.
- Compiler is a birnam module. Self-hosts when PEG grammar for birnam parses itself.

## Alien Code Absorption

```
> mod := Module load: "/usr/lib/libfoo.dylib"
> mod symbols                    " enumerate Mach-O/ELF symtab "
> mod wrap: 'foo_open' as: #Foo>>open
> mod wrap: 'foo_read' as: #Foo>>readInto:size: signature: #(int ptr int)
> Foo new open readInto: buf size: 1024
```

- Symbol enumeration via bundled Mach-O + ELF readers (hand-rolled, not libelf).
- Unknown-signature calls via dyncall or libffi.
- Each wrap = a fact. Persists across snapshot. Queryable. Diffable.

## REPL

REPL is an object. Its `run` method reads a line, parses to a message send, dispatches to a target object. Shell-level REPL is bootstrap front door until the REPL module loads.

Syntax: idst/Smalltalk-derived, visually Obj-C-y. Bracket-delimited message sends, keyword selectors, unary/binary/keyword forms, block closures, pattern-sends for queries (see §Query Surface — Pattern Sends). Statement separator `;`. No cascades (dropped Phase 10 decision; multi-send-to-same-receiver via repeated `[obj msg]` sends). Reserved prefixes and literal forms are locked. Full grammar (precedence, operator set, literal extensions) is designed in Phase 10 (language module) as PEG facts.

## Build Outputs

- `birnam` — shell executable (Mode 1 host: loads kernel + dylibs, runs REPL or `-M main.dylib --entry=...`)
- `.brn` — source file extension
- `libbirnam_<module>.dylib` / `.so` / `.dll` — runtime modules (platform-native suffix)
- `<name>.bim` — image archive (Mode 1 save format; `roots.bin` + `segments/` + `manifest.txt` with dylib deps + thread-mode tag). Pure data — saving requires no `cc`.
- `<app>` — AOT-compiled executable (`birnam --aot=static|dynamic [--strip=malleable] [--strip=history] image.bim → <app>`)

## Non-Goals (For Now)

- Multi-writer / distributed images. Single-process, single-writer. All txs serialized through one channel.
- Threads / actors / concurrency. **Deferred, not refused. Target shape locked in ADR-11.** Multi-threaded operation is on the table eventually; nothing post-Phase-11 should foreclose it. Locked target: **Datomic shape** — one designated transactor thread owns the tx engine; reader threads access persistent indexes lock-free; each thread carries its own `Vm` struct (call stack, `pc`, `current_self`, `unwinding` flag, recursion guard, FFI pause counter, bump-arena slot); mcache shared with atomic 16-byte writes (race-tolerant — mismatch falls through to `bind`); EID allocator + tx counter atomic monotonic; selector intern table single-writer + atomic root pointer. Image manifest carries `mode: single|datomic` so the format survives transactor introduction. Phase 11 ships single-threaded but with all seven keep-viable items in place.
- Sandboxing.
- Beating C performance. Smalltalk-VM ballpark is fine.

## Malleability Boundary

The **frozen layer** (kernel, Zig, ~1500 LOC pre-Phase-11; ~2400 LOC post-Phase-11 per ADR-11 addendum re-estimate; ~3200 LOC at Phase 14) contains: HAMT, RRB-Tree, fact store, four indexes, object table, tx engine, tag/untag, allocation, materialized-struct arena, dispatch mechanics, mcache primitives. **Phase 11 adds:** bytecode bytestream walker, ~20 opcode handlers (tail-call dispatch via `@call(.always_tail, ...)`), atomic mcache (16-byte stores), per-thread `Vm` struct, atomic EID/tx counters. To change any of these requires rebuilding the kernel — same constraint Piumarta had at the C boundary, just at a different boundary.

The **malleable tower** (everything above) is fact-shaped and live-replaceable: vtables, methods, dispatch policy (`bind` itself is a method), method-cache strategy, promotion/demotion heuristics, schema, parser, AST, IR, bytecode, compiler, VM, modules, REPL, history policy, pruning policy.

The metacircular property holds: you can rewrite `bind` while the system runs. You can replace the compiler with one written in birnam. You can edit the grammar that parses your source. The substrate boundary moves up vs. Piumarta — the malleable layer is correspondingly richer (relational + historical, not just structural).

## Open Questions

- ~~Datalog surface syntax.~~ Resolved: pattern-sends (see §Query Surface).
- ~~Module unit.~~ Resolved: 6-column tuple with `m` = source module EID (see §Fact). Full fact-as-entity reification deferred.
- Pruning UX. Defaults? Per-attribute retention? Decide when history actually grows uncomfortable.
- ~~Selector identity across image load.~~ Resolved: kernel EIDs (0..255) as hardcoded constants in `kernel_eids.zig`; user EIDs allocate from 256+; renumbering the kernel block is a breaking image-format change. (Originally 0..63; bumped to 0..255 in Phase 9.5.5 — no images existed yet, expansion slots for Phase 10+ language / 11+ bytecode / 12+ image work.)
- Promotion/demotion heuristics: deferred. Phase 4 ships with manual `promote`/`demote` primitives in IdApi, no counter. **Reopens at start of Phase 8**, informed by Phase 7 (first heavy cold-dispatch workload via query module) and Phase 10 (first user-language workload). Decide then between kernel-tracked counters vs. policy-module-owned bookkeeping, with real dispatch traces in hand. Likely shape: per-EID counter incremented on cold dispatch, policy module queries + promotes via additive IdApi extension.
- ~~FFI boundary pinning.~~ Resolved: whole-image demotion pause via single kernel flag (counter for nested FFI). Single-threaded image makes per-object tracking unnecessary. Promoted objects auto-pinned for FFI-call duration. Decided in Phase 9; primitive lives in Phase 8 kernel surface as `ffi_enter` / `ffi_exit`.
- ~~Materialized struct layout.~~ Resolved: uniform-Ref slots (see §Materialized Layout). Kernel structs keep fixed C-ABI shape; user objects use `[Ref_0..Ref_{N-1}]` slot arrays ordered by attribute EID.
- ~~Tx log structure.~~ Resolved: reuse RRB-Tree. No third kernel primitive.
- ~~Recursion guard for bind.~~ Resolved: single global guard counter. Single-threaded image. Becomes thread-local if/when concurrency lands.

## Roadmap

See `docs/archive/ROADMAP.md` (archived — a historical phase ledger through Phase 15) for the phase-by-phase work sequence and reasoning. This document is the design; the roadmap was the original implementation plan.
