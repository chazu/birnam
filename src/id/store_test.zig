//! Proof for rung 2a.1: the relational model models itself.
//!
//! Boots libid, brings up the store module ON it (through the id API), and shows
//! the **raw-append → seed → flip fixpoint**: a post-flip `assert` consults the
//! store's OWN seeded schema (`currentValue(a, :cardinality)`), and that consult
//! terminates on the seeded knot `[cardinality cardinality one]`. The store is
//! exercised through real libid dispatch (`api.send`), proving it is a genuine
//! libid object — not a Zig struct with a veneer.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const store = @import("store.zig");
const errmod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

/// The RAW read (absence = Zig null): these tests probe store INTERNALS —
/// retraction leaves no value, a raw overwrite retracts the mirror fact — where
/// the absent/bound-nil distinction is the point. The birnam-facing
/// `currentValue:attr:` surface normalises absence to nil (locked below).
fn cv(api: *idapi.IdApi, e: Oop, a: Oop) Oop {
    return api.send(store.store_instance, api.intern("rawCurrentValue:attr:"), &[_]Oop{ e, a }, 2);
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
    const api = floor.api();
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
    const api = floor.api();
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
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ score, store.s_history, store.s_true }, 3);
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
    const api = floor.api();
    store.bringUp(api);

    const door = api.intern("door");
    const openp = api.intern("open");
    const yes = api.intern("yes");
    const no = api.intern("no");

    // Mint a cell handle. The stateful protocol lives on the CELL, not on `door`.
    const cell = api.send(store.store_instance, store.s_cellFor, &[_]Oop{ door, openp }, 2);

    // Initially nil — the SINGLETON, not raw absence: `[cell value]` is a
    // birnam-facing surface, and a raw null receiver panics the VM.
    try std.testing.expect(api.send(cell, store.s_value, &[_]Oop{}, 0) == api.nilObj);

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
    try std.testing.expect(api.send(api.vtableOf(door), api.intern("lookup"), &.{store.s_value}, 1) == null);
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

var mop_api: ?*idapi.IdApi = null;
var mop_vt: Oop = null;
var mop_sel: Oop = null;
var mop_lookup: Oop = null;
var mop_expected_before_dict: Oop = null;
var mop_expected_before_mirror: Oop = null;
var mop_expected_after: Oop = null;
var mop_authorized = true;
var mop_authorizer_calls: usize = 0;
var mop_authorizer_saw_expected = false;
var mop_observer_calls: usize = 0;
var mop_observer_saw_expected = false;

fn lookupMethod(api: *idapi.IdApi, vt: Oop, sel: Oop) Oop {
    return api.send(vt, mop_lookup, &.{sel}, 1);
}

fn authorizeMopMutation() callconv(.c) Oop {
    const api = mop_api.?;
    mop_authorizer_calls += 1;
    mop_authorizer_saw_expected = lookupMethod(api, mop_vt, mop_sel) == mop_expected_before_dict and
        cv(api, mop_vt, mop_sel) == mop_expected_before_mirror;
    return if (mop_authorized) api.trueObj else api.falseObj;
}

fn observeMopMutation(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const api = mop_api.?;
    mop_observer_calls += 1;
    mop_observer_saw_expected = args[0] == mop_sel and args[1] == mop_vt and
        lookupMethod(api, mop_vt, mop_sel) == mop_expected_after and
        cv(api, mop_vt, mop_sel) == mop_expected_after;
    return self;
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
    const api = floor.api();
    store.bringUp(api);

    const s2 = api.send(store.store_instance, store.s_new, &[_]Oop{}, 0);
    try std.testing.expect(s2 != store.store_instance);

    // s2 is seeded: the cardinality knot resolves through it too.
    try std.testing.expect(api.send(s2, store.s_currentValue, &[_]Oop{ store.s_cardinality, store.s_cardinality }, 2) == store.s_one);

    // Independent: asserting into s2 does not touch the well-known store.
    const e = api.intern("new_e");
    const a = api.intern("new_a");
    const v = api.intern("new_v");
    _ = api.send(s2, store.s_assert, &[_]Oop{ e, a, v }, 3);
    try std.testing.expect(api.send(s2, store.s_currentValue, &[_]Oop{ e, a }, 2) == v);
    try std.testing.expect(cv(api, e, a) == null); // well-known store unaffected
}

