//! PEG engine — Phase 10.0 step 3.
//!
//! Pattern-as-facts PEG engine (Phase 10 decision P1+P3+P6+P7). Each
//! combinator is a fact-shaped entity carrying `:pattern-kind <keyword>`
//! + combinator-specific child / literal facts. A grammar is an entity
//! holding a singly-linked list of rule entities; each rule has a name
//! and a root pattern. The matcher walks fact-shaped patterns recursively
//! with a per-parse `AutoHashMap` packrat memo and emits CST nodes via
//! the kernel-reserved `:ast-*` attrs.
//!
//! **P10 deviation note.** Phase 10 decision P10 says PEG belongs in
//! `src/modules/peg/` dylib, not `src/kernel/`. Zig 0.15 module-isolation
//! rules block a dylib root file from importing kernel sources directly.
//! Matching Phase 7's precedent (kernel-resident engine + thin dylib
//! façade in `src/modules/query/`), the engine lives here and the dylib
//! façade in `src/modules/peg/peg.zig` will call back via IdApi once the
//! send-dispatched surface lands (step 4). The malleable-layer doctrine
//! is preserved at the user-facing surface — PEG_VT / GRAMMAR_VT methods
//! are installed by the dylib and can be retracted + replaced.

const std = @import("std");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const ref_mod = @import("ref.zig");
const strings = @import("strings.zig");
const kernel_eids = @import("kernel_eids.zig");

pub const Ref = ref_mod.Ref;
const FactStore = fact_store_mod.FactStore;
const ObjectTable = object_table_mod.ObjectTable;
const Interner = interner_mod.Interner;

// -- Engine ------------------------------------------------------------------

/// Per-parse engine state. Holds the source, the packrat memo, and the
/// furthest-failure tracker. Constructed for each `parse` call; freed at
/// parse end (memo is a hot read/write cache, NOT fact-shaped, per P2).
pub const Engine = struct {
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    interner: *Interner,
    module_eid: u64,

    // Cached combinator-kind keyword EIDs (interned at engine init).
    kind_literal: u64 = 0,
    kind_seq: u64 = 0,
    kind_choice: u64 = 0,
    kind_star: u64 = 0,
    kind_plus: u64 = 0,
    kind_opt: u64 = 0,
    kind_and: u64 = 0,
    kind_not: u64 = 0,
    kind_rule_ref: u64 = 0,
    kind_charset: u64 = 0,
    kind_any: u64 = 0,
    kind_eof: u64 = 0,
    kind_peg_parse_failure: u64 = 0,

    pub fn init(
        allocator: std.mem.Allocator,
        store: *FactStore,
        table: *ObjectTable,
        interner: *Interner,
        module_eid: u64,
    ) !Engine {
        var e = Engine{
            .allocator = allocator,
            .store = store,
            .table = table,
            .interner = interner,
            .module_eid = module_eid,
        };
        e.kind_literal = try interner.intern(store, table, "literal");
        e.kind_seq = try interner.intern(store, table, "seq");
        e.kind_choice = try interner.intern(store, table, "choice");
        e.kind_star = try interner.intern(store, table, "star");
        e.kind_plus = try interner.intern(store, table, "plus");
        e.kind_opt = try interner.intern(store, table, "opt");
        e.kind_and = try interner.intern(store, table, "and");
        e.kind_not = try interner.intern(store, table, "not");
        e.kind_rule_ref = try interner.intern(store, table, "rule-ref");
        e.kind_charset = try interner.intern(store, table, "charset");
        e.kind_any = try interner.intern(store, table, "any");
        e.kind_eof = try interner.intern(store, table, "eof");
        e.kind_peg_parse_failure = try interner.intern(store, table, "peg-parse-failure");
        return e;
    }
};

// -- Pattern construction ----------------------------------------------------
// Every constructor allocates a fresh pattern EID, asserts `:pattern-kind`
// + combinator-specific facts, and returns the EID. Callers compose these
// EIDs to build grammars before invoking `parse`.

fn newEid(e: *Engine) u64 {
    return e.table.alloc();
}

fn assertKind(e: *Engine, pat_eid: u64, kind_eid: u64) !void {
    _ = try e.store.assert(pat_eid, kernel_eids.ATTR_PATTERN_KIND, ref_mod.mkEid(kind_eid), e.module_eid);
}

