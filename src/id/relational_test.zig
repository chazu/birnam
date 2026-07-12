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

test "relational: equal-valued Float bindings dedup to one solution (valEq by value)" {
    const api = setup();
    const db = freshDb(api);

    // Two people share height 1.5 (DISTINCT boxes); one is 1.75.
    const a = s0(db, "entity");
    const b = s0(db, "entity");
    const c = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", a, api.intern("height"), api.mkFloat(1.5));
    _ = s3(db, "assert:attr:value:", b, api.intern("height"), api.mkFloat(1.5));
    _ = s3(db, "assert:attr:value:", c, api.intern("height"), api.mkFloat(1.75));

    // ?h where [?p #height ?h] → the distinct heights {1.5, 1.75}, deduped by VALUE.
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("height"), lvar(api, "h") })});
    const r = s2(db, "findAll:where:", lvar(api, "h"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
    try std.testing.expectEqual(@as(f64, 1.5), api.floatValue(s1(r, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(f64, 1.75), api.floatValue(s1(r, "at:", api.mkSmallInt(1))));
}

test "relational: value-typed pattern constants match by VALUE (the footgun is closed)" {
    const api = setup();
    const db = freshDb(api);

    const a = s0(db, "entity");
    const b = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", a, api.intern("age"), api.mkSmallInt(33));
    _ = s3(db, "assert:attr:value:", b, api.intern("age"), api.mkSmallInt(44));
    _ = s3(db, "assert:attr:value:", a, api.intern("height"), api.mkFloat(1.5));
    _ = s3(db, "assert:attr:value:", a, api.intern("nick"), api.mkString("al", 2));

    // ?e where [?e #age 33] — a FRESH boxed 33 as the pattern constant matches
    // the asserted (distinct) box. This exact shape used to match nothing.
    const c1 = vec(api, &.{vec(api, &.{ lvar(api, "e"), api.intern("age"), api.mkSmallInt(33) })});
    const r1 = s2(db, "findAll:where:", lvar(api, "e"), c1);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r1, "size")));
    try std.testing.expect(s1(r1, "at:", api.mkSmallInt(0)) == a);

    // Float and String constants likewise; a non-matching value still misses.
    const c2 = vec(api, &.{vec(api, &.{ lvar(api, "e"), api.intern("height"), api.mkFloat(1.5) })});
    try std.testing.expectEqual(@as(i64, 1), ival(s0(s2(db, "findAll:where:", lvar(api, "e"), c2), "size")));
    const c3 = vec(api, &.{vec(api, &.{ lvar(api, "e"), api.intern("nick"), api.mkString("al", 2) })});
    try std.testing.expectEqual(@as(i64, 1), ival(s0(s2(db, "findAll:where:", lvar(api, "e"), c3), "size")));
    const c4 = vec(api, &.{vec(api, &.{ lvar(api, "e"), api.intern("age"), api.mkSmallInt(99) })});
    try std.testing.expectEqual(@as(i64, 0), ival(s0(s2(db, "findAll:where:", lvar(api, "e"), c4), "size")));

    // Join agreement is by value too: ?v bound from a's age must agree with b's
    // age fact through a DISTINCT equal-valued box.
    _ = s3(db, "assert:attr:value:", b, api.intern("age2"), api.mkSmallInt(44));
    const c5 = vec(api, &.{
        vec(api, &.{ lvar(api, "x"), api.intern("age"), lvar(api, "v") }),
        vec(api, &.{ lvar(api, "y"), api.intern("age2"), lvar(api, "v") }),
    });
    const r5 = s2(db, "findAll:where:", lvar(api, "x"), c5);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r5, "size")));
    try std.testing.expect(s1(r5, "at:", api.mkSmallInt(0)) == b);
}

test "relational: rule args unify value constants by VALUE" {
    const api = setup();
    const db = freshDb(api);

    const e1 = s0(db, "entity");
    const e2 = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", e1, api.intern("score"), api.mkSmallInt(7));
    _ = s3(db, "assert:attr:value:", e2, api.intern("score"), api.mkSmallInt(9));

    // scored(?p ?s) :- [?p #score ?s]; invoke with a fresh boxed 7 as the arg.
    const params = vec(api, &.{ lvar(api, "p"), lvar(api, "s") });
    const body = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("score"), lvar(api, "s") })});
    _ = s3(db, "defineRule:params:body:", api.intern("scored"), params, body);
    const call = vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("scored"), lvar(api, "p"), api.mkSmallInt(7) })});
    const r = s2(db, "findAll:where:", lvar(api, "p"), call);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == e1);
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

