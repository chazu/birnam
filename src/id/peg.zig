//! src/id/peg.zig — rung 3a: a PEG engine as a module over libid.
//!
//! See `docs/32` (rung 3) and `docs/34` F5. The matching *semantics* are ported
//! from the old fact-store kernel's `src/kernel/peg.zig` (proven operator set +
//! packrat memoization + zero-width guard), but the representation is libid-
//! native, not fact-store-welded:
//!
//!   * **Grammar patterns are libid objects** (a `Pattern` is a libid object whose
//!     payload is a `Pat` struct read by pointer — fast, no rung-2 dependency).
//!     "Grammars as objects" (F5); they can be projected into the cell store for
//!     reflection later, mirroring the rung-2 dispatch decision.
//!   * **Parse output is a TRANSIENT CST** (`Node` tree in a caller-owned arena),
//!     NOT facts (F5 / D6): built per parse, consumed by codegen, discarded. The
//!     durable artifact downstream is the compiled method, not the parse tree.
//!
//! The operator set: literal / any / eof / charset / seq / choice / star / plus /
//! opt / and / not / rule-ref. Only a rule application produces a CST node (a
//! span [start,end) labelled by rule name, with nested rule-matches as children);
//! terminals just advance the position.
//!
//! Depends only on the `IdApi` (like `store.zig`); loaded by the siphon.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

pub const Range = struct { lo: u8, hi: u8 };

const Kind = enum(u8) {
    literal,
    any,
    eof,
    charset,
    seq,
    choice,
    star,
    plus,
    opt,
    p_and,
    p_not,
    rule_ref,
};

/// A grammar pattern — the payload of a `Pattern` libid object. Only the fields
/// relevant to `kind` are populated.
const Pat = struct {
    kind: Kind,
    literal: []const u8 = &.{}, // literal
    children: []const Oop = &.{}, // seq/choice/star/plus/opt/and/not (child Patterns)
    ranges: []const Range = &.{}, // charset
    rule_name: Oop = null, // rule_ref (an interned symbol)
};

/// A grammar — the payload of a `Grammar` libid object: a list of named rules.
const Rule = struct { name: Oop, pat: Oop };
const Gram = struct { rules: std.ArrayList(Rule) = .{} };

// -- Well-known objects (set by bringUp) ------------------------------------

pub var pattern_vt: Oop = null;
pub var grammar_vt: Oop = null;
var g_api: *idapi.IdApi = undefined;

fn patOf(p: Oop) *Pat {
    return @ptrCast(@alignCast(p.?));
}
fn gramOf(g: Oop) *Gram {
    return @ptrCast(@alignCast(g.?));
}

// -- Pattern constructors (Zig API; grammars are built programmatically) -----
// Patterns are long-lived (the grammar), so their backing slices are duped into
// the c_allocator. Each returned Oop is a real libid object (vtable = pattern_vt).

fn newPattern(p: Pat) Oop {
    const o = g_api.alloc(pattern_vt, @sizeOf(Pat));
    patOf(o).* = p;
    return o;
}
pub fn mkLiteral(api: *idapi.IdApi, bytes: []const u8) Oop {
    _ = api;
    return newPattern(.{ .kind = .literal, .literal = ca.dupe(u8, bytes) catch @panic("peg: oom") });
}
pub fn mkAny(api: *idapi.IdApi) Oop {
    _ = api;
    return newPattern(.{ .kind = .any });
}
pub fn mkEof(api: *idapi.IdApi) Oop {
    _ = api;
    return newPattern(.{ .kind = .eof });
}
pub fn mkCharset(api: *idapi.IdApi, ranges: []const Range) Oop {
    _ = api;
    return newPattern(.{ .kind = .charset, .ranges = ca.dupe(Range, ranges) catch @panic("peg: oom") });
}
pub fn mkSeq(api: *idapi.IdApi, children: []const Oop) Oop {
    _ = api;
    return newPattern(.{ .kind = .seq, .children = ca.dupe(Oop, children) catch @panic("peg: oom") });
}
pub fn mkChoice(api: *idapi.IdApi, children: []const Oop) Oop {
    _ = api;
    return newPattern(.{ .kind = .choice, .children = ca.dupe(Oop, children) catch @panic("peg: oom") });
}
fn mkUnary(kind: Kind, child: Oop) Oop {
    return newPattern(.{ .kind = kind, .children = ca.dupe(Oop, &[_]Oop{child}) catch @panic("peg: oom") });
}
pub fn mkStar(api: *idapi.IdApi, child: Oop) Oop {
    _ = api;
    return mkUnary(.star, child);
}
pub fn mkPlus(api: *idapi.IdApi, child: Oop) Oop {
    _ = api;
    return mkUnary(.plus, child);
}
pub fn mkOpt(api: *idapi.IdApi, child: Oop) Oop {
    _ = api;
    return mkUnary(.opt, child);
}
pub fn mkAnd(api: *idapi.IdApi, child: Oop) Oop {
    _ = api;
    return mkUnary(.p_and, child);
}
pub fn mkNot(api: *idapi.IdApi, child: Oop) Oop {
    _ = api;
    return mkUnary(.p_not, child);
}
pub fn mkRuleRef(api: *idapi.IdApi, name: Oop) Oop {
    _ = api;
    return newPattern(.{ .kind = .rule_ref, .rule_name = name });
}