test "rung 2 polish: retraction — :one removal, :many member removal, history timeline" {
    const api = floor.api();
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

test "store eq: Float values match by VALUE — set semantics + retract by a fresh box" {
    const api = floor.api();
    store.bringUp(api);
    const e = api.intern("f_e");

    // :many — a fresh equal-valued Float box is the SAME member (no growth).
    const many = api.intern("f_many");
    cardMany(api, many);
    assert3(api, e, many, api.mkFloat(1.5));
    assert3(api, e, many, api.mkFloat(1.5));
    try std.testing.expectEqual(@as(usize, 1), count(api, e, many));
    assert3(api, e, many, api.mkFloat(2.5)); // an UNEQUAL float does grow the set
    try std.testing.expectEqual(@as(usize, 2), count(api, e, many));
    // Retract by a fresh equal-valued box finds the stored fact.
    retract3(api, e, many, api.mkFloat(1.5));
    try std.testing.expectEqual(@as(usize, 1), count(api, e, many));

    // :one + history — re-asserting an equal-valued box is a no-op (no growth).
    const one = api.intern("f_one");
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ one, store.s_history, store.s_true }, 3);
    assert3(api, e, one, api.mkFloat(3.25));
    assert3(api, e, one, api.mkFloat(3.25));
    try std.testing.expectEqual(@as(usize, 1), histLen(api, e, one));
}

test "rung 2 polish: query surface — enumerate an entity's attrs / a type's selectors" {
    const api = floor.api();
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
    const m: Oop = @ptrCast(@constCast(&answer42));
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ api.intern("q_m1"), vt, m }, 3);
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ api.intern("q_m2"), vt, m }, 3);
    try std.testing.expectEqual(@as(usize, 2), attrCount(api, vt));

    // Retracting an attribute drops it from the enumeration.
    retract3(api, e, a, api.intern("q_va"));
    try std.testing.expectEqual(@as(usize, 1), attrCount(api, e));
}

test "rung 2b.1: tracked dispatch mirror — one write, two representations; escape hatch" {
    const api = floor.api();
    store.bringUp(api);

    const widget_vt = api.proto(api.object_vt);
    const s_answer = api.intern("answer");
    const m: Oop = @ptrCast(@constCast(&answer42));

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
    const api = floor.api();
    store.bringUp(api);

    const gadget_vt = api.proto(api.object_vt);
    const s_go = api.intern("go");
    const m1: Oop = @ptrCast(@constCast(&answer42));
    const m2: Oop = @ptrCast(@constCast(&answer7));

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

test "Store exact removal restores inheritance and preserves method history" {
    const api = floor.api();
    store.bringUp(api);
    errmod.bringUp(api);

    const parent_vt = api.proto(api.object_vt);
    const parent = api.alloc(parent_vt, 0);
    api.setOwner(parent_vt, parent);
    const child_vt = api.proto(parent_vt);
    const child = api.alloc(child_vt, 0);
    api.setOwner(child_vt, child);
    const sel = api.intern("trackedRemovalAnswer");
    const m42: Oop = @ptrCast(@constCast(&answer42));
    const m7: Oop = @ptrCast(@constCast(&answer7));
    assert3(api, sel, store.s_history, store.s_true);

    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ sel, parent_vt, m42 }, 3);
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ sel, child_vt, m7 }, 3);
    try std.testing.expectEqual(@as(usize, 7), @intFromPtr(api.send(child, sel, &.{}, 0)));

    const r = api.send(store.store_instance, store.s_removeMethodOn, &[_]Oop{ sel, child_vt }, 2);
    try std.testing.expect(r == store.store_instance);
    try std.testing.expect(cv(api, child_vt, sel) == null);
    try std.testing.expectEqual(@as(usize, 42), @intFromPtr(api.send(child, sel, &.{}, 0)));
    try std.testing.expectEqual(@as(usize, 2), histLen(api, child_vt, sel));

    // A second exact removal is a visible kinded error, not inherited removal.
    const missing = api.send(store.store_instance, store.s_removeMethodOn, &[_]Oop{ sel, child_vt }, 2);
    try std.testing.expect(api.send(missing, api.intern("isError"), &.{}, 0) == api.trueObj);
    try std.testing.expect(api.send(missing, api.intern("kind"), &.{}, 0) == api.intern("missingDirectMethod"));

    // No tombstone/copy: changing the parent remains visible through the child.
    _ = api.send(store.store_instance, store.s_addMethodOn, &[_]Oop{ sel, parent_vt, m7 }, 3);
    try std.testing.expectEqual(@as(usize, 7), @intFromPtr(api.send(child, sel, &.{}, 0)));
}

