//! src/id/relational.zig — the relational reading of the store: a datalog join.
//!
//! The rung-2 store (`store.zig`) holds facts and answers POINT reads
//! (`currentValue:attr:`). This module adds the RELATIONAL reading — pattern
//! queries over the same facts — realizing the North Star's "object model and
//! relational model are one store read two ways" (docs/30, D5.2). It is a sibling
//! module over the IdApi: it imports the running `Store` and INSTALLS its query
//! protocol onto the store's own vtable, so the same well-known object answers
//! both `assert:attr:value:` and `findAll:where:`. That object is also exported as
//! `db` — the name the example programs use.
//!
//! The core surface (the rest of the old kernel's datalog — predicates, negation,
//! disjunction, asOf/since, aggregation, rules, nested-ref pull — grows on top):
//!
//!   [db entity]                       → a fresh blank entity (a unique identity)
//!   [db assert: e attr: a value: v]   → (inherited from the store)
//!   [db findAll: ?v where: clauses]   → a Vector of every binding of ?v
//!   [db pull: e spec: #[ #attr … ]]   → a {attr: currentValue} record Map
//!
//! A clause is `#[ e a v ]` (a Vector); each position is either a CONSTANT (matched
//! by identity) or a LOGIC VARIABLE (`?p` in source). Logic variables are minted
//! by the grammar/codegen as `[LogicVar named: #p]` — canonical per name, so the
//! `?p` in the result position and the `?p` inside a clause are the SAME object and
//! join by identity. The engine scans the store's LIVE triples (via the
//! `liveTriplesDo:` seam) and joins clauses left-to-right, threading bindings.
//!
//! Depends on the IdApi + the Store; uses the collection factories (`Vector`/`Map`)
//! lazily at query time, so it loads after them without a bringup-order knot.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var logicvar_vt: Oop = null; // a logic variable (payload = its name symbol)
pub var logicvar_factory_vt: Oop = null;
pub var logicvar_factory: Oop = null; // the "LogicVar" facade ([LogicVar named: #p])
pub var entity_vt: Oop = null; // a blank minted entity
pub var collector_vt: Oop = null; // the liveTriplesDo: accumulator block

// Cached selectors.
var s_value3: Oop = null;
var s_size: Oop = null;
var s_at: Oop = null;
var s_atput: Oop = null;
var s_new: Oop = null;
var s_add: Oop = null;
var s_currentValue: Oop = null;
var s_liveTriplesDo: Oop = null;

// Canonical logic vars, interned by their name symbol.
var g_logicvars: std.AutoHashMapUnmanaged(Oop, Oop) = .{};

// Collection factories (Vector/Map), imported lazily on first query.
var g_vector_factory: Oop = null;
var g_map_factory: Oop = null;
fn vectorFactory() Oop {
    if (g_vector_factory == null) g_vector_factory = g_api.import("Vector");
    return g_vector_factory;
}
fn mapFactory() Oop {
    if (g_map_factory == null) g_map_factory = g_api.import("Map");
    return g_map_factory;
}

fn send0(rcv: Oop, sel: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{}, 0);
}
fn send1(rcv: Oop, sel: Oop, a: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{a}, 1);
}
fn send2(rcv: Oop, sel: Oop, a: Oop, b: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{ a, b }, 2);
}

// -- logic variables ---------------------------------------------------------

fn lvNameOf(o: Oop) Oop {
    const slot: *Oop = @ptrCast(@alignCast(o.?));
    return slot.*;
}
fn isLogicVar(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == logicvar_vt;
}

/// `[LogicVar named: #p]` — the canonical logic variable for symbol `#p` (interned
/// so equal names share identity, which is how the join matches them).
fn mNamed(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const sym = args[0];
    if (g_logicvars.get(sym)) |lv| return lv;
    const lv = g_api.alloc(logicvar_vt, @sizeOf(Oop));
    const slot: *Oop = @ptrCast(@alignCast(lv.?));
    slot.* = sym;
    g_logicvars.put(ca, sym, lv) catch @panic("relational: oom");
    return lv;
}

fn mLvName(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return lvNameOf(self);
}