// -- Grammar -----------------------------------------------------------------

pub fn mkGrammar(api: *idapi.IdApi) Oop {
    const g = api.alloc(grammar_vt, @sizeOf(Gram));
    gramOf(g).* = .{};
    return g;
}
pub fn defineRule(g: Oop, name: Oop, pat: Oop) void {
    gramOf(g).rules.append(ca, .{ .name = name, .pat = pat }) catch @panic("peg: oom");
}
fn ruleNamed(g: Oop, name: Oop) ?Oop {
    for (gramOf(g).rules.items) |r| if (r.name == name) return r.pat;
    return null;
}

// -- The transient CST (caller-owned arena; NOT facts) -----------------------

pub const Node = struct {
    rule: Oop, // the rule name (symbol) this node matched
    start: usize,
    end: usize,
    children: []*Node,
};

const no_children: []*Node = &[_]*Node{};

const Match = struct { end: usize, children: []*Node };
const MemoKey = struct { pat: Oop, pos: usize };
const MemoEntry = struct { ok: bool, end: usize, children: []*Node };

const RunState = struct {
    g: Oop,
    source: []const u8,
    na: std.mem.Allocator, // node arena (caller-owned; the transient CST lives here)
    memo: *std.AutoHashMap(MemoKey, MemoEntry),
    furthest: usize = 0, // furthest failure position (for diagnostics later)

    fn note(rs: *RunState, pos: usize) void {
        if (pos > rs.furthest) rs.furthest = pos;
    }
};

// -- The match engine (ported semantics; reads pattern objects by pointer) ----