/// Link a list of child EIDs as `:pattern-first-child` + `:pattern-next-sibling`.
fn linkPatternChildren(e: *Engine, parent: u64, children: []const u64) !void {
    if (children.len == 0) return;
    _ = try e.store.assert(parent, kernel_eids.ATTR_PATTERN_FIRST_CHILD, ref_mod.mkEid(children[0]), e.module_eid);
    var i: usize = 0;
    while (i + 1 < children.len) : (i += 1) {
        _ = try e.store.assert(children[i], kernel_eids.ATTR_PATTERN_NEXT_SIBLING, ref_mod.mkEid(children[i + 1]), e.module_eid);
    }
}

pub fn mkLiteral(e: *Engine, value: []const u8) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_literal);
    const s = try strings.mkString(e.allocator, value);
    _ = try e.store.assert(pat, kernel_eids.ATTR_LITERAL_VALUE, s, e.module_eid);
    return pat;
}

pub fn mkSeq(e: *Engine, children: []const u64) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_seq);
    try linkPatternChildren(e, pat, children);
    return pat;
}

pub fn mkChoice(e: *Engine, children: []const u64) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_choice);
    try linkPatternChildren(e, pat, children);
    return pat;
}

fn mkUnary(e: *Engine, kind_eid: u64, child: u64) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, kind_eid);
    _ = try e.store.assert(pat, kernel_eids.ATTR_PATTERN_FIRST_CHILD, ref_mod.mkEid(child), e.module_eid);
    return pat;
}

pub fn mkStar(e: *Engine, child: u64) !u64 {
    return mkUnary(e, e.kind_star, child);
}
pub fn mkPlus(e: *Engine, child: u64) !u64 {
    return mkUnary(e, e.kind_plus, child);
}
pub fn mkOpt(e: *Engine, child: u64) !u64 {
    return mkUnary(e, e.kind_opt, child);
}
pub fn mkAnd(e: *Engine, child: u64) !u64 {
    return mkUnary(e, e.kind_and, child);
}
pub fn mkNot(e: *Engine, child: u64) !u64 {
    return mkUnary(e, e.kind_not, child);
}

pub fn mkRuleRef(e: *Engine, rule_name: []const u8) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_rule_ref);
    const name_eid = try e.interner.intern(e.store, e.table, rule_name);
    _ = try e.store.assert(pat, kernel_eids.ATTR_RULE_REF_NAME, ref_mod.mkEid(name_eid), e.module_eid);
    return pat;
}

pub const Range = struct { lo: u8, hi: u8 };

pub fn mkCharset(e: *Engine, ranges: []const Range) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_charset);
    if (ranges.len == 0) return pat;
    var prev: ?u64 = null;
    var first: u64 = 0;
    for (ranges) |r| {
        const range_eid = newEid(e);
        _ = try e.store.assert(range_eid, kernel_eids.ATTR_CHARSET_RANGE_LO, ref_mod.mkSmallInt(@intCast(r.lo)).?, e.module_eid);
        _ = try e.store.assert(range_eid, kernel_eids.ATTR_CHARSET_RANGE_HI, ref_mod.mkSmallInt(@intCast(r.hi)).?, e.module_eid);
        if (prev) |p| {
            _ = try e.store.assert(p, kernel_eids.ATTR_CHARSET_NEXT_RANGE, ref_mod.mkEid(range_eid), e.module_eid);
        } else {
            first = range_eid;
        }
        prev = range_eid;
    }
    _ = try e.store.assert(pat, kernel_eids.ATTR_CHARSET_FIRST_RANGE, ref_mod.mkEid(first), e.module_eid);
    return pat;
}

pub fn mkAny(e: *Engine) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_any);
    return pat;
}

pub fn mkEof(e: *Engine) !u64 {
    const pat = newEid(e);
    try assertKind(e, pat, e.kind_eof);
    return pat;
}

// -- Grammar + rule entities -------------------------------------------------

pub fn mkGrammar(e: *Engine) !u64 {
    const g = newEid(e);
    _ = try e.store.assert(g, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.GRAMMAR_VT), e.module_eid);
    return g;
}