test "relational: negation excludes solutions whose inner pattern matches" {
    const api = setup();
    const db = freshDb(api);
    const people = fixture(api, db); // Alice(Boston,Dog) Bob(Boston,Cat) Carol(Denver,Dog)

    // ?p in Boston AND not pet=Cat → Alice only (Bob is the Boston cat-owner).
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") }),
        vec(api, &.{ api.intern("not"), vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Cat") }) }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == people[0]); // Alice
}

test "relational: negation as failure — no inner match keeps every solution" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db);

    // ?p with any city AND not pet=Fish → all three (nobody owns a Fish).
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("city"), lvar(api, "c") }),
        vec(api, &.{ api.intern("not"), vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Fish") }) }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 3), ival(s0(r, "size")));
}

test "relational: disjunction unions the branches" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db); // Alice(Boston,Dog) Bob(Boston,Cat) Carol(Denver,Dog)

    // ?p where (city=Boston OR city=Denver) → everyone.
    const clauses = vec(api, &.{
        vec(api, &.{
            api.intern("or"),
            vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") }),
            vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Denver") }),
        }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 3), ival(s0(r, "size")));
}

test "relational: disjunction across different attributes" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db);

    // ?p where (pet=Cat OR city=Denver) → Bob (cat) + Carol (Denver) = 2.
    const clauses = vec(api, &.{
        vec(api, &.{
            api.intern("or"),
            vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Cat") }),
            vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Denver") }),
        }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
}

test "relational: disjunction composes with a conjunction; projection dedups" {
    const api = setup();
    const db = freshDb(api);
    _ = fixture(api, db);

    // ?c where [?p #city ?c] AND (pet=Cat OR pet=Dog) → Boston (Alice/Bob) and
    // Denver (Carol) all qualify; projecting the city dedups to {Boston, Denver}.
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("city"), lvar(api, "c") }),
        vec(api, &.{
            api.intern("or"),
            vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Cat") }),
            vec(api, &.{ lvar(api, "p"), api.intern("pet"), api.intern("Dog") }),
        }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "c"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size"))); // Boston, Denver
}

// People with integer `:age` facts, returned so tests can identify matches.
fn ageFixture(api: *idapi.IdApi, db: Oop) [3]Oop {
    const young = s0(db, "entity"); // 18 — the boundary
    const mid = s0(db, "entity"); // 30
    const old = s0(db, "entity"); // 40
    _ = s3(db, "assert:attr:value:", young, api.intern("age"), api.mkSmallInt(18));
    _ = s3(db, "assert:attr:value:", mid, api.intern("age"), api.mkSmallInt(30));
    _ = s3(db, "assert:attr:value:", old, api.intern("age"), api.mkSmallInt(40));
    return .{ young, mid, old };
}

// `?p where [?p #age ?a] AND [?a <op> <rhs>]` → count of matching people.
fn ageQuery(api: *idapi.IdApi, db: Oop, op: [*:0]const u8, rhs: Oop) i64 {
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ lvar(api, "a"), api.intern(op), rhs }),
    });
    return ival(s0(s2(db, "findAll:where:", lvar(api, "p"), clauses), "size"));
}

test "relational: predicate clauses filter by comparison (named + glyph ops)" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db);
    const age18 = api.mkSmallInt(18);

    // Ordering ops, both spellings agree.
    try std.testing.expectEqual(@as(i64, 2), ageQuery(api, db, "gt", age18)); // 30,40
    try std.testing.expectEqual(@as(i64, 2), ageQuery(api, db, ">", age18));
    try std.testing.expectEqual(@as(i64, 3), ageQuery(api, db, "ge", age18)); // 18,30,40
    try std.testing.expectEqual(@as(i64, 3), ageQuery(api, db, ">=", age18));
    try std.testing.expectEqual(@as(i64, 0), ageQuery(api, db, "lt", age18)); // none < 18
    try std.testing.expectEqual(@as(i64, 1), ageQuery(api, db, "le", age18)); // just 18
    try std.testing.expectEqual(@as(i64, 1), ageQuery(api, db, "<=", age18));

    // Equality and its negation.
    try std.testing.expectEqual(@as(i64, 1), ageQuery(api, db, "eq", api.mkSmallInt(30)));
    try std.testing.expectEqual(@as(i64, 1), ageQuery(api, db, "=", api.mkSmallInt(30)));
    try std.testing.expectEqual(@as(i64, 2), ageQuery(api, db, "ne", api.mkSmallInt(30))); // 18,40
    try std.testing.expectEqual(@as(i64, 2), ageQuery(api, db, "~=", api.mkSmallInt(30)));
}

