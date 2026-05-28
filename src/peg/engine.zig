const std = @import("std");
const nodes = @import("nodes.zig");
const IdApi = @import("api.zig").IdApi;
const Oop = nodes.Oop;

pub const MAX_CAPTURES = 64;

pub const CaptureRange = struct {
    begin: usize,
    end: usize,
};

pub const Parser = struct {
    input: []const u8,
    pos: usize,
    grammar: Oop,
    id: *const IdApi,
    captures: [MAX_CAPTURES]CaptureRange,
    capture_count: usize,

    pub fn init(input: []const u8, grammar: Oop, api: *const IdApi) Parser {
        return .{
            .input = input,
            .pos = 0,
            .grammar = grammar,
            .id = api,
            .captures = undefined,
            .capture_count = 0,
        };
    }

    pub fn parse(self: *Parser) bool {
        const g = nodes.asGrammar(self.grammar) orelse return false;
        if (g.start_rule == null) return false;
        return self.matchNode(g.start_rule);
    }

    pub fn matchNode(self: *Parser, node: Oop) bool {
        const vt = nodes.vtableOf(node);
        if (vt == null) return false;

        if (vt == nodes.sequence_vt) return self.matchSequence(node);
        if (vt == nodes.alternate_vt) return self.matchAlternate(node);
        if (vt == nodes.star_vt) return self.matchStar(node);
        if (vt == nodes.plus_vt) return self.matchPlus(node);
        if (vt == nodes.query_vt) return self.matchQuery(node);
        if (vt == nodes.peek_for_vt) return self.matchPeekFor(node);
        if (vt == nodes.peek_not_vt) return self.matchPeekNot(node);
        if (vt == nodes.name_vt) return self.matchName(node);
        if (vt == nodes.string_lit_vt) return self.matchStringLit(node);
        if (vt == nodes.char_class_vt) return self.matchCharClass(node);
        if (vt == nodes.dot_vt) return self.matchDot();
        if (vt == nodes.action_vt) return true; // actions always succeed
        if (vt == nodes.capture_vt) return self.matchCaptureNode(node);
        if (vt == nodes.rule_vt) return self.matchRule(node);

        return false;
    }

    fn matchSequence(self: *Parser, node: Oop) bool {
        const seq = nodes.asSequence(node) orelse return false;
        const saved = self.pos;
        const elems = getVectorElements(seq.elements) orelse return false;
        for (0..seq.count) |i| {
            if (!self.matchNode(elems[i])) {
                self.pos = saved;
                return false;
            }
        }
        return true;
    }

    fn matchAlternate(self: *Parser, node: Oop) bool {
        const alt = nodes.asAlternate(node) orelse return false;
        const elems = getVectorElements(alt.elements) orelse return false;
        for (0..alt.count) |i| {
            const saved = self.pos;
            if (self.matchNode(elems[i])) return true;
            self.pos = saved;
        }
        return false;
    }

    fn matchStar(self: *Parser, node: Oop) bool {
        const s = nodes.asStar(node) orelse return false;
        while (true) {
            const saved = self.pos;
            if (!self.matchNode(s.element)) {
                self.pos = saved;
                break;
            }
        }
        return true; // star always succeeds
    }

    fn matchPlus(self: *Parser, node: Oop) bool {
        const p = nodes.asPlus(node) orelse return false;
        if (!self.matchNode(p.element)) return false;
        while (true) {
            const saved = self.pos;
            if (!self.matchNode(p.element)) {
                self.pos = saved;
                break;
            }
        }
        return true;
    }

    fn matchQuery(self: *Parser, node: Oop) bool {
        const q = nodes.asQuery(node) orelse return false;
        const saved = self.pos;
        if (!self.matchNode(q.element)) {
            self.pos = saved;
        }
        return true; // query always succeeds
    }

    fn matchPeekFor(self: *Parser, node: Oop) bool {
        const p = nodes.asPeekFor(node) orelse return false;
        const saved = self.pos;
        const result = self.matchNode(p.element);
        self.pos = saved;
        return result;
    }

    fn matchPeekNot(self: *Parser, node: Oop) bool {
        const p = nodes.asPeekNot(node) orelse return false;
        const saved = self.pos;
        const result = self.matchNode(p.element);
        self.pos = saved;
        return !result;
    }

    fn matchName(self: *Parser, node: Oop) bool {
        const n = nodes.asName(node) orelse return false;
        // Look up rule in grammar by name
        const g = nodes.asGrammar(self.grammar) orelse return false;
        const rules = getVectorElements(g.rules) orelse return false;
        for (0..g.rule_count) |i| {
            const rule = nodes.asRule(rules[i]) orelse continue;
            if (rule.name == n.rule_name) {
                return self.matchNode(rule.expr);
            }
        }
        return false; // rule not found
    }

    fn matchRule(self: *Parser, node: Oop) bool {
        const rule = nodes.asRule(node) orelse return false;
        return self.matchNode(rule.expr);
    }

    fn matchStringLit(self: *Parser, node: Oop) bool {
        const s = nodes.asStringLit(node) orelse return false;
        const ptr = s.ptr orelse return false;
        const lit = ptr[0..s.len];
        if (self.pos + s.len > self.input.len) return false;
        if (!std.mem.eql(u8, self.input[self.pos .. self.pos + s.len], lit)) return false;
        self.pos += s.len;
        return true;
    }

    fn matchCharClass(self: *Parser, node: Oop) bool {
        const c = nodes.asCharClass(node) orelse return false;
        const table = c.table orelse return false;
        if (self.pos >= self.input.len) return false;
        if (table[self.input[self.pos]] != 0) {
            self.pos += 1;
            return true;
        }
        return false;
    }

    fn matchDot(self: *Parser) bool {
        if (self.pos >= self.input.len) return false;
        self.pos += 1;
        return true;
    }

    fn matchCaptureNode(self: *Parser, node: Oop) bool {
        const c = nodes.asCapture(node) orelse return false;
        const begin = self.pos;
        if (!self.matchNode(c.element)) return false;
        if (self.capture_count < MAX_CAPTURES) {
            self.captures[self.capture_count] = .{ .begin = begin, .end = self.pos };
            self.capture_count += 1;
        }
        return true;
    }
};

// -- Helper: read vector elements from an id Vector object ------------------

/// Read elements pointer from a vector Oop. The vector layout is:
/// [vtable_slot][size: usize][elem0][elem1]...
/// The object pointer points past vtable_slot, so data starts at the object.
fn getVectorElements(vec_oop: Oop) ?[*]Oop {
    const o = vec_oop orelse return null;
    const base: [*]u8 = @ptrCast(@alignCast(o));
    // Skip the size field to get to elements
    return @ptrCast(@alignCast(base + @sizeOf(usize)));
}