/// Appends a rule entity to the grammar's rule list. Returns the rule EID.
pub fn defineRule(e: *Engine, grammar_eid: u64, name: []const u8, pattern_eid: u64) !u64 {
    const rule = newEid(e);
    const name_eid = try e.interner.intern(e.store, e.table, name);
    _ = try e.store.assert(rule, kernel_eids.ATTR_RULE_NAME, ref_mod.mkEid(name_eid), e.module_eid);
    _ = try e.store.assert(rule, kernel_eids.ATTR_RULE_PATTERN, ref_mod.mkEid(pattern_eid), e.module_eid);
    if (e.store.currentValue(grammar_eid, kernel_eids.ATTR_GRAMMAR_FIRST_RULE)) |first_ref| {
        // Walk to end of list, append.
        var cur = ref_mod.untagEid(first_ref);
        while (e.store.currentValue(cur, kernel_eids.ATTR_RULE_NEXT)) |next_ref| {
            cur = ref_mod.untagEid(next_ref);
        }
        _ = try e.store.assert(cur, kernel_eids.ATTR_RULE_NEXT, ref_mod.mkEid(rule), e.module_eid);
    } else {
        _ = try e.store.assert(grammar_eid, kernel_eids.ATTR_GRAMMAR_FIRST_RULE, ref_mod.mkEid(rule), e.module_eid);
    }
    return rule;
}

fn findRule(e: *Engine, grammar_eid: u64, name_eid: u64) ?u64 {
    var cur_ref = e.store.currentValue(grammar_eid, kernel_eids.ATTR_GRAMMAR_FIRST_RULE) orelse return null;
    while (true) {
        const rule = ref_mod.untagEid(cur_ref);
        if (e.store.currentValue(rule, kernel_eids.ATTR_RULE_NAME)) |n_ref| {
            if (ref_mod.isEid(n_ref) and ref_mod.untagEid(n_ref) == name_eid) return rule;
        }
        cur_ref = e.store.currentValue(rule, kernel_eids.ATTR_RULE_NEXT) orelse return null;
    }
}

// -- Matcher -----------------------------------------------------------------

const MemoKey = struct {
    pat: u64,
    pos: usize,
};

const MemoEntry = struct {
    end_pos: usize,
    children: []u64, // children EIDs (allocated in scratch arena)
    success: bool,
};

const MemoMap = std.AutoHashMap(MemoKey, MemoEntry);

pub const ParseOutcome = union(enum) {
    success: u64, // root AST EID
    failure: u64, // ERROR_VT entity EID
};

const RunState = struct {
    engine: *Engine,
    source: []const u8,
    memo: MemoMap,
    scratch: std.heap.ArenaAllocator,
    furthest_pos: usize = 0,
    expected: std.array_list.Managed(u64), // interned name EIDs

    fn deinit(self: *RunState) void {
        self.memo.deinit();
        self.scratch.deinit();
        self.expected.deinit();
    }

    fn recordExpected(self: *RunState, pos: usize, name_eid: u64) void {
        if (pos < self.furthest_pos) return;
        if (pos > self.furthest_pos) {
            self.furthest_pos = pos;
            self.expected.clearRetainingCapacity();
        }
        for (self.expected.items) |existing| if (existing == name_eid) return;
        self.expected.append(name_eid) catch {};
    }
};

const Match = struct {
    end_pos: usize,
    children: []u64,
};

