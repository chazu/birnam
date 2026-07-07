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
//!
//! From birnam, `Peg` is the facade. Primitive constructors: literal: any eof
//! charset ruleRef: seq choice grammar; `[Peg seq]`/`[Peg choice]` grow via
//! `add:`, `[Peg charset]` via `from:to:`. Ergonomic forms: newGrammar anyByte
//! seqOf: choiceOf: star: plus: opt: not: and: anyOf: range:to:. A grammar
//! answers `rule:is:` (rule names may be Symbols OR Strings — Strings are
//! interned, so both name the same rule) and `parse:from:` → a CST root (nil on
//! failure). CST nodes answer rule ruleName start end childCount childAt:
//! children — walk them and slice the source with String `copyFrom:to:`.

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
// A grammar carries its OWN last-parse diagnostics (docs/42 §C2): the furthest
// input offset reached, and — if the failure was a reference to a rule the grammar
// doesn't define (§C3) — that rule's name. Per-instance, NOT a module global that
// interleaved parses on other grammars would clobber. Rule names are permanent
// (interned symbols), so `undefined_rule` needs no GC tracing.
const Gram = struct {
    rules: std.ArrayList(Rule) = .{},
    furthest: usize = 0,
    undefined_rule: Oop = null,
    left_recursive_rule: Oop = null, // §G4: a left-recursive rule reached last parse
};

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
// `in_progress` marks a rule that is CURRENTLY being evaluated at this position:
// planted before descending so a left-recursive re-entry at the same (rule, pos)
// is caught (§G4) instead of recursing to a stack overflow. Overwritten by the
// real ok/fail entry once the match returns.
const MemoEntry = struct { ok: bool, end: usize, children: []*Node, in_progress: bool = false };

const RunState = struct {
    g: Oop,
    source: []const u8,
    na: std.mem.Allocator, // node arena (caller-owned; the transient CST lives here)
    memo: *std.AutoHashMap(MemoKey, MemoEntry),
    furthest: usize = 0, // furthest failure position (for diagnostics later)
    undefined_rule: Oop = null, // a rule_ref to a rule the grammar doesn't define (§C3)
    left_recursive_rule: Oop = null, // a rule that re-enters itself at the same pos (§G4)

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
            // A reference to a rule the grammar doesn't define is a GRAMMAR bug (a
            // typo), not a legitimate non-match — record the name so a failed parse
            // can report it distinctly (§C3) instead of a silent "syntax error".
            const rule_pat = ruleNamed(rs.g, p.rule_name) orelse {
                rs.undefined_rule = p.rule_name;
                return null;
            };
            const key = MemoKey{ .pat = rule_pat, .pos = pos }; // packrat at the rule boundary
            if (rs.memo.get(key)) |entry| {
                if (entry.in_progress) {
                    // We re-entered this rule at the SAME position without consuming
                    // input: left recursion (direct or indirect). That's a structural
                    // grammar defect, never a legitimate non-match — flag it (§G4) and
                    // fail this path so it becomes a kinded error, not a stack overflow.
                    rs.left_recursive_rule = p.rule_name;
                    return null;
                }
                if (!entry.ok) return null;
                return Match{ .end = entry.end, .children = entry.children };
            }
            // Plant the in-progress marker BEFORE descending so a left-recursive
            // re-entry at this (rule, pos) is caught above rather than recursing.
            rs.memo.put(key, .{ .ok = false, .end = pos, .children = no_children, .in_progress = true }) catch @panic("peg: oom");
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
    // Publish this parse's diagnostics onto the grammar instance (not a module
    // global): the furthest offset reached, and any undefined-rule name (§C2/§C3).
    const gd = gramOf(g);
    gd.undefined_rule = null;
    gd.left_recursive_rule = null;
    const root_pat = ruleNamed(g, root_name) orelse {
        gd.undefined_rule = root_name; // the ROOT rule itself is undefined
        gd.furthest = 0;
        return null;
    };
    if (matchPattern(&rs, root_pat, 0)) |m| {
        // Full consumption is the caller's concern via the root rule (birnam's
        // `program` is `eof`-anchored, so a short match already fails there); this
        // primitive keeps prefix-matching semantics for other roots/tests.
        gd.furthest = rs.furthest;
        gd.undefined_rule = null; // a masked undefined-ref in a tried branch isn't an error
        gd.left_recursive_rule = null; // ditto: LR hit in a backtracked branch that still parsed
        const node = na.create(Node) catch @panic("peg: oom");
        node.* = .{ .rule = root_name, .start = 0, .end = m.end, .children = m.children };
        return node;
    }
    gd.furthest = rs.furthest;
    gd.left_recursive_rule = rs.left_recursive_rule;
    gd.undefined_rule = rs.undefined_rule;
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

fn isStr(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == g_api.string_vt;
}

/// Coerce a rule-name argument to the interned symbol the engine matches by
/// identity: a String is interned (so `"digit"` and `#digit` name the same rule),
/// a Symbol passes through. Lets grammars use whichever reads better.
fn asRuleSym(o: Oop) Oop {
    if (!isStr(o)) return o;
    const s = strBytes(o);
    const z = ca.allocSentinel(u8, s.len, 0) catch @panic("peg: oom");
    defer ca.free(z);
    @memcpy(z[0..s.len], s);
    return g_api.intern(z.ptr);
}

/// A single byte from a `range:to:`/`anyOf:` argument — a 1-char String's byte,
/// or a SmallInt's low byte.
fn byteArg(o: Oop) u8 {
    if (isStr(o)) {
        const s = strBytes(o);
        return if (s.len > 0) s[0] else 0;
    }
    return @intCast(g_api.smallIntValue(o) & 0xff);
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
    return mkRuleRef(g_api, asRuleSym(a[0]));
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
    defineRule(self, asRuleSym(a[0]), a[1]);
    return self;
}
/// A kinded parse error value (F7 model, imported at call time), or nil if the error
/// module isn't loaded (docs/42 §C2/§C3) — so a parse failure carries WHY, and the
/// diagnostic rides the returned value instead of a clobberable module global.
fn pegError(comptime kind: [:0]const u8, message: []const u8) Oop {
    const ef = g_api.import("Error") orelse return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:message:"), &[_]Oop{
        g_api.intern(kind.ptr), g_api.mkString(message.ptr, message.len),
    }, 2);
}

