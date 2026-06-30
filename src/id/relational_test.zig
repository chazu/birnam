//! Proof for the relational reading of the store: the datalog join + pull, driven
//! through the object model. Brings up store + collection + relational, mints
//! entities and logic variables by message, and runs queries.

const std = @import("std");
const id = @import("id.zig");
const prelude = @import("prelude.zig");
const store = @import("store.zig");
const collection = @import("collection.zig");
const relational = @import("relational.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api);
    store.bringUp(api);
    collection.bringUp(api);
    relational.bringUp(api);
    return api;
}

fn s0(rcv: Oop, sel: [*:0]const u8) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{}, 0);
}
fn s1(rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{a}, 1);
}
fn s2(rcv: Oop, sel: [*:0]const u8, a: Oop, b: Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{ a, b }, 2);
}
fn s3(rcv: Oop, sel: [*:0]const u8, a: Oop, b: Oop, c: Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{ a, b, c }, 3);
}
fn ival(o: Oop) i64 {
    return gapi.smallIntValue(o);
}
fn strEq(o: Oop, want: []const u8) bool {
    var len: usize = 0;
    const p = gapi.stringBytes(o, &len);
    return std.mem.eql(u8, p[0..len], want);
}

// Build a Vector from elements via the factory + add: (mirrors `#[…]` codegen).
fn vec(api: *idapi.IdApi, elems: []const Oop) Oop {
    var v = s0(api.import("Vector"), "new");
    for (elems) |e| v = s1(v, "add:", e);
    return v;
}
fn lvar(api: *idapi.IdApi, name: [*:0]const u8) Oop {
    return s1(api.import("LogicVar"), "named:", api.intern(name));
}

// A fresh, independent store per test (the well-known `db` singleton persists
// across tests in one process, so isolate by minting a new seeded store).
fn freshDb(api: *idapi.IdApi) Oop {
    return s0(api.import("Store"), "new");
}

// A small fixture over `db`: three people with :city and :pet facts.
fn fixture(api: *idapi.IdApi, db: Oop) [3]Oop {
    const alice = s0(db, "entity");
    const bob = s0(db, "entity");
    const carol = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", alice, api.intern("city"), api.intern("Boston"));
    _ = s3(db, "assert:attr:value:", alice, api.intern("pet"), api.intern("Dog"));
    _ = s3(db, "assert:attr:value:", bob, api.intern("city"), api.intern("Boston"));
    _ = s3(db, "assert:attr:value:", bob, api.intern("pet"), api.intern("Cat"));
    _ = s3(db, "assert:attr:value:", carol, api.intern("city"), api.intern("Denver"));
    _ = s3(db, "assert:attr:value:", carol, api.intern("pet"), api.intern("Dog"));
    return .{ alice, bob, carol };
}

test "relational: LogicVar named: is canonical; entity mints distinct identities" {
    const api = setup();
    try std.testing.expect(lvar(api, "p") == lvar(api, "p")); // interned by name
    try std.testing.expect(lvar(api, "p") != lvar(api, "q"));

    const db = api.import("Store").?;
    try std.testing.expect(s0(db, "entity") != s0(db, "entity")); // fresh each time
    try std.testing.expect(api.import("db") == db); // `db` is the store
}

test "relational: findAll single clause with a constant value" {
    const api = setup();
    const db = freshDb(api);
    const people = fixture(api, db);

    // ?p where [?p #city #Boston]  → Alice + Bob
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") })});
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
    // membership (order is solution order: Alice then Bob)
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == people[0]);
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(1)) == people[1]);
}

test "relational: a logic var in the value position projects distinct values" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db);

    // ?c where [?p #city ?c]  → the distinct cities (Boston, Denver) — deduped.
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), lvar(api, "c") })});
    const r = s2(db, "findAll:where:", lvar(api, "c"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == api.intern("Boston"));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(1)) == api.intern("Denver"));
}

test "relational: two-clause conjunction joins on the shared variable" {
    const api = setup();
    const db = freshDb(api);
    const people = fixture(api, db);

    // ?p where [?p #city #Boston] AND [?p #pet #Dog]  → only Alice
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") }),
        vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Dog") }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == people[0]); // Alice
}

test "relational: no match yields an empty vector" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db);
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Tokyo") })});
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 0), ival(s0(r, "size")));
}

test "relational: pull builds a record map; absent attrs omitted" {
    const api = setup();
    const db = freshDb(api);
    const people = fixture(api, db);

    const spec = vec(api, &.{ api.intern("city"), api.intern("pet") });
    const rec = s2(db, "pull:spec:", people[0], spec);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(rec, "size")));
    try std.testing.expect(s1(rec, "at:", api.intern("city")) == api.intern("Boston"));
    try std.testing.expect(s1(rec, "at:", api.intern("pet")) == api.intern("Dog"));

    // an attribute the entity lacks is simply not in the record
    const spec2 = vec(api, &.{ api.intern("city"), api.intern("salary") });
    const rec2 = s2(db, "pull:spec:", people[0], spec2);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(rec2, "size")));
}

// Assert three people into `db`, keeping NO reference after return — so the
// entities live only inside the store's fact list (their GC survival depends on
// the store's root provider, not on lingering stack slots).
noinline fn populate(api: *idapi.IdApi, db: Oop) void {
    var i: usize = 0;
    while (i < 3) : (i += 1) {
        const e = s0(db, "entity");
        const city = if (i < 2) api.intern("Boston") else api.intern("Denver");
        _ = s3(db, "assert:attr:value:", e, api.intern("city"), city);
    }
}

test "relational: GC keeps store-only entities alive (root provider, F3)" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;

    const db = api.import("Store").?; // the singleton — covered by the store provider
    populate(api, db);

    // Collect: the entities are reachable only via the store's facts now.
    _ = api.gcCollect();

    // They survived and are still queryable (and their fact values intact).
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") })});
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
}

test "relational: Symbol printString renders with the # sigil" {
    const api = setup();
    try std.testing.expect(strEq(s0(api.intern("Boston"), "printString"), "#Boston"));
    try std.testing.expect(strEq(s0(lvar(api, "p"), "printString"), "?p"));
}
