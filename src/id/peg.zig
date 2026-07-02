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

// GC trace hooks (F3): patterns hold their children, and grammars their rules,
// in c_allocator memory the payload scan can't see — without these hooks a
// collection sweeps the pattern graph out from under the (module-global,
// unscanned) grammar reference and the next parse is a use-after-free.
fn patTrace(p: Oop, mark: idapi.MarkFn) callconv(.c) void {
    const pat = patOf(p);
    for (pat.children) |ch| mark(ch);
    if (pat.rule_name != null) mark(pat.rule_name);
}
fn gramTrace(g: Oop, mark: idapi.MarkFn) callconv(.c) void {
    for (gramOf(g).rules.items) |r| {
        mark(r.name);
        mark(r.pat);
    }
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

/// Append a child to a seq/choice pattern (the builder; grammar construction is
/// one-time, so reallocating per add is fine). The empty default slice (`&.{}`)
/// is comptime, so only previously-grown heap slices are freed.
fn addChild(pat: Oop, child: Oop) void {
    const p = patOf(pat);
    const old = p.children;
    const new = ca.alloc(Oop, old.len + 1) catch @panic("peg: oom");
    @memcpy(new[0..old.len], old);
    new[old.len] = child;
    if (old.len > 0) ca.free(@constCast(old));
    p.children = new;
}
fn addRange(pat: Oop, lo: u8, hi: u8) void {
    const p = patOf(pat);
    const old = p.ranges;
    const new = ca.alloc(Range, old.len + 1) catch @panic("peg: oom");
    @memcpy(new[0..old.len], old);
    new[old.len] = .{ .lo = lo, .hi = hi };
    if (old.len > 0) ca.free(@constCast(old));
    p.ranges = new;
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

// -- The message (id) api: peg through the object model ----------------------
//
// peg is driven through `api.send`, so `lang` (a sibling module) builds the
// birnam grammar + parser by messaging these objects — no Zig linkage between
// modules. The `Peg` facade mints patterns; patterns answer the combinators;
// the `Grammar` defines rules and parses; the parse result is a tree of `CstNode`
// objects (message-accessible, still transient — not facts).

pub var peg_vt: Oop = null; // the Peg facade's vtable
pub var peg_obj: Oop = null; // the Peg facade singleton (exported "Peg")
pub var cstnode_vt: Oop = null;

/// A message-accessible CST node — the libid wrapper over a transient parse
/// `Node`. Heap libid objects (leak until gc, like everything); the Zig `Node`
/// arena is freed right after conversion.
const Cst = struct { rule: Oop, start: usize, end: usize, children: []Oop };
fn cstOf(o: Oop) *Cst {
    return @ptrCast(@alignCast(o.?));
}

fn strBytes(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}

// Mid-conversion protection (F3): a fresh Cst's children sit in a bare
// c_allocator array until the node's payload is assigned, so a collection
// firing during a later sibling's conversion would sweep the earlier ones (the
// array is invisible to both the stack scan and the payload scan). Every node
// the CURRENT conversion mints is held here — a registered GC root — until the
// top-level conversion returns and the root node owns the tree through its
// trace hook. Cleared by `mParseFrom` (parses don't nest).
var g_cst_scratch: std.ArrayListUnmanaged(Oop) = .{};

fn cstScratchRoots(mark: idapi.MarkFn) callconv(.c) void {
    for (g_cst_scratch.items) |o| mark(o);
}

/// GC trace hook: a Cst's children are Oops in non-id memory.
fn cstTrace(o: Oop, mark: idapi.MarkFn) callconv(.c) void {
    for (cstOf(o).children) |ch| mark(ch);
}

/// GC finalizer: free the children array a dying Cst owns (it was an unbounded
/// per-parse leak — the collector reclaimed the nodes but not their arrays).
fn cstFinalize(o: Oop) callconv(.c) void {
    const c = cstOf(o);
    if (c.children.len > 0) ca.free(c.children);
}

fn toCst(node: *Node) Oop {
    const o = g_api.alloc(cstnode_vt, @sizeOf(Cst));
    g_cst_scratch.append(ca, o) catch @panic("peg: oom");
    g_api.setFinalizer(o, &cstFinalize);
    const kids = ca.alloc(Oop, node.children.len) catch @panic("peg: oom");
    for (node.children, 0..) |ch, i| kids[i] = toCst(ch);
    cstOf(o).* = .{ .rule = node.rule, .start = node.start, .end = node.end, .children = kids };
    return o;
}

// Peg facade: `[Peg literal: aString]`, `[Peg seq]`, `[Peg ruleRef: #sym]`, …
fn mLiteral(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkLiteral(g_api, strBytes(a[0]));
}
fn mAnyMsg(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return mkAny(g_api);
}
fn mEofMsg(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return mkEof(g_api);
}
fn mCharset(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return newPattern(.{ .kind = .charset });
}
fn mRuleRefMsg(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkRuleRef(g_api, a[0]);
}
fn mSeq(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return newPattern(.{ .kind = .seq });
}
fn mChoice(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return newPattern(.{ .kind = .choice });
}
fn mGrammarMsg(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return mkGrammar(g_api);
}

// Pattern combinators (on pattern_vt): `[pat star]`, `[seq add: child]`, …
fn mStar(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkUnary(.star, self);
}
fn mPlus(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkUnary(.plus, self);
}
fn mOpt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkUnary(.opt, self);
}
fn mNegate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkUnary(.p_not, self);
}
fn mLookahead(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkUnary(.p_and, self);
}
fn mAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    addChild(self, a[0]);
    return self;
}
fn mFromTo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    addRange(self, @intCast(g_api.smallIntValue(a[0])), @intCast(g_api.smallIntValue(a[1])));
    return self;
}