test "raw method mutation retracts a primitive mirror marker" {
    const api = floor.api();
    store.bringUp(api);

    const vt = api.proto(api.object_vt);
    const owner = api.alloc(vt, 0);
    api.setOwner(vt, owner);
    const sel = api.intern("nativeMarkerRemoval");
    api.method(vt, sel, &answer42);
    _ = api.send(store.store_instance, api.intern("backfillMirror"), &.{}, 0);
    try std.testing.expect(cv(api, vt, sel) == api.intern("primitive"));

    api.method(vt, sel, &answer7);
    try std.testing.expect(cv(api, vt, sel) == null);
}

test "rung 2d: history-off asOf is HONEST, not silently nil (docs/42 §D3)" {
    const api = floor.api();
    store.bringUp(api);
    errmod.bringUp(api); // so `valueOf:attr:asOf:` can yield a kinded error value

    const db = store.store_instance;
    const e = api.intern("widget");
    const x = api.intern("colour"); // history-off :one by default
    const valueOf = api.intern("valueOf:attr:asOf:");
    const basisSel = api.intern("basis");

    _ = api.send(db, store.s_assert, &[_]Oop{ e, x, api.intern("red") }, 3);
    const b1 = api.send(db, basisSel, &[_]Oop{}, 0); // snapshot: colour == red

    // 1. Reliable: the deciding fact predates the basis → the real value, red.
    try std.testing.expect(api.send(db, valueOf, &[_]Oop{ e, x, b1 }, 3) == api.intern("red"));

    // 2. Overwrite (history-off → the old value is discarded, the fact's tx bumps).
    _ = api.send(db, store.s_assert, &[_]Oop{ e, x, api.intern("blue") }, 3);

    // 3. asOf b1 can no longer be answered — but rather than a silently-WRONG nil
    //    (implying colour was absent at b1, when it was red), it yields an honest
    //    #historyNotRetained error value.
    const r = api.send(db, valueOf, &[_]Oop{ e, x, b1 }, 3);
    try std.testing.expect(r != api.nilObj);
    try std.testing.expect(api.send(r, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r, api.intern("kind"), &[_]Oop{}, 0) == api.intern("historyNotRetained"));

    // 4. A genuinely absent (e,a) — no fact at all — still reads nil (honest absence).
    const b2 = api.send(db, basisSel, &[_]Oop{}, 0);
    try std.testing.expect(api.send(db, valueOf, &[_]Oop{ e, api.intern("ghost"), b2 }, 3) == api.nilObj);
}

// A 0-arg stand-in block: a plain object whose `value` returns a fixed sentinel,
// so `at:ifAbsent:` has something to evaluate on the absent path.
fn ifAbsent99(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gG6api.mkSmallInt(99);
}
var gG6api: *idapi.IdApi = undefined;

test "§G6: Globals tells 'absent' from 'bound to nil' — includes: honest, at:ifAbsent: distinguishes" {
    const api = floor.api();
    store.bringUp(api);
    gG6api = api;

    const g = store.globals_obj;
    const present = api.intern("g6_present");
    const nilbound = api.intern("g6_nilbound");
    const absent = api.intern("g6_absent"); // never bound

    _ = api.send(g, api.intern("at:put:"), &[_]Oop{ present, api.mkSmallInt(7) }, 2);
    _ = api.send(g, api.intern("at:put:"), &[_]Oop{ nilbound, api.nilObj }, 2); // an EXPLICIT nil binding

    // includes: reads presence honestly — a global set to nil IS present; only a
    // never-bound name is absent. (The store keeps internal absence distinct from
    // the nil value, so this needs no Binding object.)
    try std.testing.expect(api.send(g, api.intern("includes:"), &[_]Oop{present}, 1) == api.trueObj);
    try std.testing.expect(api.send(g, api.intern("includes:"), &[_]Oop{nilbound}, 1) == api.trueObj);
    try std.testing.expect(api.send(g, api.intern("includes:"), &[_]Oop{absent}, 1) == api.falseObj);

    // at:ifAbsent: — the block runs ONLY on genuine absence; a bound nil returns nil.
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern("value"), &ifAbsent99);
    const blk = api.alloc(vt, 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(g, api.intern("at:ifAbsent:"), &[_]Oop{ present, blk }, 2)));
    try std.testing.expect(api.send(g, api.intern("at:ifAbsent:"), &[_]Oop{ nilbound, blk }, 2) == api.nilObj); // bound nil: block NOT run
    try std.testing.expectEqual(@as(i64, 99), api.smallIntValue(api.send(g, api.intern("at:ifAbsent:"), &[_]Oop{ absent, blk }, 2))); // absent: block runs

    // at: alone CAN'T distinguish (both answer nil) — the reason at:ifAbsent: exists.
    try std.testing.expect(api.send(g, api.intern("at:"), &[_]Oop{nilbound}, 1) == api.nilObj);
    try std.testing.expect(api.send(g, api.intern("at:"), &[_]Oop{absent}, 1) == api.nilObj);
}