test "relational: the planner reorders a predicate written BEFORE its binder" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db); // 18, 30, 40

    // The predicate comes first, before ?a is bound — the planner defers it until
    // the triple binds ?a, so this still yields the two people over 18.
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "a"), api.intern("gt"), api.mkSmallInt(18) }),
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
}

test "relational: predicate projects the filtered entity, not a comparison op" {
    const api = setup();
    const db = freshDb(api);
    const people = ageFixture(api, db);

    // ?p where [?p #age ?a] AND [?a #gt 25] → the two older people (mid, old).
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ lvar(api, "a"), api.intern("gt"), api.mkSmallInt(25) }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == people[1]); // mid (30)
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(1)) == people[2]); // old (40)
}

test "relational: a constant left operand in a predicate is honored" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db);

    // [18 #lt ?a] keeps rows where 18 < age → 30 and 40.
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ api.mkSmallInt(18), api.intern("lt"), lvar(api, "a") }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
}

test "relational: negation may contain a predicate (applyClause recurses)" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db); // ages 18, 30, 40

    // ?p where [?p #age ?a] AND not [?a #eq 30] → everyone but the 30-year-old.
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ api.intern("not"), vec(api, &.{ lvar(api, "a"), api.intern("eq"), api.mkSmallInt(30) }) }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size"))); // the 18 and 40 year olds
}

test "relational: aggregates fold over distinct projected values" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db); // ages 18, 30, 40

    const ageVar = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") })});
    try std.testing.expectEqual(@as(i64, 3), ival(s2(db, "count:where:", lvar(api, "p"), ageVar)));
    try std.testing.expectEqual(@as(i64, 88), ival(s2(db, "sum:where:", lvar(api, "a"), ageVar))); // 18+30+40
    try std.testing.expectEqual(@as(i64, 18), ival(s2(db, "min:where:", lvar(api, "a"), ageVar)));
    try std.testing.expectEqual(@as(i64, 40), ival(s2(db, "max:where:", lvar(api, "a"), ageVar)));
}

test "relational: aggregates are distinct-by-default (set semantics)" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db);
    const dup = s0(db, "entity"); // a fourth person, age 30 — a duplicate value
    _ = s3(db, "assert:attr:value:", dup, api.intern("age"), api.mkSmallInt(30));

    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") })});
    // Four people, but only three DISTINCT ages.
    try std.testing.expectEqual(@as(i64, 4), ival(s2(db, "count:where:", lvar(api, "p"), clauses)));
    try std.testing.expectEqual(@as(i64, 3), ival(s2(db, "count:where:", lvar(api, "a"), clauses)));
    try std.testing.expectEqual(@as(i64, 88), ival(s2(db, "sum:where:", lvar(api, "a"), clauses))); // 30 counted once
}

test "relational: aggregates compose with predicate filters" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db);

    // sum of ages over 18 → 30 + 40 = 70.
    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ lvar(api, "a"), api.intern("gt"), api.mkSmallInt(18) }),
    });
    try std.testing.expectEqual(@as(i64, 70), ival(s2(db, "sum:where:", lvar(api, "a"), clauses)));
    try std.testing.expectEqual(@as(i64, 2), ival(s2(db, "count:where:", lvar(api, "p"), clauses)));
}