fn matchPattern(rs: *RunState, grammar_eid: u64, pat_eid: u64, pos: usize) anyerror!?Match {
    const e = rs.engine;
    const kind_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_KIND) orelse return null;
    if (!ref_mod.isEid(kind_ref)) return null;
    const kind = ref_mod.untagEid(kind_ref);

    if (kind == e.kind_literal) {
        const v_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_LITERAL_VALUE) orelse return null;
        if (!strings.isString(v_ref)) return null;
        const want = strings.bytes(v_ref);
        if (pos + want.len > rs.source.len or !std.mem.eql(u8, rs.source[pos .. pos + want.len], want)) {
            const name_eid = try rs.engine.interner.intern(rs.engine.store, rs.engine.table, want);
            rs.recordExpected(pos, name_eid);
            return null;
        }
        return Match{ .end_pos = pos + want.len, .children = &.{} };
    }

    if (kind == e.kind_any) {
        if (pos >= rs.source.len) return null;
        return Match{ .end_pos = pos + 1, .children = &.{} };
    }

    if (kind == e.kind_eof) {
        if (pos != rs.source.len) return null;
        return Match{ .end_pos = pos, .children = &.{} };
    }

    if (kind == e.kind_charset) {
        if (pos >= rs.source.len) return null;
        const c = rs.source[pos];
        var cur_opt = e.store.currentValue(pat_eid, kernel_eids.ATTR_CHARSET_FIRST_RANGE);
        while (cur_opt) |cur_ref| {
            const r_eid = ref_mod.untagEid(cur_ref);
            const lo_ref = e.store.currentValue(r_eid, kernel_eids.ATTR_CHARSET_RANGE_LO) orelse return null;
            const hi_ref = e.store.currentValue(r_eid, kernel_eids.ATTR_CHARSET_RANGE_HI) orelse return null;
            const lo: u8 = @intCast(ref_mod.untagSmallInt(lo_ref));
            const hi: u8 = @intCast(ref_mod.untagSmallInt(hi_ref));
            if (c >= lo and c <= hi) return Match{ .end_pos = pos + 1, .children = &.{} };
            cur_opt = e.store.currentValue(r_eid, kernel_eids.ATTR_CHARSET_NEXT_RANGE);
        }
        return null;
    }

    if (kind == e.kind_seq) {
        var children = std.array_list.Managed(u64).init(rs.scratch.allocator());
        var p = pos;
        var cur_opt = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD);
        while (cur_opt) |cur_ref| {
            const child_pat = ref_mod.untagEid(cur_ref);
            const m = (try matchPattern(rs, grammar_eid, child_pat, p)) orelse {
                children.deinit();
                return null;
            };
            try children.appendSlice(m.children);
            p = m.end_pos;
            cur_opt = e.store.currentValue(child_pat, kernel_eids.ATTR_PATTERN_NEXT_SIBLING);
        }
        return Match{ .end_pos = p, .children = try children.toOwnedSlice() };
    }

    if (kind == e.kind_choice) {
        var cur_opt = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD);
        while (cur_opt) |cur_ref| {
            const child_pat = ref_mod.untagEid(cur_ref);
            if (try matchPattern(rs, grammar_eid, child_pat, pos)) |m| return m;
            cur_opt = e.store.currentValue(child_pat, kernel_eids.ATTR_PATTERN_NEXT_SIBLING);
        }
        return null;
    }

    if (kind == e.kind_star or kind == e.kind_plus) {
        const child_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD) orelse return null;
        const child_pat = ref_mod.untagEid(child_ref);
        var children = std.array_list.Managed(u64).init(rs.scratch.allocator());
        var p = pos;
        var count: usize = 0;
        while (try matchPattern(rs, grammar_eid, child_pat, p)) |m| {
            try children.appendSlice(m.children);
            // Zero-width-success guard: stop infinite loops.
            if (m.end_pos == p) break;
            p = m.end_pos;
            count += 1;
        }
        if (kind == e.kind_plus and count == 0) {
            children.deinit();
            return null;
        }
        return Match{ .end_pos = p, .children = try children.toOwnedSlice() };
    }

    if (kind == e.kind_opt) {
        const child_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD) orelse return null;
        const child_pat = ref_mod.untagEid(child_ref);
        if (try matchPattern(rs, grammar_eid, child_pat, pos)) |m| return m;
        return Match{ .end_pos = pos, .children = &.{} };
    }

    if (kind == e.kind_and) {
        const child_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD) orelse return null;
        const child_pat = ref_mod.untagEid(child_ref);
        if (try matchPattern(rs, grammar_eid, child_pat, pos)) |_| {
            return Match{ .end_pos = pos, .children = &.{} };
        }
        return null;
    }

    if (kind == e.kind_not) {
        const child_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_PATTERN_FIRST_CHILD) orelse return null;
        const child_pat = ref_mod.untagEid(child_ref);
        if (try matchPattern(rs, grammar_eid, child_pat, pos)) |_| return null;
        return Match{ .end_pos = pos, .children = &.{} };
    }

    if (kind == e.kind_rule_ref) {
        const name_ref = e.store.currentValue(pat_eid, kernel_eids.ATTR_RULE_REF_NAME) orelse return null;
        if (!ref_mod.isEid(name_ref)) return null;
        const name_eid = ref_mod.untagEid(name_ref);
        const rule_eid = findRule(e, grammar_eid, name_eid) orelse return null;
        const rule_pat_ref = e.store.currentValue(rule_eid, kernel_eids.ATTR_RULE_PATTERN) orelse return null;
        const rule_pat = ref_mod.untagEid(rule_pat_ref);

        // Packrat: memoize at the rule boundary.
        const key = MemoKey{ .pat = rule_eid, .pos = pos };
        if (rs.memo.get(key)) |entry| {
            if (!entry.success) {
                rs.recordExpected(pos, name_eid);
                return null;
            }
            return Match{ .end_pos = entry.end_pos, .children = entry.children };
        }

        if (try matchPattern(rs, grammar_eid, rule_pat, pos)) |inner| {
            // Build CST node for this rule with collected children.
            const ast_eid = e.table.alloc();
            _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_RULE_NAME, ref_mod.mkEid(name_eid), e.module_eid);
            _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_BYTE_START, ref_mod.mkSmallInt(@intCast(pos)).?, e.module_eid);
            _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_BYTE_END, ref_mod.mkSmallInt(@intCast(inner.end_pos)).?, e.module_eid);
            if (inner.children.len > 0) {
                _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_FIRST_CHILD, ref_mod.mkEid(inner.children[0]), e.module_eid);
                var i: usize = 0;
                while (i + 1 < inner.children.len) : (i += 1) {
                    _ = try e.store.assert(inner.children[i], kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(inner.children[i + 1]), e.module_eid);
                }
            }
            const wrapped = try rs.scratch.allocator().alloc(u64, 1);
            wrapped[0] = ast_eid;
            try rs.memo.put(key, .{ .end_pos = inner.end_pos, .children = wrapped, .success = true });
            return Match{ .end_pos = inner.end_pos, .children = wrapped };
        }
        try rs.memo.put(key, .{ .end_pos = pos, .children = &.{}, .success = false });
        rs.recordExpected(pos, name_eid);
        return null;
    }

    return null;
}

