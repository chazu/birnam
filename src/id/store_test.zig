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
    return @intCast(api.smallIntValue(api.send(store.store_instance, store.s_count, &[_]Oop{ e, a }, 2)));
}
fn histLen(api: *idapi.IdApi, e: Oop, a: Oop) usize {
    return @intCast(api.smallIntValue(api.send(store.store_instance, store.s_history_read, &[_]Oop{ e, a }, 2)));
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
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(cell, store.s_history, &[_]Oop{}, 0)));

    // The stratification guarantee: `door` itself does not understand `value`
    // (the reflective/stateful protocol is on the cell mirror, not the entity).
    try std.testing.expect(id.bind(door, @ptrCast(@alignCast(store.s_value.?))) == null);
}

fn answer42(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(42);
}

fn answer7(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(7);
}

fn retract3(api: *idapi.IdApi, e: Oop, a: Oop, v: Oop) void {
    _ = api.send(store.store_instance, store.s_retract, &[_]Oop{ e, a, v }, 3);
}
fn cardMany(api: *idapi.IdApi, a: Oop) void {
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ a, store.s_cardinality, store.s_many }, 3);
}
fn attrCount(api: *idapi.IdApi, e: Oop) usize {
    return @intCast(api.smallIntValue(api.send(store.store_instance, store.s_attrCount, &[_]Oop{e}, 1)));
}

test "rung 2 polish: Store new mints independent, seeded stores" {
    const api = id.id_init();
    store.bringUp(api);

    const s2 = api.send(store.store_instance, store.s_new, &[_]Oop{}, 0);
    try std.testing.expect(s2 != store.store_instance);

    // s2 is seeded: the cardinality knot resolves through it too.
    try std.testing.expect(api.send(s2, store.s_currentValue,
        &[_]Oop{ store.s_cardinality, store.s_cardinality }, 2) == store.s_one);

    // Independent: asserting into s2 does not touch the well-known store.
    const e = api.intern("new_e");
    const a = api.intern("new_a");
    const v = api.intern("new_v");
    _ = api.send(s2, store.s_assert, &[_]Oop{ e, a, v }, 3);
    try std.testing.expect(api.send(s2, store.s_currentValue, &[_]Oop{ e, a }, 2) == v);
    try std.testing.expect(cv(api, e, a) == null); // well-known store unaffected
}

test "rung 2 polish: retraction — :one removal, :many member removal, history timeline" {
    const api = id.id_init();
    store.bringUp(api);
    const e = api.intern("rt_e");

    // :one (default, history off) — retract removes the value entirely.
    const one = api.intern("rt_one");
    const x = api.intern("rt_x");
    assert3(api, e, one, x);
    try std.testing.expect(cv(api, e, one) == x);
    retract3(api, e, one, x);
    try std.testing.expect(cv(api, e, one) == null);
    try std.testing.expectEqual(@as(usize, 0), count(api, e, one));

    // :many (history off) — retract drops one member; re-adding works (op-aware).
    const many = api.intern("rt_many");
    cardMany(api, many);
    const m1 = api.intern("rt_m1");
    const m2 = api.intern("rt_m2");
    assert3(api, e, many, m1);
    assert3(api, e, many, m2);
    try std.testing.expectEqual(@as(usize, 2), count(api, e, many));
    retract3(api, e, many, m1);
    try std.testing.expectEqual(@as(usize, 1), count(api, e, many));
    assert3(api, e, many, m1); // re-add a retracted member
    try std.testing.expectEqual(@as(usize, 2), count(api, e, many));

    // :many + history on — the add/remove timeline is retained (unblocked by retract).
    const hm = api.intern("rt_hist");
    cardMany(api, hm);
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ hm, store.s_history, store.s_true }, 3);
    const h1 = api.intern("rt_h1");
    assert3(api, e, hm, h1); // assert event
    retract3(api, e, hm, h1); // retract event
    try std.testing.expectEqual(@as(usize, 0), count(api, e, hm)); // not present
    try std.testing.expectEqual(@as(usize, 2), histLen(api, e, hm)); // both events kept
}