fn mParseFrom(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const src = strBytes(a[0]);
    const root = parse(g_api, arena.allocator(), self, src, asRuleSym(a[1])) orelse {
        const gd = gramOf(self);
        // §C3: a reference to an undefined rule is a distinct, nameable failure — a
        // typo, not a mere non-match that once silently produced the same nil.
        if (gd.undefined_rule) |ur| {
            var mb: [256]u8 = undefined;
            const msg = std.fmt.bufPrint(&mb, "undefined rule: {s}", .{std.mem.span(g_api.symbolName(ur))}) catch "undefined rule";
            return pegError("undefinedRule", msg);
        }
        // §G4: a left-recursive rule is a structural grammar defect (it would recurse
        // to a stack overflow) — a distinct, nameable failure, not a mere non-match.
        if (gd.left_recursive_rule) |lr| {
            var mb: [256]u8 = undefined;
            const msg = std.fmt.bufPrint(&mb, "left-recursive rule: {s}", .{std.mem.span(g_api.symbolName(lr))}) catch "left-recursive rule";
            return pegError("leftRecursion", msg);
        }
        // §C2: a plain syntax error, located at the furthest offset reached.
        var line: usize = 1;
        var col: usize = 1;
        var i: usize = 0;
        const off = @min(gd.furthest, src.len);
        while (i < off) : (i += 1) {
            if (src[i] == '\n') {
                line += 1;
                col = 1;
            } else col += 1;
        }
        var mb: [128]u8 = undefined;
        const msg = std.fmt.bufPrint(&mb, "syntax error at line {d}, col {d}", .{ line, col }) catch "syntax error";
        return pegError("syntaxError", msg);
    };
    const cst = toCst(root);
    // The root now owns the tree (via its trace hook); drop the scratch roots.
    g_cst_scratch.clearRetainingCapacity();
    return cst;
}

/// `[aGrammar furthestFail]` — the furthest source offset reached by the grammar's
/// most recent `parse:from:` before failing (a "the syntax error is around here"
/// pointer). Per-instance state now, not a module global. Only meaningful right
/// after a parse that returned nil / an error.
fn mFurthestFail(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(gramOf(self).furthest));
}