/// Parse `source` against `grammar_eid` starting at `rule_name`. Returns
/// either the root CST EID on success or an ERROR_VT diagnostic EID on
/// failure. Per PR3 spec, calls happen inside one tx implicitly because
/// the underlying store assert path is tx-aware; the engine itself does
/// not bracket a tx (test harnesses may wrap if desired).
pub fn parse(e: *Engine, grammar_eid: u64, source: []const u8, rule_name: []const u8) !ParseOutcome {
    var rs = RunState{
        .engine = e,
        .source = source,
        .memo = MemoMap.init(e.allocator),
        .scratch = std.heap.ArenaAllocator.init(e.allocator),
        .expected = std.array_list.Managed(u64).init(e.allocator),
    };
    defer rs.deinit();

    const name_eid = try e.interner.intern(e.store, e.table, rule_name);
    const rule_eid = findRule(e, grammar_eid, name_eid) orelse {
        return ParseOutcome{ .failure = try mkErrorEntity(e, rule_name, &.{}, 0) };
    };
    const rule_pat_ref = e.store.currentValue(rule_eid, kernel_eids.ATTR_RULE_PATTERN) orelse {
        return ParseOutcome{ .failure = try mkErrorEntity(e, rule_name, &.{}, 0) };
    };
    const rule_pat = ref_mod.untagEid(rule_pat_ref);

    if (try matchPattern(&rs, grammar_eid, rule_pat, 0)) |m| {
        // Wrap top-level result in a rule CST node (matches rule-ref behavior).
        const ast_eid = e.table.alloc();
        _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_RULE_NAME, ref_mod.mkEid(name_eid), e.module_eid);
        _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_BYTE_START, ref_mod.mkSmallInt(0).?, e.module_eid);
        _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_BYTE_END, ref_mod.mkSmallInt(@intCast(m.end_pos)).?, e.module_eid);
        if (m.children.len > 0) {
            _ = try e.store.assert(ast_eid, kernel_eids.ATTR_AST_FIRST_CHILD, ref_mod.mkEid(m.children[0]), e.module_eid);
            var i: usize = 0;
            while (i + 1 < m.children.len) : (i += 1) {
                _ = try e.store.assert(m.children[i], kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(m.children[i + 1]), e.module_eid);
            }
        }
        return ParseOutcome{ .success = ast_eid };
    }
    return ParseOutcome{ .failure = try mkErrorEntity(e, rule_name, rs.expected.items, rs.furthest_pos) };
}

