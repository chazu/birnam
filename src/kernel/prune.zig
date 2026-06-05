//! Phase 13 — pruning / reachability GC.
//!
//! Reachable-from-roots policy (13.1) over the bytecode-closure + provenance
//! model (roots/edges re-derived 2026-06-02, see ROADMAP Phase 13):
//!
//!   - Prune scope is **M_USER only**. CODE facts (bootstrap + module-init
//!     installs: kernel vts, primitives, grammar rules) are reconstructed
//!     every boot, so they are never swept.
//!   - Roots: the stable top-level REPL scope (`TOP_SCOPE_EID`); installed
//!     methods (any M_USER fact whose value is a `:bytecode-stream` entity —
//!     e.g. `[vt sel mkEid(method)]`); saved queries; M_USER FFI wraps.
//!   - Marking follows ALL eid-valued facts forward from each live entity
//!     (constant-pool block children, `:method-body-ast`/`:range-ast` AST
//!     debug subtree, `:scope-first-binding`/`:bind-next`/`:bind-value`
//!     chains, …). Conservative: over-marking keeps maybe-garbage; it never
//!     drops a live entity.
//!
//! Sweep = `FactStore.compact` (13.2): rebuild indexes keeping CODE facts +
//! marked M_USER facts; history collapses; object-table entries for swept
//! entities are cleared.
//!
//! Phase 15.4: swept entities are `promotion.demote`d before their slot is
//! cleared, so a materialized struct's heap is reclaimed (clearing the slot
//! alone leaked it — demote can't run once the slot is gone).

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const promotion = @import("promotion.zig");

const Ref = ref_mod.Ref;
const FactStore = fact_store.FactStore;
const ObjectTable = object_table.ObjectTable;
const MarkSet = std.AutoHashMap(u64, void);

pub const PruneError = error{OutOfMemory};
pub const PruneStats = struct { kept: usize, dropped_facts: usize, swept_entities: usize };

fn seed(marked: *MarkSet, work: *std.ArrayList(u64), a: std.mem.Allocator, eid: u64) !void {
    const g = try marked.getOrPut(eid);
    if (!g.found_existing) try work.append(a, eid);
}

/// Mark every entity reachable from the live roots.
fn markReachable(store: *const FactStore, scratch: std.mem.Allocator) PruneError!MarkSet {
    var marked = MarkSet.init(scratch);
    var work = std.ArrayList(u64){};
    defer work.deinit(scratch);

    // Root 1: stable top-level REPL scope.
    try seed(&marked, &work, scratch, kernel_eids.TOP_SCOPE_EID);

    // Root 2: installed methods — an M_USER fact whose value is a
    // bytecode-method eid (e.g. [vt sel mkEid(method)]) keeps both the
    // method AND its subject (the vt holding the install) alive, so the
    // install fact itself survives the sweep even when the subject is a
    // kernel vt that is otherwise not reachable from a data root.
    var it = store.meatIter();
    while (it.next()) |f| {
        if (f.m != kernel_eids.M_USER) continue;
        if (ref_mod.isEid(f.v)) {
            const t = ref_mod.untagEid(f.v);
            if (store.currentValue(t, kernel_eids.ATTR_BYTECODE_STREAM) != null) {
                try seed(&marked, &work, scratch, t);
                try seed(&marked, &work, scratch, f.e);
            }
        }
    }

    // Roots 3 + 4: saved queries, FFI wraps.
    var sq = store.entitiesWith(kernel_eids.ATTR_SAVED_QUERY_SPEC);
    while (sq.next()) |e| try seed(&marked, &work, scratch, e.e);
    var ff = store.entitiesWith(kernel_eids.ATTR_FFI_SIG);
    while (ff.next()) |e| try seed(&marked, &work, scratch, e.e);

    // BFS: follow every eid-valued fact forward.
    while (work.pop()) |eid| {
        var ai = store.attributesOf(eid);
        while (ai.next()) |av| {
            if (ref_mod.isEid(av.v)) try seed(&marked, &work, scratch, ref_mod.untagEid(av.v));
        }
    }
    return marked;
}