fn matchPattern(rs: *RunState, pat: Oop, pos: usize) ?Match {
    const p = patOf(pat);
    switch (p.kind) {
        .literal => {
            const want = p.literal;
            if (pos + want.len > rs.source.len or !std.mem.eql(u8, rs.source[pos .. pos + want.len], want)) {
                rs.note(pos);
                return null;
            }
            return Match{ .end = pos + want.len, .children = no_children };
        },
        .any => {
            if (pos >= rs.source.len) {
                rs.note(pos);
                return null;
            }
            return Match{ .end = pos + 1, .children = no_children };
        },
        .eof => {
            if (pos != rs.source.len) {
                rs.note(pos);
                return null;
            }
            return Match{ .end = pos, .children = no_children };
        },
        .charset => {
            if (pos >= rs.source.len) {
                rs.note(pos);
                return null;
            }
            const c = rs.source[pos];
            for (p.ranges) |r| if (c >= r.lo and c <= r.hi) return Match{ .end = pos + 1, .children = no_children };
            rs.note(pos);
            return null;
        },
        .seq => {
            var kids: std.ArrayList(*Node) = .{};
            var at = pos;
            for (p.children) |child| {
                const m = matchPattern(rs, child, at) orelse return null;
                kids.appendSlice(rs.na, m.children) catch @panic("peg: oom");
                at = m.end;
            }
            return Match{ .end = at, .children = kids.toOwnedSlice(rs.na) catch @panic("peg: oom") };
        },
        .choice => {
            for (p.children) |child| if (matchPattern(rs, child, pos)) |m| return m;
            return null;
        },
        .star, .plus => {
            var kids: std.ArrayList(*Node) = .{};
            var at = pos;
            var count: usize = 0;
            while (matchPattern(rs, p.children[0], at)) |m| {
                kids.appendSlice(rs.na, m.children) catch @panic("peg: oom");
                if (m.end == at) break; // zero-width-success guard: no infinite loop
                at = m.end;
                count += 1;
            }
            if (p.kind == .plus and count == 0) return null;
            return Match{ .end = at, .children = kids.toOwnedSlice(rs.na) catch @panic("peg: oom") };
        },
        .opt => {
            if (matchPattern(rs, p.children[0], pos)) |m| return m;
            return Match{ .end = pos, .children = no_children };
        },
        .p_and => {
            if (matchPattern(rs, p.children[0], pos)) |_| return Match{ .end = pos, .children = no_children };
            return null;
        },
        .p_not => {
            if (matchPattern(rs, p.children[0], pos)) |_| return null;
            return Match{ .end = pos, .children = no_children };
        },
        .rule_ref => {
            const rule_pat = ruleNamed(rs.g, p.rule_name) orelse return null;
            const key = MemoKey{ .pat = rule_pat, .pos = pos }; // packrat at the rule boundary
            if (rs.memo.get(key)) |entry| {
                if (!entry.ok) return null;
                return Match{ .end = entry.end, .children = entry.children };
            }
            if (matchPattern(rs, rule_pat, pos)) |inner| {
                const node = rs.na.create(Node) catch @panic("peg: oom");
                node.* = .{ .rule = p.rule_name, .start = pos, .end = inner.end, .children = inner.children };
                const wrapped = rs.na.alloc(*Node, 1) catch @panic("peg: oom");
                wrapped[0] = node;
                rs.memo.put(key, .{ .ok = true, .end = inner.end, .children = wrapped }) catch @panic("peg: oom");
                return Match{ .end = inner.end, .children = wrapped };
            }
            rs.memo.put(key, .{ .ok = false, .end = pos, .children = no_children }) catch @panic("peg: oom");
            return null;
        },
    }
}

/// Parse `source` against `g` starting at the rule `root_name`. Allocates the
/// transient CST in `na` (a caller-owned arena, freed after codegen). Returns the
/// root `*Node` on success, or null on failure (richer diagnostics come later).
pub fn parse(api: *idapi.IdApi, na: std.mem.Allocator, g: Oop, source: []const u8, root_name: Oop) ?*Node {
    _ = api;
    var memo = std.AutoHashMap(MemoKey, MemoEntry).init(ca);
    defer memo.deinit();
    var rs = RunState{ .g = g, .source = source, .na = na, .memo = &memo };
    const root_pat = ruleNamed(g, root_name) orelse return null;
    if (matchPattern(&rs, root_pat, 0)) |m| {
        const node = na.create(Node) catch @panic("peg: oom");
        node.* = .{ .rule = root_name, .start = 0, .end = m.end, .children = m.children };
        return node;
    }
    return null;
}

// -- Bringup / module entrypoint --------------------------------------------

/// Wire peg onto libid: build (or re-import) the Pattern + Grammar vtables.
/// Patterns/grammars are data objects (read by pointer in the hot match loop);
/// no methods are installed on them yet — a message facade for building grammars
/// from birnam source arrives with collection literals.
pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const reloading = api.isExported("PegPatternVt") != 0;
    if (reloading) {
        pattern_vt = api.import("PegPatternVt");
        grammar_vt = api.import("PegGrammarVt");
    } else {
        pattern_vt = api.proto(api.object_vt);
        grammar_vt = api.proto(api.object_vt);
        _ = api.exportObj("PegPatternVt", pattern_vt);
        _ = api.exportObj("PegGrammarVt", grammar_vt);
    }
}

pub export fn module_init(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