test "relational: groupBy partitions and aggregates per group" {
    const api = setup();
    const db = freshDb(api);
    // Four people across two cities, with ages.
    const mk = struct {
        fn p(a: *idapi.IdApi, d: Oop, city: Oop, age: i64) void {
            const e = s0(d, "entity");
            _ = s3(d, "assert:attr:value:", e, a.intern("city"), city);
            _ = s3(d, "assert:attr:value:", e, a.intern("age"), a.mkSmallInt(age));
        }
    }.p;
    mk(api, db, api.intern("Boston"), 20);
    mk(api, db, api.intern("Boston"), 40);
    mk(api, db, api.intern("Denver"), 30);
    mk(api, db, api.intern("Denver"), 30); // duplicate age 30 — distinct within group

    const clauses = vec(api, &.{
        vec(api, &.{ lvar(api, "p"), api.intern("city"), lvar(api, "c") }),
        vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") }),
    });

    // count of people per city (count is over distinct ?p, so 2 each).
    const counts = api.send(db, api.intern("groupBy:aggregate:of:where:"), &[_]Oop{ lvar(api, "c"), api.intern("count"), lvar(api, "p"), clauses }, 4);
    try std.testing.expectEqual(@as(i64, 2), ival(s1(counts, "at:", api.intern("Boston"))));
    try std.testing.expectEqual(@as(i64, 2), ival(s1(counts, "at:", api.intern("Denver"))));

    // sum of DISTINCT ages per city: Boston 20+40=60; Denver just 30 (deduped).
    const sums = api.send(db, api.intern("groupBy:aggregate:of:where:"), &[_]Oop{ lvar(api, "c"), api.intern("sum"), lvar(api, "a"), clauses }, 4);
    try std.testing.expectEqual(@as(i64, 60), ival(s1(sums, "at:", api.intern("Boston"))));
    try std.testing.expectEqual(@as(i64, 30), ival(s1(sums, "at:", api.intern("Denver"))));

    // max age per city.
    const maxes = api.send(db, api.intern("groupBy:aggregate:of:where:"), &[_]Oop{ lvar(api, "c"), api.intern("max"), lvar(api, "a"), clauses }, 4);
    try std.testing.expectEqual(@as(i64, 40), ival(s1(maxes, "at:", api.intern("Boston"))));
    try std.testing.expectEqual(@as(i64, 30), ival(s1(maxes, "at:", api.intern("Denver"))));
}

test "relational: aggregates over no matches — count/sum 0, min/max nil" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db);

    // Nobody has city facts here, so the query is empty.
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), lvar(api, "c") })});
    try std.testing.expectEqual(@as(i64, 0), ival(s2(db, "count:where:", lvar(api, "c"), clauses)));
    try std.testing.expectEqual(@as(i64, 0), ival(s2(db, "sum:where:", lvar(api, "c"), clauses)));
    try std.testing.expect(s2(db, "min:where:", lvar(api, "c"), clauses) == api.nilObj);
    try std.testing.expect(s2(db, "max:where:", lvar(api, "c"), clauses) == api.nilObj);
}

test "relational: findAll asOf: a basis sees the earlier snapshot" {
    const api = setup();
    const db = freshDb(api);

    // Two people in Boston, then capture the basis, then a third arrives.
    const a = s0(db, "entity");
    const b = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", a, api.intern("city"), api.intern("Boston"));
    _ = s3(db, "assert:attr:value:", b, api.intern("city"), api.intern("Boston"));
    const basis = s0(db, "basis"); // snapshot point: two Bostonians so far
    const c = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", c, api.intern("city"), api.intern("Boston"));

    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") })});
    // Current: three.
    try std.testing.expectEqual(@as(i64, 3), ival(s0(s2(db, "findAll:where:", lvar(api, "p"), clauses), "size")));
    // As of the basis: only the two that existed then.
    const past = s3(db, "findAll:where:asOf:", lvar(api, "p"), clauses, basis);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(past, "size")));
}