// -- Phase 14 M2: AST data strip (--strip=malleable) ------------------------

fn isAstAttr(a: u64) bool {
    return a == kernel_eids.ATTR_METHOD_BODY_AST or a == kernel_eids.ATTR_METHOD_AST_RANGE;
}

fn keepNonAst(astSet: *MarkSet, m: u64, e: u64, a: u64, v: Ref) bool {
    _ = v;
    if (m != kernel_eids.M_USER) return true; // CODE untouched
    if (astSet.contains(e)) return false; // AST node / range entity
    if (isAstAttr(a)) return false; // method -> AST debug back-ref
    return true;
}

/// Drop the user's parsed AST trees + bytecode→AST debug back-refs from the
/// store (Phase 14 M2). A pure-VM (`--strip=malleable`) binary runs bytecode,
/// never the AST, so these are dead weight. Methods keep their
/// `:bytecode-stream`/`:constant-pool`. Returns facts dropped.
pub fn stripAst(
    store: *FactStore,
    table: *ObjectTable,
    scratch: std.mem.Allocator,
) PruneError!usize {
    // AST nodes carry :ast-byte-start; range entities carry :range-method.
    var astSet = MarkSet.init(scratch);
    defer astSet.deinit();
    var it = store.meatIter();
    while (it.next()) |f| {
        if (f.m != kernel_eids.M_USER) continue;
        if (f.a == kernel_eids.ATTR_AST_BYTE_START or f.a == kernel_eids.ATTR_RANGE_METHOD)
            try astSet.put(f.e, {});
    }
    // Clear object-table slots for AST entities (rarely materialized, but
    // be safe), then rewrite.
    var keys = astSet.keyIterator();
    while (keys.next()) |e| {
        promotion.demote(e.*); // 15.4: free a materialized AST struct, if any
        table.clear(e.*) catch {};
    }

    const cs = store.compact(scratch, &astSet, keepNonAst) catch return error.OutOfMemory;
    return cs.dropped;
}

fn keep(marked: *MarkSet, m: u64, e: u64, a: u64, v: Ref) bool {
    _ = a;
    _ = v;
    if (m != kernel_eids.M_USER) return true; // CODE: reconstructed, never pruned
    return marked.contains(e);
}

/// Reachable-from-roots prune. Sweeps unreachable M_USER entities' facts and
/// collapses history; clears the object table for swept entities.
pub fn pruneReachable(
    store: *FactStore,
    table: *ObjectTable,
    scratch: std.mem.Allocator,
) PruneError!PruneStats {
    var marked = try markReachable(store, scratch);
    defer marked.deinit();

    // Collect M_USER entities that will be swept (current facts whose entity
    // is unmarked) so their materialized structs can be dropped too.
    var swept = std.ArrayList(u64){};
    defer swept.deinit(scratch);
    var seen = MarkSet.init(scratch);
    defer seen.deinit();
    var it = store.meatIter();
    while (it.next()) |f| {
        if (f.m != kernel_eids.M_USER or marked.contains(f.e)) continue;
        const g = try seen.getOrPut(f.e);
        if (!g.found_existing) try swept.append(scratch, f.e);
    }

    const cs = store.compact(scratch, &marked, keep) catch return error.OutOfMemory;
    // Phase 15.4: demote each swept entity BEFORE clearing its slot, so its
    // materialized struct heap is reclaimed. `table.clear` alone leaks the
    // struct — and `demote` can't run afterward (it reads `table.get(eid)`,
    // already gone). `demote` is a no-op when promotion is uninitialized or
    // the entity was never materialized; `table.clear` is the fallback for
    // the cold (EID-slot) case demote leaves untouched.
    for (swept.items) |e| {
        promotion.demote(e);
        table.clear(e) catch {};
    }

    return .{ .kept = cs.kept, .dropped_facts = cs.dropped, .swept_entities = swept.items.len };
}
