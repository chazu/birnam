const std = @import("std");
const fs = @import("fact_store.zig");
const ref = @import("ref.zig");
const testing = std.testing;

// Convenience: build a value Ref from a small int.
fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

// Module IDs we use across tests
const M_BOOT: u64 = 1;
const M_FOO: u64 = 2;
const M_BAR: u64 = 3;

// Entity / attribute IDs
const E1: u64 = 100;
const E2: u64 = 101;
const E3: u64 = 102;
const A_NAME: u64 = 200;
const A_AGE: u64 = 201;
const A_PARENT: u64 = 202;

// -- Basic assert / lookup ----------------------------------------------------

test "assert + currentValue round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const tx = try store.assert(E1, A_NAME, vi(42), M_BOOT);
    try testing.expectEqual(@as(u64, 1), tx);
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
}

test "currentValue null for never-asserted fact" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
}

test "tx counter monotonic across assert/retract" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    const t1 = try store.assert(E1, A_NAME, vi(1), M_BOOT);
    const t2 = try store.assert(E2, A_NAME, vi(2), M_BOOT);
    const t3 = try store.retract(E1, A_NAME, vi(1), M_BOOT);
    try testing.expect(t1 < t2);
    try testing.expect(t2 < t3);
}

// -- Retract -----------------------------------------------------------------

test "retract removes current value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    try testing.expectEqual(vi(30), store.currentValue(E1, A_AGE).?);
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_AGE));
}

test "retract of never-asserted is no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.retract(E1, A_NAME, vi(999), M_BOOT);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
}

test "retract of wrong-value is no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.retract(E1, A_AGE, vi(99), M_BOOT); // wrong value
    try testing.expectEqual(vi(30), store.currentValue(E1, A_AGE).?);
}

// -- Supersession ------------------------------------------------------------

test "assert v2 over v1: current becomes v2" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    try testing.expectEqual(vi(31), store.currentValue(E1, A_AGE).?);
}

test "history walk via as-of-tx" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    const tx1 = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    const tx2 = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    const tx3 = try store.retract(E1, A_AGE, vi(31), M_BOOT);

    try testing.expectEqual(@as(?ref.Ref, null), store.currentValueAsOf(E1, A_AGE, 0));
    try testing.expectEqual(vi(30), store.currentValueAsOf(E1, A_AGE, tx1).?);
    try testing.expectEqual(vi(31), store.currentValueAsOf(E1, A_AGE, tx2).?);
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValueAsOf(E1, A_AGE, tx3));
}

// -- Four-index agreement on current state ------------------------------------

test "EAVT and AEVT agree on current state" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(100), M_BOOT);
    _ = try store.assert(E2, A_NAME, vi(200), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);

    // EAVT-shaped: attributes of E1
    var attrs = store.attributesOf(E1);
    var seen_name = false;
    var seen_age = false;
    while (attrs.next()) |kv| {
        if (kv.a == A_NAME and kv.v == vi(100)) seen_name = true;
        if (kv.a == A_AGE and kv.v == vi(30)) seen_age = true;
    }
    try testing.expect(seen_name);
    try testing.expect(seen_age);

    // AEVT-shaped: entities with A_NAME
    var entities = store.entitiesWith(A_NAME);
    var saw_e1 = false;
    var saw_e2 = false;
    while (entities.next()) |kv| {
        if (kv.e == E1 and kv.v == vi(100)) saw_e1 = true;
        if (kv.e == E2 and kv.v == vi(200)) saw_e2 = true;
    }
    try testing.expect(saw_e1);
    try testing.expect(saw_e2);
}

test "AVET equality lookup" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E2, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E3, A_NAME, vi(8), M_BOOT);

    var it = store.entitiesMatching(A_NAME, vi(7));
    var seen_e1 = false;
    var seen_e2 = false;
    var count: usize = 0;
    while (it.next()) |e| {
        if (e == E1) seen_e1 = true;
        if (e == E2) seen_e2 = true;
        count += 1;
    }
    try testing.expectEqual(@as(usize, 2), count);
    try testing.expect(seen_e1);
    try testing.expect(seen_e2);
}