test "relational: asOf: reflects a value CHANGE on a history-on :one attribute" {
    const api = setup();
    const db = freshDb(api);

    // Opt the city attribute into history (default-off would overwrite in place,
    // discarding the past — D6); then a person in Boston moves to Denver.
    _ = s3(db, "assert:attr:value:", api.intern("city"), api.intern("history"), api.intern("true"));
    const p = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", p, api.intern("city"), api.intern("Boston"));
    const basis = s0(db, "basis");
    _ = s3(db, "assert:attr:value:", p, api.intern("city"), api.intern("Denver")); // :one supersedes

    const inBoston = vec(api, &.{vec(api, &.{ lvar(api, "x"), api.intern("city"), api.intern("Boston") })});
    const inDenver = vec(api, &.{vec(api, &.{ lvar(api, "x"), api.intern("city"), api.intern("Denver") })});

    // Now: in Denver, not Boston.
    try std.testing.expectEqual(@as(i64, 1), ival(s0(s2(db, "findAll:where:", lvar(api, "x"), inDenver), "size")));
    try std.testing.expectEqual(@as(i64, 0), ival(s0(s2(db, "findAll:where:", lvar(api, "x"), inBoston), "size")));
    // As of the basis: in Boston, not yet Denver.
    try std.testing.expectEqual(@as(i64, 1), ival(s0(s3(db, "findAll:where:asOf:", lvar(api, "x"), inBoston, basis), "size")));
    try std.testing.expectEqual(@as(i64, 0), ival(s0(s3(db, "findAll:where:asOf:", lvar(api, "x"), inDenver, basis), "size")));
}

test "relational: pull renders a :many attribute as a Vector of members" {
    const api = setup();
    const db = freshDb(api);
    // Declare :pet as :many in this store's schema.
    _ = s3(db, "assert:attr:value:", api.intern("pet"), api.intern("cardinality"), api.intern("many"));
    const p = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", p, api.intern("name"), api.intern("Alice"));
    _ = s3(db, "assert:attr:value:", p, api.intern("pet"), api.intern("Dog"));
    _ = s3(db, "assert:attr:value:", p, api.intern("pet"), api.intern("Cat"));

    const spec = vec(api, &.{ api.intern("name"), api.intern("pet") });
    const rec = s2(db, "pull:spec:", p, spec);
    // name is :one → scalar; pet is :many → a 2-member Vector.
    try std.testing.expect(s1(rec, "at:", api.intern("name")) == api.intern("Alice"));
    const pets = s1(rec, "at:", api.intern("pet"));
    try std.testing.expectEqual(@as(i64, 2), ival(s0(pets, "size")));
}

test "relational: pull recurses into a nested ref spec" {
    const api = setup();
    const db = freshDb(api);
    const alice = s0(db, "entity");
    const bob = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", alice, api.intern("name"), api.intern("Alice"));
    _ = s3(db, "assert:attr:value:", bob, api.intern("name"), api.intern("Bob"));
    _ = s3(db, "assert:attr:value:", alice, api.intern("friend"), bob); // alice → bob

    // pull alice's name and her friend's name (the friend is itself pulled).
    const sub = vec(api, &.{api.intern("name")});
    const refSpec = vec(api, &.{ api.intern("friend"), sub });
    const spec = vec(api, &.{ api.intern("name"), refSpec });
    const rec = s2(db, "pull:spec:", alice, spec);
    try std.testing.expect(s1(rec, "at:", api.intern("name")) == api.intern("Alice"));
    const friend = s1(rec, "at:", api.intern("friend")); // a sub-record Map
    try std.testing.expect(s1(friend, "at:", api.intern("name")) == api.intern("Bob"));
}

// A parent chain tom → bob → ann → kim (each `#parent` fact: parent then child).
fn lineage(api: *idapi.IdApi, db: Oop) [4]Oop {
    const tom = s0(db, "entity");
    const bob = s0(db, "entity");
    const ann = s0(db, "entity");
    const kim = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", tom, api.intern("parent"), bob);
    _ = s3(db, "assert:attr:value:", bob, api.intern("parent"), ann);
    _ = s3(db, "assert:attr:value:", ann, api.intern("parent"), kim);
    return .{ tom, bob, ann, kim };
}

// Define ancestor(?x,?y): base [?x #parent ?y]; recursive [?x #parent ?z] AND
// [#rule #ancestor ?z ?y]. Two definitions whose union is the transitive closure.
fn defineAncestor(api: *idapi.IdApi, db: Oop) void {
    const base = vec(api, &.{vec(api, &.{ lvar(api, "x"), api.intern("parent"), lvar(api, "y") })});
    _ = s3(db, "defineRule:params:body:", api.intern("ancestor"), vec(api, &.{ lvar(api, "x"), lvar(api, "y") }), base);
    const rec = vec(api, &.{
        vec(api, &.{ lvar(api, "x"), api.intern("parent"), lvar(api, "z") }),
        vec(api, &.{ api.intern("rule"), api.intern("ancestor"), lvar(api, "z"), lvar(api, "y") }),
    });
    _ = s3(db, "defineRule:params:body:", api.intern("ancestor"), vec(api, &.{ lvar(api, "x"), lvar(api, "y") }), rec);
}