fn mkErrorEntity(e: *Engine, entry_rule: []const u8, expected: []const u64, position: usize) !u64 {
    _ = entry_rule;
    const err = e.table.alloc();
    _ = try e.store.assert(err, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.ERROR_VT), e.module_eid);
    _ = try e.store.assert(err, kernel_eids.ATTR_ERROR_KIND, ref_mod.mkEid(e.kind_peg_parse_failure), e.module_eid);
    _ = try e.store.assert(err, kernel_eids.ATTR_PEG_POSITION, ref_mod.mkSmallInt(@intCast(position)).?, e.module_eid);
    for (expected) |name_eid| {
        _ = try e.store.assert(err, kernel_eids.ATTR_PEG_EXPECTED, ref_mod.mkEid(name_eid), e.module_eid);
    }
    return err;
}

// -- Send-dispatched API surface (Phase 10.0 step 4) -----------------------
//
// The PEG dylib (src/modules/peg/peg.zig) calls `installSendApi` at module-
// init time via a single appended IdApi field (`peg_install_send_api`).
// Caching the engine state here lets the installed method closures reach
// allocator + interner + cached combinator-kind EIDs without threading
// them through each send.

const closure_mod = @import("closure.zig");
const send_mod = @import("send.zig");
const Closure = closure_mod.Closure;
const Send = send_mod.Send;

const InstallState = struct {
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    interner: *Interner,
    engine: Engine,
};

var g_install: ?InstallState = null;

/// Installs `parse:from:` on GRAMMAR_VT and caches the engine state. The
/// dylib calls this at module-init via the appended IdApi entry; tests
/// call it directly. Multiple calls overwrite the cached engine state —
/// the engine has no resources of its own beyond the cached EIDs.
pub fn installSendApi(
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    interner: *Interner,
    module_eid: u64,
) !void {
    const engine = try Engine.init(allocator, store, table, interner, module_eid);
    g_install = .{
        .allocator = allocator,
        .store = store,
        .table = table,
        .interner = interner,
        .engine = engine,
    };

    // `parse:from:` method on GRAMMAR_VT — see send.zig for the dispatch path.
    const sel = try interner.intern(store, table, "parse:from:");
    const closure_eid = try allocNativeClosure(allocator, table, @ptrCast(&parseFromImpl));
    _ = try store.assert(kernel_eids.GRAMMAR_VT, sel, ref_mod.mkEid(closure_eid), module_eid);
}

fn allocNativeClosure(
    allocator: std.mem.Allocator,
    table: *ObjectTable,
    method_fn: *const anyopaque,
) !u64 {
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_fn;
    c.data = ref_mod.NIL;
    const eid = table.alloc();
    try table.put(eid, ref_mod.mkPointer(@ptrCast(c)));
    return eid;
}

/// `[grammar parse: source from: ruleName]` — runs the engine against
/// source bytes starting at the named rule. Returns the CST root EID
/// on success or an ERROR_VT EID on failure (caller introspects
/// `:vtable` to discriminate).
fn parseFromImpl(s: *Send, source_ref: Ref, rule_name_ref: Ref) callconv(.c) Ref {
    var st = &(g_install orelse return ref_mod.NIL);
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    if (!strings.isString(source_ref) or !strings.isString(rule_name_ref)) return ref_mod.NIL;
    const grammar = ref_mod.untagEid(s.receiver);
    const src = strings.bytes(source_ref);
    const rule = strings.bytes(rule_name_ref);
    const outcome = parse(&st.engine, grammar, src, rule) catch return ref_mod.NIL;
    return switch (outcome) {
        .success => |ast| ref_mod.mkEid(ast),
        .failure => |err| ref_mod.mkEid(err),
    };
}