test "VAET reverse-ref walk" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const parent_ref = ref.mkEid(E3);
    _ = try store.assert(E1, A_PARENT, parent_ref, M_BOOT);
    _ = try store.assert(E2, A_PARENT, parent_ref, M_BOOT);

    var it = try store.pointingAt(parent_ref, arena.allocator());
    defer it.deinit();
    var seen_e1 = false;
    var seen_e2 = false;
    while (it.next()) |ae| {
        try testing.expectEqual(A_PARENT, ae.a);
        if (ae.e == E1) seen_e1 = true;
        if (ae.e == E2) seen_e2 = true;
    }
    try testing.expect(seen_e1);
    try testing.expect(seen_e2);
}

// -- hasFact -----------------------------------------------------------------

test "hasFact reflects current state" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    try testing.expect(store.hasFact(E1, A_AGE, vi(30)));
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
}

test "supersession: prev value's hasFact becomes false" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(31), M_BOOT);
    try testing.expect(!store.hasFact(E1, A_AGE, vi(30)));
    try testing.expect(store.hasFact(E1, A_AGE, vi(31)));
}

// -- entityExists ------------------------------------------------------------

test "entityExists true after assert, false after full retract" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    try testing.expect(!store.entityExists(E1));
    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    try testing.expect(store.entityExists(E1));
    _ = try store.retract(E1, A_NAME, vi(7), M_BOOT);
    try testing.expect(!store.entityExists(E1));
}

// -- Module column -----------------------------------------------------------

test "facts owned by different modules are independently queryable" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(1), M_FOO);
    _ = try store.assert(E2, A_NAME, vi(2), M_FOO);
    _ = try store.assert(E3, A_NAME, vi(3), M_BAR);

    var foo_it = try store.factsOfModule(M_FOO, arena.allocator());
    defer foo_it.deinit();
    var foo_count: usize = 0;
    while (foo_it.next()) |_| foo_count += 1;
    try testing.expectEqual(@as(usize, 2), foo_count);

    var bar_it = try store.factsOfModule(M_BAR, arena.allocator());
    defer bar_it.deinit();
    var bar_count: usize = 0;
    while (bar_it.next()) |_| bar_count += 1;
    try testing.expectEqual(@as(usize, 1), bar_count);
}

test "module unload: walk MEAT, retract each (e, a, v) — fact disappears" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    _ = try store.assert(E1, A_NAME, vi(1), M_FOO);
    _ = try store.assert(E1, A_AGE, vi(30), M_FOO);
    _ = try store.assert(E2, A_NAME, vi(2), M_BAR);

    // Simulate module unload for FOO.
    var to_retract = std.ArrayList(fs.FactStore.ModuleFactIter.EAV){};
    defer to_retract.deinit(arena.allocator());
    var it = try store.factsOfModule(M_FOO, arena.allocator());
    defer it.deinit();
    while (it.next()) |triple| try to_retract.append(arena.allocator(), triple);
    for (to_retract.items) |t| _ = try store.retract(t.e, t.a, t.v, M_FOO);

    // FOO's facts gone.
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_NAME));
    try testing.expectEqual(@as(?ref.Ref, null), store.currentValue(E1, A_AGE));
    // BAR's fact untouched.
    try testing.expectEqual(vi(2), store.currentValue(E2, A_NAME).?);
}

// -- attributesOf / entitiesWith reflect supersession + retract ---------------

test "attributesOf does not include retracted attributes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());
    _ = try store.assert(E1, A_NAME, vi(7), M_BOOT);
    _ = try store.assert(E1, A_AGE, vi(30), M_BOOT);
    _ = try store.retract(E1, A_AGE, vi(30), M_BOOT);

    var it = store.attributesOf(E1);
    var count: usize = 0;
    var saw_age = false;
    while (it.next()) |kv| {
        if (kv.a == A_AGE) saw_age = true;
        count += 1;
    }
    try testing.expectEqual(@as(usize, 1), count);
    try testing.expect(!saw_age);
}

// -- Scale -------------------------------------------------------------------

test "10k assertions over 1k entities" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    const N_E: u64 = 1000;
    const N_A: u64 = 10;
    var e: u64 = 0;
    while (e < N_E) : (e += 1) {
        var a: u64 = 0;
        while (a < N_A) : (a += 1) {
            _ = try store.assert(1000 + e, 5000 + a, vi(@as(i64, @intCast(e * 100 + a))), M_BOOT);
        }
    }

    // Sample lookups
    try testing.expectEqual(vi(0), store.currentValue(1000, 5000).?);
    try testing.expectEqual(vi(50_007), store.currentValue(1500, 5007).?);
    try testing.expectEqual(vi(99_909), store.currentValue(1999, 5009).?);

    // Tx counter = 10000 + 1
    try testing.expectEqual(@as(u64, 10_000), store.currentTx());
}