test "relational: avg aggregates the mean of the distinct values" {
    const api = setup();
    const db = freshDb(api);
    _ = ageFixture(api, db); // 18, 30, 40
    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("age"), lvar(api, "a") })});
    // (18 + 30 + 40) / 3 = 88 / 3 = 29 (integer division truncates).
    try std.testing.expectEqual(@as(i64, 29), ival(s2(db, "avg:where:", lvar(api, "a"), clauses)));
}

test "relational: since: reports only triples that changed after a basis" {
    const api = setup();
    const db = freshDb(api);
    const a = s0(db, "entity");
    const b = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", a, api.intern("city"), api.intern("Boston"));
    _ = s3(db, "assert:attr:value:", b, api.intern("city"), api.intern("Boston"));
    const t1 = s0(db, "basis"); // two Bostonians committed so far
    const c = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", c, api.intern("city"), api.intern("Boston"));
    const t2 = s0(db, "basis");

    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") })});
    // Current: three; changed since t1: just the third.
    try std.testing.expectEqual(@as(i64, 3), ival(s0(s2(db, "findAll:where:", lvar(api, "p"), clauses), "size")));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(s3(db, "findAll:where:since:", lvar(api, "p"), clauses, t1), "size")));
    // Window [t1, t2): also just the third.
    const win = api.send(db, api.intern("findAll:where:between:and:"), &[_]Oop{ lvar(api, "p"), clauses, t1, t2 }, 4);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(win, "size")));
}

test "relational: a recursive rule computes the transitive closure" {
    const api = setup();
    const db = freshDb(api);
    const p = lineage(api, db); // tom bob ann kim
    defineAncestor(api, db);

    // All of tom's ancestors-of (descendants): bob, ann, kim.
    const q = vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("ancestor"), p[0], lvar(api, "y") })});
    const r = s2(db, "findAll:where:", lvar(api, "y"), q);
    try std.testing.expectEqual(@as(i64, 3), ival(s0(r, "size")));

    // Count via the aggregate path, too.
    try std.testing.expectEqual(@as(i64, 3), ival(s2(db, "count:where:", lvar(api, "y"), q)));
}

test "relational: rules survive a cache rebuild from their canonical facts (docs/42 §D2)" {
    const api = setup();
    const db = freshDb(api);
    const p = lineage(api, db);
    defineAncestor(api, db);
    const q = vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("ancestor"), p[0], lvar(api, "y") })});
    try std.testing.expectEqual(@as(i64, 3), ival(s0(s2(db, "findAll:where:", lvar(api, "y"), q), "size")));

    // [db rebuildRules] wipes the in-memory rule cache and repopulates it PURELY
    // from the stored [pair __rule-name__ name] facts — exactly what happens after
    // an image load replaces the fact stream. If the definitions weren't reified as
    // durable data, the rule would vanish and the query would return nothing.
    _ = s0(db, "rebuildRules");
    try std.testing.expectEqual(@as(i64, 3), ival(s0(s2(db, "findAll:where:", lvar(api, "y"), q), "size")));
}