test "rung 2a.1: currentValue of an absent (e,a) is nil" {
    // Tests share one process: id_init + bringUp are both idempotent (bringUp
    // takes its reload path and re-imports the live store), so this just rides
    // the already-seeded store.
    const api = floor.api();
    store.bringUp(api);

    const ghost = api.intern("ghost");
    const slot = api.intern("slot");
    // The birnam-facing surface answers the nil SINGLETON (a raw null receiver
    // panics the VM — `[db currentValue: #absent attr: #x]` was a crash); the raw
    // tier keeps the internal absence for Zig callers; `hasValue:attr:` is the
    // block-free presence read (§G6).
    try std.testing.expect(api.send(store.store_instance, store.s_currentValue, &[_]Oop{ ghost, slot }, 2) == api.nilObj);
    try std.testing.expect(cv(api, ghost, slot) == null);
    try std.testing.expect(api.send(store.store_instance, api.intern("hasValue:attr:"), &[_]Oop{ ghost, slot }, 2) == api.falseObj);
    // …and a fact BOUND to nil reads as present: nil is a value (the no-NULL lock).
    _ = api.send(store.store_instance, store.s_assert, &[_]Oop{ ghost, slot, api.nilObj }, 3);
    try std.testing.expect(api.send(store.store_instance, store.s_currentValue, &[_]Oop{ ghost, slot }, 2) == api.nilObj);
    try std.testing.expect(cv(api, ghost, slot) == api.nilObj);
    try std.testing.expect(api.send(store.store_instance, api.intern("hasValue:attr:"), &[_]Oop{ ghost, slot }, 2) == api.trueObj);
}

test "MOP authorization and observation bracket completed consistent mutations" {
    const api = floor.api();
    store.bringUp(api);

    mop_api = api;
    mop_lookup = api.intern("lookup");
    mop_vt = api.proto(api.object_vt);
    const owner = api.alloc(mop_vt, 0);
    api.setOwner(mop_vt, owner);
    mop_sel = api.intern("mopOrderingAnswer");
    const m42: Oop = @ptrCast(@constCast(&answer42));
    const m7: Oop = @ptrCast(@constCast(&answer7));

    const observer_vt = api.proto(api.object_vt);
    api.method(observer_vt, api.intern("managedMethodChanged:on:"), &observeMopMutation);
    const observer = api.alloc(observer_vt, 0);
    try std.testing.expect(api.send(store.store_instance, api.intern("setMethodMutationObserver:"), &.{observer}, 1) == store.store_instance);
    const authorizer_address: i64 = @intCast(@intFromPtr(&authorizeMopMutation));
    try std.testing.expect(api.send(store.store_instance, api.intern("setMethodMutationAuthorizer:"), &.{api.mkSmallInt(authorizer_address)}, 1) == store.store_instance);

    // Rejection happens before either representation changes and never notifies.
    mop_expected_before_dict = null;
    mop_expected_before_mirror = null;
    mop_authorized = false;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    const rejected_add = api.send(store.store_instance, store.s_addMethodOn, &.{ mop_sel, mop_vt, m42 }, 3);
    try std.testing.expect(rejected_add == api.falseObj);
    try std.testing.expectEqual(@as(usize, 1), mop_authorizer_calls);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
    try std.testing.expect(lookupMethod(api, mop_vt, mop_sel) == null);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == null);

    // Addition and replacement notify once, only after dictionary and mirror
    // agree on the completed value.
    mop_authorized = true;
    mop_expected_before_dict = null;
    mop_expected_before_mirror = null;
    mop_expected_after = m42;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    _ = api.send(store.store_instance, store.s_addMethodOn, &.{ mop_sel, mop_vt, m42 }, 3);
    try std.testing.expectEqual(@as(usize, 1), mop_authorizer_calls);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expectEqual(@as(usize, 1), mop_observer_calls);
    try std.testing.expect(mop_observer_saw_expected);

    mop_expected_before_dict = m42;
    mop_expected_before_mirror = m42;
    mop_expected_after = m7;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    _ = api.send(store.store_instance, store.s_addMethodOn, &.{ mop_sel, mop_vt, m7 }, 3);
    try std.testing.expectEqual(@as(usize, 1), mop_authorizer_calls);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expectEqual(@as(usize, 1), mop_observer_calls);
    try std.testing.expect(mop_observer_saw_expected);

    // Rejected removal preserves both; accepted removal completes both before
    // the one observer callback.
    mop_authorized = false;
    mop_expected_before_dict = m7;
    mop_expected_before_mirror = m7;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    try std.testing.expect(api.send(store.store_instance, store.s_removeMethodOn, &.{ mop_sel, mop_vt }, 2) == api.falseObj);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
    try std.testing.expect(lookupMethod(api, mop_vt, mop_sel) == m7);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == m7);

    mop_authorized = true;
    mop_expected_after = null;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    _ = api.send(store.store_instance, store.s_removeMethodOn, &.{ mop_sel, mop_vt }, 2);
    try std.testing.expectEqual(@as(usize, 1), mop_authorizer_calls);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expectEqual(@as(usize, 1), mop_observer_calls);
    try std.testing.expect(mop_observer_saw_expected);
}