/// `?name` — a logic variable renders with its `?` sigil.
fn mLvPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nm = std.mem.span(g_api.symbolName(lvNameOf(self)));
    const buf = ca.alloc(u8, nm.len + 1) catch @panic("relational: oom");
    defer ca.free(buf);
    buf[0] = '?';
    @memcpy(buf[1..], nm);
    return g_api.mkString(buf.ptr, buf.len);
}

/// `#name` — a symbol renders with its `#` sigil (records show symbol keys/values).
fn mSymPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nm = std.mem.span(g_api.symbolName(self));
    const buf = ca.alloc(u8, nm.len + 1) catch @panic("relational: oom");
    defer ca.free(buf);
    buf[0] = '#';
    @memcpy(buf[1..], nm);
    return g_api.mkString(buf.ptr, buf.len);
}

// -- entities ----------------------------------------------------------------

/// `[db entity]` — mint a fresh blank entity. Identity IS the entity (the floor's
/// pointer identity, the store's matching key); it carries facts via `assert:…`.
fn mEntity(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.alloc(entity_vt, 0);
}

fn mEntityPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const s = "anEntity";
    return g_api.mkString(s.ptr, s.len);
}

// -- the live-triple collector (a block the store drives) --------------------

const Triple = struct { e: Oop, a: Oop, v: Oop };
const Triples = std.ArrayList(Triple);

/// The store sends `value:value:value:` here once per live triple; we push it into
/// the Zig list whose address is stashed in the block's payload.
fn mCollect3(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const slot: *usize = @ptrCast(@alignCast(self.?));
    const lst: *Triples = @ptrFromInt(slot.*);
    lst.append(ca, .{ .e = args[0], .a = args[1], .v = args[2] }) catch @panic("relational: oom");
    return g_api.nilObj;
}

fn gatherTriples(store: Oop, out: *Triples) void {
    const blk = g_api.alloc(collector_vt, @sizeOf(usize));
    const slot: *usize = @ptrCast(@alignCast(blk.?));
    slot.* = @intFromPtr(out);
    _ = send1(store, s_liveTriplesDo, blk);
}

// -- the join ----------------------------------------------------------------

const Binding = struct { v: Oop, val: Oop };

fn lookup(sol: []const Binding, v: Oop) ?Oop {
    for (sol) |b| if (b.v == v) return b.val;
    return null;
}

/// Extend solution `sol` by matching clause positions `pos` against triple `tri`.
/// A logic-var position binds (or must agree with an existing binding); a constant
/// must equal (by identity). Returns the extended binding set, or null on mismatch.
fn tryClause(aa: std.mem.Allocator, sol: []const Binding, pos: [3]Oop, tri: Triple) ?[]Binding {
    var b: std.ArrayList(Binding) = .{};
    b.appendSlice(aa, sol) catch @panic("relational: oom");
    const vals = [3]Oop{ tri.e, tri.a, tri.v };
    for (pos, vals) |p, val| {
        if (isLogicVar(p)) {
            if (lookup(b.items, p)) |existing| {
                if (existing != val) return null;
            } else {
                b.append(aa, .{ .v = p, .val = val }) catch @panic("relational: oom");
            }
        } else if (p != val) return null;
    }
    return b.toOwnedSlice(aa) catch @panic("relational: oom");
}