test "relational: mutually-recursive rules converge (even/odd over a succ chain)" {
    const api = setup();
    const db = freshDb(api);
    // n0 → n1 → n2 → n3 → n4 via #succ; n0 is the zero.
    var ns: [5]Oop = undefined;
    for (&ns) |*e| e.* = s0(db, "entity");
    _ = s3(db, "assert:attr:value:", ns[0], api.intern("isZero"), api.intern("yes"));
    var i: usize = 0;
    while (i < 4) : (i += 1) _ = s3(db, "assert:attr:value:", ns[i], api.intern("succ"), ns[i + 1]);

    // even(?x) :- x isZero;  even(?x) :- ?y succ ?x, odd(?y).
    // odd(?x)  :- ?y succ ?x, even(?y).   (mutual recursion)
    _ = s3(db, "defineRule:params:body:", api.intern("even"), vec(api, &.{lvar(api, "x")}), vec(api, &.{vec(api, &.{ lvar(api, "x"), api.intern("isZero"), api.intern("yes") })}));
    _ = s3(db, "defineRule:params:body:", api.intern("even"), vec(api, &.{lvar(api, "x")}), vec(api, &.{
        vec(api, &.{ lvar(api, "y"), api.intern("succ"), lvar(api, "x") }),
        vec(api, &.{ api.intern("rule"), api.intern("odd"), lvar(api, "y") }),
    }));
    _ = s3(db, "defineRule:params:body:", api.intern("odd"), vec(api, &.{lvar(api, "x")}), vec(api, &.{
        vec(api, &.{ lvar(api, "y"), api.intern("succ"), lvar(api, "x") }),
        vec(api, &.{ api.intern("rule"), api.intern("even"), lvar(api, "y") }),
    }));

    const evens = s2(db, "findAll:where:", lvar(api, "x"), vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("even"), lvar(api, "x") })}));
    const odds = s2(db, "findAll:where:", lvar(api, "x"), vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("odd"), lvar(api, "x") })}));
    try std.testing.expectEqual(@as(i64, 3), ival(s0(evens, "size"))); // n0, n2, n4
    try std.testing.expectEqual(@as(i64, 2), ival(s0(odds, "size"))); // n1, n3
}

test "relational: a rule clause binds the reverse direction (constant 2nd arg)" {
    const api = setup();
    const db = freshDb(api);
    const p = lineage(api, db); // tom bob ann kim
    defineAncestor(api, db);

    // Who are kim's ancestors? [#rule #ancestor ?x kim] → tom, bob, ann (set of 3).
    const q = vec(api, &.{vec(api, &.{ api.intern("rule"), api.intern("ancestor"), lvar(api, "x"), p[3] })});
    const r = s2(db, "findAll:where:", lvar(api, "x"), q);
    try std.testing.expectEqual(@as(i64, 3), ival(s0(r, "size")));
    // bob is NOT among them only if... actually bob IS (bob→ann→kim); tom/bob/ann all are.
    // kim is not its own ancestor:
    var sawKim = false;
    var i: i64 = 0;
    while (i < 3) : (i += 1) if (s1(r, "at:", api.mkSmallInt(i)) == p[3]) {
        sawKim = true;
    };
    try std.testing.expect(!sawKim);
}

test "relational: a rule clause composes with a predicate and a plain triple" {
    const api = setup();
    const db = freshDb(api);
    const p = lineage(api, db); // tom bob ann kim
    defineAncestor(api, db);
    // Tag two people with an :age so a predicate can filter rule results.
    _ = s3(db, "assert:attr:value:", p[2], api.intern("age"), api.mkSmallInt(40)); // ann
    _ = s3(db, "assert:attr:value:", p[3], api.intern("age"), api.mkSmallInt(10)); // kim

    // tom's descendants who have an age over 18 → just ann (40).
    const q = vec(api, &.{
        vec(api, &.{ api.intern("rule"), api.intern("ancestor"), p[0], lvar(api, "y") }),
        vec(api, &.{ lvar(api, "y"), api.intern("age"), lvar(api, "a") }),
        vec(api, &.{ lvar(api, "a"), api.intern("gt"), api.mkSmallInt(18) }),
    });
    const r = s2(db, "findAll:where:", lvar(api, "y"), q);
    try std.testing.expectEqual(@as(i64, 1), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "at:", api.mkSmallInt(0)) == p[2]); // ann
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

test "relational: GC trace hook keeps a fresh [Store new] store's facts alive (F3)" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;

    // A fresh store, held only in this local (reachable via the stack scan). Its
    // entities live solely in its fact list → they survive only via store_vt's
    // per-instance trace hook, not via the singleton root.
    const db = freshDb(api);
    populate(api, db);
    _ = api.gcCollect();

    const clauses = vec(api, &.{vec(api, &.{ lvar(api, "p"), api.intern("city"), api.intern("Boston") })});
    const r = s2(db, "findAll:where:", lvar(api, "p"), clauses);
    try std.testing.expectEqual(@as(i64, 2), ival(s0(r, "size")));
}

test "relational: Symbol printString renders with the # sigil" {
    const api = setup();
    try std.testing.expect(strEq(s0(api.intern("Boston"), "printString"), "#Boston"));
    try std.testing.expect(strEq(s0(lvar(api, "p"), "printString"), "?p"));
}