// CstNode accessors (on cstnode_vt).
fn mCstRule(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return cstOf(self).rule;
}
/// `[node ruleName]` — the matched rule's name as a String (the text form of the
/// `rule` symbol), so it compares with String literals in a CST walk.
fn mCstRuleName(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const name = std.mem.span(g_api.symbolName(cstOf(self).rule));
    return g_api.mkString(name.ptr, name.len);
}
/// `[node children]` — a Vector of the child CST nodes (rule matches nested in
/// this one), ready for `select:`/`collect:`.
fn mCstChildren(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const kids = cstOf(self).children;
    var vec = g_api.send(g_api.import("Vector"), g_api.intern("new"), &[_]Oop{}, 0);
    const s_add = g_api.intern("add:");
    for (kids) |k| vec = g_api.send(vec, s_add, &[_]Oop{k}, 1);
    return vec;
}

// -- Peg facade: ergonomic constructors over the primitive combinators -------
// The primitives (`seq`+`add:`, `charset`+`from:to:`) compose anything, but these
// read far better in a grammar: vector forms for seq/choice, prefix wrappers for
// star/plus/opt/not/and, and charsets from a string or a range.

/// `[Peg seqOf: #[ p1 p2 … ]]` — a sequence of the patterns in the Vector.
fn mSeqOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return newPattern(.{ .kind = .seq, .children = childrenFromVec(a[0]) });
}
/// `[Peg choiceOf: #[ p1 p2 … ]]` — an ordered choice among the Vector's patterns.
fn mChoiceOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return newPattern(.{ .kind = .choice, .children = childrenFromVec(a[0]) });
}
/// Copy a birnam Vector of patterns into an owned `[]Oop` (via `size`/`at:`).
fn childrenFromVec(v: Oop) []const Oop {
    const count: usize = @intCast(g_api.smallIntValue(g_api.send(v, g_api.intern("size"), &[_]Oop{}, 0)));
    const out = ca.alloc(Oop, count) catch @panic("peg: oom");
    const s_at = g_api.intern("at:");
    var i: usize = 0;
    while (i < count) : (i += 1) out[i] = g_api.send(v, s_at, &[_]Oop{g_api.mkSmallInt(@intCast(i))}, 1);
    return out;
}
fn mStarOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkUnary(.star, a[0]);
}
fn mPlusOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkUnary(.plus, a[0]);
}
fn mOptOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkUnary(.opt, a[0]);
}
fn mNotOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkUnary(.p_not, a[0]);
}
fn mAndOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkUnary(.p_and, a[0]);
}
/// `[Peg anyOf: "abc"]` — a charset matching any one of the bytes in the String.
fn mAnyOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cs = newPattern(.{ .kind = .charset });
    for (strBytes(a[0])) |b| addRange(cs, b, b);
    return cs;
}
/// `[Peg range: lo to: hi]` — a charset matching one byte in [lo, hi]. Bounds may
/// be 1-char Strings (`"0"`/`"9"`) or SmallInt byte values.
fn mRangeTo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cs = newPattern(.{ .kind = .charset });
    addRange(cs, byteArg(a[0]), byteArg(a[1]));
    return cs;
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
    // Ergonomic facade: vector/prefix forms + charset builders + aliases.
    api.method(peg_vt, api.intern("newGrammar"), &mGrammarMsg);
    api.method(peg_vt, api.intern("anyByte"), &mAnyMsg);
    api.method(peg_vt, api.intern("seqOf:"), &mSeqOf);
    api.method(peg_vt, api.intern("choiceOf:"), &mChoiceOf);
    api.method(peg_vt, api.intern("star:"), &mStarOf);
    api.method(peg_vt, api.intern("plus:"), &mPlusOf);
    api.method(peg_vt, api.intern("opt:"), &mOptOf);
    api.method(peg_vt, api.intern("not:"), &mNotOf);
    api.method(peg_vt, api.intern("and:"), &mAndOf);
    api.method(peg_vt, api.intern("anyOf:"), &mAnyOf);
    api.method(peg_vt, api.intern("range:to:"), &mRangeTo);

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
    api.method(grammar_vt, api.intern("furthestFail"), &mFurthestFail);

    // CstNode accessors.
    api.method(cstnode_vt, api.intern("rule"), &mCstRule);
    api.method(cstnode_vt, api.intern("ruleName"), &mCstRuleName);
    api.method(cstnode_vt, api.intern("start"), &mCstStart);
    api.method(cstnode_vt, api.intern("end"), &mCstEnd);
    api.method(cstnode_vt, api.intern("childCount"), &mCstChildCount);
    api.method(cstnode_vt, api.intern("childAt:"), &mCstChildAt);
    api.method(cstnode_vt, api.intern("children"), &mCstChildren);

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