test "raw MOP mutation stays unobserved and primitive markers never go stale" {
    const api = floor.api();
    store.bringUp(api);
    mop_api = api;
    mop_lookup = api.intern("lookup");
    mop_vt = api.proto(api.object_vt);
    const owner = api.alloc(mop_vt, 0);
    api.setOwner(mop_vt, owner);
    mop_sel = api.intern("mopPrimitiveLifecycle");
    const m42: Oop = @ptrCast(@constCast(&answer42));
    const primitive = api.intern("primitive");

    mop_observer_calls = 0;
    api.method(mop_vt, mop_sel, &answer7);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == null);

    _ = api.send(store.store_instance, api.intern("backfillMirror"), &.{}, 0);
    _ = api.send(store.store_instance, api.intern("backfillMirror"), &.{}, 0);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == primitive);
    try std.testing.expectEqual(@as(usize, 1), count(api, mop_vt, mop_sel));
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);

    // Tracked replacement retracts the presence marker and installs the exact
    // method before notifying.
    mop_authorized = true;
    mop_expected_before_dict = @ptrCast(@constCast(&answer7));
    mop_expected_before_mirror = primitive;
    mop_expected_after = m42;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    _ = api.send(store.store_instance, store.s_addMethodOn, &.{ mop_sel, mop_vt, m42 }, 3);
    try std.testing.expect(mop_authorizer_saw_expected);
    try std.testing.expect(mop_observer_saw_expected);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == m42);

    // Raw replacement and removal retract any exact/primitive mirror, never
    // synthesize a new one, and never invoke the supported-mutation observer.
    mop_observer_calls = 0;
    api.method(mop_vt, mop_sel, &answer7);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == null);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
    _ = api.send(store.store_instance, api.intern("backfillMirror"), &.{}, 0);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == primitive);
    _ = api.send(mop_vt, api.intern("removeMethod:"), &.{mop_sel}, 1);
    try std.testing.expect(cv(api, mop_vt, mop_sel) == null);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
}

test "MOP backfill marks real types and excludes internal dictionaries" {
    const api = floor.api();
    store.bringUp(api);
    _ = api.send(store.store_instance, api.intern("backfillMirror"), &.{}, 0);

    const clone = api.intern("clone");
    try std.testing.expect(cv(api, api.object_vt, clone) == api.intern("primitive"));

    var internal_vtables: usize = 0;
    var i: usize = 0;
    while (i < api.vtableCount()) : (i += 1) {
        const vt = api.vtableAt(i);
        if (vt == api.object_vt or api.vtableParent(vt) != null) continue;
        if (api.vtableSelectorAt(vt, 0) == null) continue;
        internal_vtables += 1;
        var j: usize = 0;
        while (api.vtableSelectorAt(vt, j)) |selector| : (j += 1)
            try std.testing.expect(cv(api, vt, selector) == null);
    }
    try std.testing.expect(internal_vtables >= 2); // symbol interner + export environment
}