/// `[db findAll: ?v where: clauses]` — every distinct binding of `?v` across all
/// joined solutions of the clause conjunction. `clauses` is a Vector of `#[e a v]`
/// clause Vectors. Returns a Vector (empty if nothing matches).
fn mFindAll(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const result_var = args[0];
    const clauses = args[1];

    var triples: Triples = .{};
    defer triples.deinit(ca);
    gatherTriples(self, &triples);

    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const aa = arena.allocator();

    // Begin with one empty solution; each clause filters/extends the set.
    var solutions: std.ArrayList([]Binding) = .{};
    solutions.append(aa, &[_]Binding{}) catch @panic("relational: oom");

    const nclauses: usize = @intCast(g_api.smallIntValue(send0(clauses, s_size)));
    var c: usize = 0;
    while (c < nclauses) : (c += 1) {
        const clause = send1(clauses, s_at, g_api.mkSmallInt(@intCast(c)));
        var pos: [3]Oop = undefined;
        for (&pos, 0..) |*p, i| p.* = send1(clause, s_at, g_api.mkSmallInt(@intCast(i)));

        var next: std.ArrayList([]Binding) = .{};
        for (solutions.items) |sol| {
            for (triples.items) |tri| {
                if (tryClause(aa, sol, pos, tri)) |ns|
                    next.append(aa, ns) catch @panic("relational: oom");
            }
        }
        solutions = next;
    }

    // Project the result variable, dropping duplicates (set semantics).
    var out = send0(vectorFactory(), s_new);
    var seen: std.ArrayList(Oop) = .{};
    defer seen.deinit(ca);
    for (solutions.items) |sol| {
        const val = lookup(sol, result_var) orelse continue;
        var dup = false;
        for (seen.items) |s| if (s == val) {
            dup = true;
            break;
        };
        if (dup) continue;
        seen.append(ca, val) catch @panic("relational: oom");
        out = send1(out, s_add, val);
    }
    return out;
}

/// `[db pull: e spec: #[ #attr … ]]` — materialize `e` into a record Map of each
/// requested attribute to its current value (absent attributes are omitted).
fn mPull(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const e = args[0];
    const spec = args[1];
    var m = send0(mapFactory(), s_new);
    const sz: usize = @intCast(g_api.smallIntValue(send0(spec, s_size)));
    var i: usize = 0;
    while (i < sz) : (i += 1) {
        const a = send1(spec, s_at, g_api.mkSmallInt(@intCast(i)));
        const v = send2(self, s_currentValue, e, a);
        if (v != null) m = send2(m, s_atput, a, v);
    }
    return m;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const store = api.import("Store") orelse @panic("relational: Store not loaded");
    const store_vt = api.import("StoreVt") orelse @panic("relational: StoreVt not loaded");

    s_value3 = api.intern("value:value:value:");
    s_size = api.intern("size");
    s_at = api.intern("at:");
    s_atput = api.intern("at:put:");
    s_new = api.intern("new");
    s_add = api.intern("add:");
    s_currentValue = api.intern("currentValue:attr:");
    s_liveTriplesDo = api.intern("liveTriplesDo:");

    const reloading = api.isExported("db") != 0;
    if (reloading) {
        logicvar_vt = api.import("LogicVarVt");
        logicvar_factory_vt = api.import("LogicVarFactoryVt");
        logicvar_factory = api.import("LogicVar");
        entity_vt = api.import("EntityVt");
        collector_vt = api.import("RelCollectorVt");
    } else {
        logicvar_vt = api.proto(api.object_vt);
        logicvar_factory_vt = api.proto(api.object_vt);
        logicvar_factory = api.alloc(logicvar_factory_vt, 0);
        entity_vt = api.proto(api.object_vt);
        collector_vt = api.proto(api.object_vt);
    }

    // The query protocol is installed onto the STORE's own vtable — the same
    // object answers both the imperative and relational readings (D5.2).
    api.method(store_vt, api.intern("entity"), &mEntity);
    api.method(store_vt, api.intern("findAll:where:"), &mFindAll);
    api.method(store_vt, api.intern("pull:spec:"), &mPull);

    api.method(logicvar_factory_vt, api.intern("named:"), &mNamed);
    api.method(logicvar_vt, api.intern("name"), &mLvName);
    api.method(logicvar_vt, api.intern("printString"), &mLvPrintString);
    api.method(entity_vt, api.intern("printString"), &mEntityPrintString);
    api.method(collector_vt, s_value3, &mCollect3);

    // Symbols render with their sigil (records print symbol keys/values).
    api.method(api.symbol_vt, api.intern("printString"), &mSymPrintString);

    if (!reloading) {
        _ = api.exportObj("LogicVarVt", logicvar_vt);
        _ = api.exportObj("LogicVarFactoryVt", logicvar_factory_vt);
        _ = api.exportObj("LogicVar", logicvar_factory);
        _ = api.exportObj("EntityVt", entity_vt);
        _ = api.exportObj("RelCollectorVt", collector_vt);
        _ = api.exportObj("db", store); // the store, under the name the examples use
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