test "rung 2 polish: query surface — enumerate an entity's attrs / a type's selectors" {
    const api = id.id_init();
    store.bringUp(api);

    const e = api.intern("q_e");
    const a = api.intern("q_a");
    const b = api.intern("q_b");
    assert3(api, e, a, api.intern("q_va"));
    assert3(api, e, b, api.intern("q_vb"));
    try std.testing.expectEqual(@as(usize, 2), attrCount(api, e));

    // Indexed access, first-appearance order; out-of-range is nil.
    try std.testing.expect(api.send(store.store_instance, store.s_attrAt, &[_]Oop{ e, api.mkSmallInt(0) }, 2) == a);
    try std.testing.expect(api.send(store.store_instance, store.s_attrAt, &[_]Oop{ e, api.mkSmallInt(1) }, 2) == b);
    try std.testing.expect(api.send(store.store_instance, store.s_attrAt, &[_]Oop{ e, api.mkSmallInt(2) }, 2) == null);

    // Same mechanism over a tracked vtable: attrCount == # mirrored selectors.
    const vt = api.proto(api.object_vt);
    const m: Oop = @constCast(@ptrCast(&answer42));
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ api.intern("q_m1"), vt, m }, 3);
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ api.intern("q_m2"), vt, m }, 3);
    try std.testing.expectEqual(@as(usize, 2), attrCount(api, vt));

    // Retracting an attribute drops it from the enumeration.
    retract3(api, e, a, api.intern("q_va"));
    try std.testing.expectEqual(@as(usize, 1), attrCount(api, e));
}

test "rung 2b.1: tracked dispatch mirror — one write, two representations; escape hatch" {
    const api = id.id_init();
    store.bringUp(api);

    const widget_vt = api.proto(api.object_vt);
    const s_answer = api.intern("answer");
    const m: Oop = @constCast(@ptrCast(&answer42));

    // Tracked install THROUGH the mirror.
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ s_answer, widget_vt, m }, 3);

    // 1. Authoritative fast path works: an instance dispatches the method.
    const w = api.alloc(widget_vt, 0);
    try std.testing.expectEqual(@as(usize, 42), @intFromPtr(api.send(w, s_answer, &[_]Oop{}, 0)));

    // 2. The mirror recorded it as a fact: currentValue(widget_vt, answer) == m.
    try std.testing.expect(cv(api, widget_vt, s_answer) == m);
    try std.testing.expectEqual(@as(usize, 1), count(api, widget_vt, s_answer));

    // 3. Escape hatch: a DIRECT (untracked) install dispatches but is invisible
    //    to the store — the root metavtable is never patched.
    const s_other = api.intern("other");
    api.method(widget_vt, s_other, &answer42);
    try std.testing.expect(api.send(w, s_other, &[_]Oop{}, 0) != null); // dispatch works
    try std.testing.expect(cv(api, widget_vt, s_other) == null); // not mirrored
}

test "rung 2b.1: the mirror can't desync — a raw overwrite retracts the stale fact (docs/42 §D1)" {
    const api = id.id_init();
    store.bringUp(api);

    const gadget_vt = api.proto(api.object_vt);
    const s_go = api.intern("go");
    const m1: Oop = @constCast(@ptrCast(&answer42));
    const m2: Oop = @constCast(@ptrCast(&answer7));

    // 1. Tracked install through the mirror: fact records m1.
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ s_go, gadget_vt, m1 }, 3);
    try std.testing.expect(cv(api, gadget_vt, s_go) == m1);

    // 2. A raw (escape-hatch) install REBINDS (gadget_vt, go) in the dict to m2.
    //    Before the fix the mirror kept pointing at the dead m1 (the desync bug).
    api.method(gadget_vt, s_go, &answer7);

    // 3. The mirror is now ABSENT (honest), never the stale m1 (wrong). The dict is
    //    authoritative — dispatch answers m2's 7.
    try std.testing.expect(cv(api, gadget_vt, s_go) == null);
    const g = api.alloc(gadget_vt, 0);
    try std.testing.expectEqual(@as(usize, 7), @intFromPtr(api.send(g, s_go, &[_]Oop{}, 0)));

    // 4. Re-tracking through the mirror restores a CURRENT fact (m2), not a stale one.
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ s_go, gadget_vt, m2 }, 3);
    try std.testing.expect(cv(api, gadget_vt, s_go) == m2);
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