// -- Phase 14 --strip=history: frozen read-only store ------------------------

test "frozenInsert builds indexes; freeze rejects further mutation" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var store = fs.FactStore.init(arena.allocator());

    // Bulk-build via the frozen path (no tx churn — counter stays at 1).
    try store.frozenInsert(E1, A_NAME, vi(42), M_FOO);
    try store.frozenInsert(E2, A_AGE, vi(7), M_FOO);
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
    try testing.expectEqual(vi(7), store.currentValue(E2, A_AGE).?);
    try testing.expectEqual(@as(u64, 0), store.currentTx()); // never bumped (assert would)

    // Freeze → writes rejected.
    store.freeze();
    try testing.expectError(error.ReadOnly, store.assert(E3, A_NAME, vi(99), M_FOO));
    try testing.expectError(error.ReadOnly, store.retract(E1, A_NAME, vi(42), M_FOO));
    // Reads still work after freeze.
    try testing.expectEqual(vi(42), store.currentValue(E1, A_NAME).?);
}

// -- Phase 15.I — index epoch arena -----------------------------------------

const KeepAll = struct {
    fn keep(_: void, _: u64, _: u64, _: u64, _: ref.Ref) bool {
        return true;
    }
};

test "index epoch arena: compact frees old epoch wholesale; values survive" {
    // Store directly on the leak-checked allocator. The index lives on the
    // epoch arena; if compact fails to free the old epoch (or deinit the
    // current one), testing.allocator flags the leak. Reify maps freed below.
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    // 1000 asserts (each (e,a) rewritten once → spine garbage + history churn,
    // all accumulating in epoch 1).
    var i: u64 = 0;
    while (i < 500) : (i += 1) {
        _ = try store.assert(100 + i, 5, vi(@intCast(i)), M_BOOT);
        _ = try store.assert(100 + i, 5, vi(@intCast(i + 1000)), M_BOOT);
    }
    try testing.expectEqual(vi(1000), store.currentValue(100, 5).?);

    const old_epoch_ptr = store.index_arena.?;
    const stats = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 500), stats.kept); // current-only collapse
    // Epoch swapped (old freed; pointer value compared, not dereferenced).
    try testing.expect(store.index_arena.? != old_epoch_ptr);

    // No corruption / UAF across the swap+free.
    try testing.expectEqual(vi(1000), store.currentValue(100, 5).?);
    try testing.expectEqual(vi(1499), store.currentValue(599, 5).?);
    try testing.expect(store.currentValue(99, 5) == null);
}

test "epoch registry (R1b): pin retains old epoch across compact; unpin frees it" {
    // Leak-checked allocator: a retained epoch that is never freed (or freed
    // twice) trips the allocator. Proves the refcount gate keeps a pinned
    // epoch alive across compact and frees it exactly once on unpin.
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);

    // Pin the current (writable) epoch, then compact. The old epoch is pinned
    // → moved to retained_epochs, NOT freed.
    const gen = store.pinCurrentEpoch();
    const old_ptr = store.index_arena.?;
    _ = try store.compact(a, {}, KeepAll.keep);

    try testing.expect(store.index_arena.? != old_ptr); // swapped to fresh epoch
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);
    try testing.expectEqual(@as(u32, 1), store.retained_epochs.items[0].refcount);
    // New epoch serves current values fine.
    try testing.expectEqual(vi(10), store.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), store.currentValue(2, 5).?);

    // Unpin → refcount 0 and below the (current-only) horizon → freed + dropped.
    store.unpinEpoch(gen);
    try testing.expectEqual(@as(usize, 0), store.retained_epochs.items.len);
    try testing.expectEqual(vi(10), store.currentValue(1, 5).?); // no UAF
}

test "epoch registry (R1b): compact does not advance basis_t" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT); // rewrite (1,5) thrice → churn
    _ = try store.assert(1, 5, vi(11), M_BOOT);
    _ = try store.assert(1, 5, vi(12), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);
    const basis_before = store.currentBasisT(); // 4 commits
    _ = try store.compact(a, {}, KeepAll.keep); // keeps 2 current facts
    try testing.expectEqual(basis_before, store.currentBasisT()); // unchanged
    // next_tx legitimately reset to the per-epoch coordinate (2 < 4).
    try testing.expect(store.currentTx() < basis_before);
}