// Grammar (on grammar_vt): `[g rule: #name is: pat]`, `[g parse: src from: #root]`.
fn mRuleIs(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    defineRule(self, a[0], a[1]);
    return self;
}
fn mParseFrom(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const root = parse(g_api, arena.allocator(), self, strBytes(a[0]), a[1]) orelse return g_api.nilObj;
    const cst = toCst(root);
    // The root now owns the tree (via its trace hook); drop the scratch roots.
    g_cst_scratch.clearRetainingCapacity();
    return cst;
}

// CstNode accessors (on cstnode_vt).
fn mCstRule(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return cstOf(self).rule;
}
fn mCstStart(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(cstOf(self).start));
}
fn mCstEnd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(cstOf(self).end));
}
fn mCstChildCount(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(cstOf(self).children.len));
}
fn mCstChildAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const i = g_api.smallIntValue(a[0]);
    const c = cstOf(self);
    if (i < 0 or @as(usize, @intCast(i)) >= c.children.len) return g_api.nilObj;
    return c.children[@intCast(i)];
}

// -- Bringup / module entrypoint --------------------------------------------

/// Wire peg onto libid as a message-driven module: build (or re-import) the
/// Pattern / Grammar / Peg-facade / CstNode vtables, install their methods, and
/// export the `Peg` facade so siblings (`lang`) can `api.import("Peg")` and drive
/// peg entirely through `api.send`.
pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const reloading = api.isExported("Peg") != 0;
    if (reloading) {
        pattern_vt = api.import("PegPatternVt");
        grammar_vt = api.import("PegGrammarVt");
        peg_vt = api.import("PegFacadeVt");
        peg_obj = api.import("Peg");
        cstnode_vt = api.import("PegCstVt");
    } else {
        pattern_vt = api.proto(api.object_vt);
        grammar_vt = api.proto(api.object_vt);
        peg_vt = api.proto(api.object_vt);
        cstnode_vt = api.proto(api.object_vt);
        peg_obj = api.alloc(peg_vt, 0);
    }

    // GC wiring (F3): patterns/grammars/CST nodes all hold Oops in non-id
    // memory (trace hooks); a conversion in progress roots its nodes itself.
    api.setTraceHook(pattern_vt, &patTrace);
    api.setTraceHook(grammar_vt, &gramTrace);
    api.setTraceHook(cstnode_vt, &cstTrace);
    if (!reloading) api.gcRegisterRootProvider(&cstScratchRoots);

    // Peg facade constructors.
    api.method(peg_vt, api.intern("literal:"), &mLiteral);
    api.method(peg_vt, api.intern("any"), &mAnyMsg);
    api.method(peg_vt, api.intern("eof"), &mEofMsg);
    api.method(peg_vt, api.intern("charset"), &mCharset);
    api.method(peg_vt, api.intern("ruleRef:"), &mRuleRefMsg);
    api.method(peg_vt, api.intern("seq"), &mSeq);
    api.method(peg_vt, api.intern("choice"), &mChoice);
    api.method(peg_vt, api.intern("grammar"), &mGrammarMsg);

    // Pattern combinators.
    api.method(pattern_vt, api.intern("star"), &mStar);
    api.method(pattern_vt, api.intern("plus"), &mPlus);
    api.method(pattern_vt, api.intern("opt"), &mOpt);
    api.method(pattern_vt, api.intern("negate"), &mNegate);
    api.method(pattern_vt, api.intern("lookahead"), &mLookahead);
    api.method(pattern_vt, api.intern("add:"), &mAdd);
    api.method(pattern_vt, api.intern("from:to:"), &mFromTo);

    // Grammar.
    api.method(grammar_vt, api.intern("rule:is:"), &mRuleIs);
    api.method(grammar_vt, api.intern("parse:from:"), &mParseFrom);

    // CstNode accessors.
    api.method(cstnode_vt, api.intern("rule"), &mCstRule);
    api.method(cstnode_vt, api.intern("start"), &mCstStart);
    api.method(cstnode_vt, api.intern("end"), &mCstEnd);
    api.method(cstnode_vt, api.intern("childCount"), &mCstChildCount);
    api.method(cstnode_vt, api.intern("childAt:"), &mCstChildAt);

    if (!reloading) {
        _ = api.exportObj("PegPatternVt", pattern_vt);
        _ = api.exportObj("PegGrammarVt", grammar_vt);
        _ = api.exportObj("PegFacadeVt", peg_vt);
        _ = api.exportObj("PegCstVt", cstnode_vt);
        _ = api.exportObj("Peg", peg_obj);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
// Export the dylib entrypoint only in non-test builds — a test that statically
// links several modules would otherwise hit duplicate `module_init` symbols.
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
