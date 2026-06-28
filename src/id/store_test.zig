//! Proof for rung 2a.1: the relational model models itself.
//!
//! Boots libid, brings up the store module ON it (through the id API), and shows
//! the **raw-append → seed → flip fixpoint**: a post-flip `assert` consults the
//! store's OWN seeded schema (`currentValue(a, :cardinality)`), and that consult
//! terminates on the seeded knot `[cardinality cardinality one]`. The store is
//! exercised through real libid dispatch (`api.send`), proving it is a genuine
//! libid object — not a Zig struct with a veneer.

const std = @import("std");
const id = @import("id.zig");
const store = @import("store.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn cv(api: *idapi.IdApi, e: Oop, a: Oop) Oop {
    return api.send(store.store_instance, store.s_currentValue, &[_]Oop{ e, a }, 2);
}
fn assert3(api: *idapi.IdApi, e: Oop, a: Oop, v: Oop) void {
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ e, a, v }, 3);
}
fn count(api: *idapi.IdApi, e: Oop, a: Oop) usize {
    return @intFromPtr(api.send(store.store_instance, store.s_count, &[_]Oop{ e, a }, 2));
}
fn histLen(api: *idapi.IdApi, e: Oop, a: Oop) usize {
    return @intFromPtr(api.send(store.store_instance, store.s_history_read, &[_]Oop{ e, a }, 2));
}

test "rung 2a.1: raw-append → seed → flip; the relational model models itself" {
    const api = id.id_init();
    store.bringUp(api);

    // The knot is present and resolves THROUGH the store: cardinality of
    // cardinality is `one` — the fact that unties the regress.
    try std.testing.expect(cv(api, store.s_cardinality, store.s_cardinality) == store.s_one);

    // Post-flip, register `tags` as :many by asserting [tags cardinality many].
    // Placing this fact consults cardinalityOf(:cardinality) == one (the knot).
    const tags = api.intern("tags");
    assert3(api, tags, store.s_cardinality, store.s_many);
    try std.testing.expect(cv(api, tags, store.s_cardinality) == store.s_many);

    // A :many attribute ACCUMULATES — observable proof the consult resolved
    // through the seeded schema (not a hardcoded default).
    const e = api.intern("e1");
    const alpha = api.intern("alpha");
    const beta = api.intern("beta");
    assert3(api, e, tags, alpha);
    assert3(api, e, tags, beta);
    try std.testing.expectEqual(@as(usize, 2), count(api, e, tags));
    // set semantics: re-asserting an existing member does not grow the set.
    assert3(api, e, tags, alpha);
    try std.testing.expectEqual(@as(usize, 2), count(api, e, tags));

    // A default (unseeded) attribute is :one — supersedes; count stays 1.
    const color = api.intern("color");
    const red = api.intern("red");
    const blue = api.intern("blue");
    assert3(api, e, color, red);
    assert3(api, e, color, blue);
    try std.testing.expect(cv(api, e, color) == blue);
    try std.testing.expectEqual(@as(usize, 1), count(api, e, color));
}

test "rung 2a.2: history is opt-in per attribute, default OFF (the 4 GB lesson)" {
    const api = id.id_init();
    store.bringUp(api);

    // A default `:one` attribute keeps ONE slot however many times it changes —
    // the log does not grow with updates.
    const player = api.intern("player");
    const hp = api.intern("hp");
    const v10 = api.intern("v10");
    const v20 = api.intern("v20");
    const v30 = api.intern("v30");
    assert3(api, player, hp, v10);
    assert3(api, player, hp, v20);
    assert3(api, player, hp, v30);
    try std.testing.expect(cv(api, player, hp) == v30); // latest wins
    try std.testing.expectEqual(@as(usize, 1), histLen(api, player, hp)); // no growth

    // Opt `score` into history (a schema cell), then the timeline IS retained.
    const score = api.intern("score");
    _ = api.send(store.store_instance, store.s_assert,
        &[_]Oop{ score, store.s_history, store.s_true }, 3);
    assert3(api, player, score, v10);
    assert3(api, player, score, v20);
    assert3(api, player, score, v30);
    try std.testing.expect(cv(api, player, score) == v30); // still latest-wins
    try std.testing.expectEqual(@as(usize, 3), histLen(api, player, score)); // retained

    // Re-asserting the unchanged current value is a no-op (history doesn't grow).
    assert3(api, player, score, v30);
    try std.testing.expectEqual(@as(usize, 3), histLen(api, player, score));
}

test "rung 2a.3: a Cell is a handle onto (e,a) — value/set:/history, off base objects" {
    const api = id.id_init();
    store.bringUp(api);

    const door = api.intern("door");
    const openp = api.intern("open");
    const yes = api.intern("yes");
    const no = api.intern("no");

    // Mint a cell handle. The stateful protocol lives on the CELL, not on `door`.
    const cell = api.send(store.store_instance, store.s_cellFor, &[_]Oop{ door, openp }, 2);

    // Initially nil.
    try std.testing.expect(api.send(cell, store.s_value, &[_]Oop{}, 0) == null);

    // [cell set: yes] then [cell value] == yes.
    _ = api.send(cell, store.s_set, &[_]Oop{yes}, 1);
    try std.testing.expect(api.send(cell, store.s_value, &[_]Oop{}, 0) == yes);

    // Set goes through the schema-aware path: :one default supersedes.
    _ = api.send(cell, store.s_set, &[_]Oop{no}, 1);
    try std.testing.expect(api.send(cell, store.s_value, &[_]Oop{}, 0) == no);

    // A second cell onto the SAME (e,a) sees the same value — cells are views.
    const cell2 = api.send(store.store_instance, store.s_cellFor, &[_]Oop{ door, openp }, 2);
    try std.testing.expect(api.send(cell2, store.s_value, &[_]Oop{}, 0) == no);

    // History off by default → timeline length 1 despite two sets.
    try std.testing.expectEqual(@as(usize, 1), @intFromPtr(api.send(cell, store.s_history, &[_]Oop{}, 0)));

    // The stratification guarantee: `door` itself does not understand `value`
    // (the reflective/stateful protocol is on the cell mirror, not the entity).
    try std.testing.expect(id.bind(door, @ptrCast(@alignCast(store.s_value.?))) == null);
}

test "rung 2a.1: currentValue of an absent (e,a) is nil" {
    // Tests share one process: id_init + bringUp are both idempotent (bringUp
    // takes its reload path and re-imports the live store), so this just rides
    // the already-seeded store.
    const api = id.id_init();
    store.bringUp(api);

    const ghost = api.intern("ghost");
    const slot = api.intern("slot");
    try std.testing.expect(cv(api, ghost, slot) == null);
}