test "snapshot (R1c): frozen view is isolated from later live mutation" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    const snap = try store.snapshotNow();
    defer snap.deinit(a);

    // Mutate live AFTER the snapshot.
    _ = try store.assert(1, 5, vi(20), M_BOOT);
    _ = try store.assert(2, 5, vi(99), M_BOOT); // entity not present at snapshot

    // Snapshot still sees the captured state; live sees the new state.
    try testing.expectEqual(vi(10), snap.view.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), store.currentValue(1, 5).?);
    try testing.expect(snap.view.currentValue(2, 5) == null); // didn't exist yet
    try testing.expectEqual(vi(99), store.currentValue(2, 5).?);
    try testing.expectEqual(@as(u64, 1), snap.basis_t); // one commit at capture
}

test "snapshot (R1c): pin survives compact; release frees the epoch" {
    const a = testing.allocator; // leak-checked
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    _ = try store.assert(1, 5, vi(10), M_BOOT);
    _ = try store.assert(2, 5, vi(20), M_BOOT);
    const snap = try store.snapshotNow();

    // Live churns then compacts — the snapshot's epoch is pinned, so compact
    // retains it rather than freeing it.
    _ = try store.assert(1, 5, vi(11), M_BOOT);
    _ = try store.compact(a, {}, KeepAll.keep);
    try testing.expectEqual(@as(usize, 1), store.retained_epochs.items.len);

    // Snapshot reads remain valid across the compact (no UAF) — it reads the
    // RETAINED old epoch's nodes, not the new live epoch.
    try testing.expectEqual(vi(10), snap.view.currentValue(1, 5).?);
    try testing.expectEqual(vi(20), snap.view.currentValue(2, 5).?);
    // Live shows the post-churn current value.
    try testing.expectEqual(vi(11), store.currentValue(1, 5).?);

    // Release → epoch refcount hits 0, below horizon → freed + dropped.
    snap.deinit(a);
    try testing.expectEqual(@as(usize, 0), store.retained_epochs.items.len);
    try testing.expectEqual(vi(11), store.currentValue(1, 5).?); // live intact
}

test "snapshot (R1c): datalog query runs against the frozen view" {
    const a = testing.allocator;
    var store = fs.FactStore.init(a);
    defer store.fact_eids.deinit();
    defer store.fact_modules.deinit();
    try store.enableIndexArena();
    defer store.deinitIndexArena();

    const attr_kind: u64 = 5;
    _ = try store.assert(100, attr_kind, vi(7), M_BOOT);
    _ = try store.assert(101, attr_kind, vi(7), M_BOOT);
    const snap = try store.snapshotNow();
    defer snap.deinit(a);

    // After the snapshot, a third entity gets the same value on the LIVE store.
    _ = try store.assert(102, attr_kind, vi(7), M_BOOT);

    // entitiesMatching against the view sees exactly the 2 captured entities;
    // the live store sees 3. (Iterator-level datalog primitive over the view.)
    var snap_count: usize = 0;
    var sit = snap.view.entitiesMatching(attr_kind, vi(7));
    while (sit.next()) |_| snap_count += 1;
    try testing.expectEqual(@as(usize, 2), snap_count);

    var live_count: usize = 0;
    var lit = store.entitiesMatching(attr_kind, vi(7));
    while (lit.next()) |_| live_count += 1;
    try testing.expectEqual(@as(usize, 3), live_count);
}

test "index epoch arena: retain_history gate skips the free (no swap)" {
    // Backing arena reclaims the intentionally-retained epoch at teardown.
    var backing = std.heap.ArenaAllocator.init(testing.allocator);
    defer backing.deinit();
    const a = backing.allocator();
    var store = fs.FactStore.init(a);
    try store.enableIndexArena();
    store.retain_history = true;

    _ = try store.assert(1, 2, vi(3), M_BOOT);
    const epoch = store.index_arena.?;
    _ = try store.compact(a, {}, KeepAll.keep);

    try testing.expect(store.index_arena.? == epoch); // NOT swapped, old kept
    try testing.expectEqual(vi(3), store.currentValue(1, 2).?);
}
