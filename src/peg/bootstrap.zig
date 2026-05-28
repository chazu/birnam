const std = @import("std");
const nodes = @import("nodes.zig");
const IdApi = @import("api.zig").IdApi;
const Oop = nodes.Oop;

/// Hand-written recursive descent parser for PEG grammar syntax.
///
/// Grammar parsed:
///   grammar    = spacing (rule)* end-of-input
///   rule       = identifier EQUALS expression SEMI?
///   expression = sequence (BAR sequence)*
///   sequence   = prefix+
///   prefix     = AND suffix | NOT suffix | suffix
///   suffix     = primary (STAR | PLUS | QUESTION)?
///   primary    = identifier !(EQUALS)
///              | OPEN expression CLOSE
///              | literal | class | DOT
///              | LANGLE expression RANGLE
///
pub const PegParser = struct {
    src: []const u8,
    pos: usize,
    id: *const IdApi,
    err_msg: ?[]const u8,

    // Scratch buffer for building element arrays (sequences/alternates)
    scratch: [128]Oop,
    scratch_count: usize,

    // Char class table allocation
    class_tables: [32]*[256]u8,
    class_count: usize,

    pub fn init(src: []const u8, api: *const IdApi) PegParser {
        return .{
            .src = src,
            .pos = 0,
            .id = api,
            .err_msg = null,
            .scratch = [_]Oop{null} ** 128,
            .scratch_count = 0,
            .class_tables = undefined,
            .class_count = 0,
        };
    }

    pub fn parseGrammar(self: *PegParser) Oop {
        self.spacing();

        // Collect rules into scratch buffer
        self.scratch_count = 0;
        var first_rule: Oop = null;

        while (self.pos < self.src.len) {
            self.spacing();
            if (self.pos >= self.src.len) break;
            if (!isIdentStart(self.peek())) break;

            const rule = self.parseRule() orelse {
                return null;
            };
            if (first_rule == null) first_rule = rule;
            if (self.scratch_count < self.scratch.len) {
                self.scratch[self.scratch_count] = rule;
                self.scratch_count += 1;
            }
        }

        if (self.scratch_count == 0) {
            self.err_msg = "empty grammar";
            return null;
        }

        // Build grammar object
        const g = nodes.makeGrammar() orelse return null;
        const grammar = nodes.asGrammar(g).?;

        // Build rules vector
        const rules_vec = self.buildVector(self.scratch[0..self.scratch_count]) orelse return null;
        grammar.rules = rules_vec;
        grammar.rule_count = self.scratch_count;

        // Start rule is the expression of the first rule
        const first = nodes.asRule(first_rule).?;
        grammar.start_rule = first.expr;

        return g;
    }

    fn parseRule(self: *PegParser) ?Oop {
        var name = self.parseIdentifier() orelse {
            self.err_msg = "expected rule name";
            return null;
        };
        self.spacing();

        if (!self.matchChar('=')) {
            self.err_msg = "expected '=' after rule name";
            return null;
        }
        self.spacing();

        const expr = self.parseExpression() orelse return null;

        // Optional semicolon
        self.spacing();
        _ = self.matchChar(';');
        self.spacing();

        const name_sel = self.id.intern(name.ptr());
        return nodes.makeRule(name_sel, expr);
    }

    fn parseExpression(self: *PegParser) ?Oop {
        const first = self.parseSequenceExpr() orelse return null;

        // Check for alternates: (BAR sequence)*
        var alts: [64]Oop = undefined;
        var alt_count: usize = 0;
        alts[0] = first;
        alt_count = 1;

        while (true) {
            self.spacing();
            if (!self.matchChar('/')) break;
            self.spacing();
            const seq = self.parseSequenceExpr() orelse return null;
            if (alt_count < alts.len) {
                alts[alt_count] = seq;
                alt_count += 1;
            }
        }

        if (alt_count == 1) return first;

        // Build Alternate node
        const alt = nodes.makeAlternate() orelse return null;
        const a = nodes.asAlternate(alt).?;
        a.elements = self.buildVector(alts[0..alt_count]) orelse return null;
        a.count = alt_count;
        return alt;
    }

    fn parseSequenceExpr(self: *PegParser) ?Oop {
        var elems: [64]Oop = undefined;
        var count: usize = 0;

        while (true) {
            self.spacing();
            if (self.pos >= self.src.len) break;
            const ch = self.peek();
            // Stop at tokens that end a sequence
            if (ch == '/' or ch == ')' or ch == ';' or ch == '>' or ch == '}') break;
            // Stop at next rule definition (identifier followed by '=')
            if (isIdentStart(ch)) {
                const saved = self.pos;
                _ = self.parseIdentifier();
                self.spacing();
                const is_rule_def = self.pos < self.src.len and self.peek() == '=';
                self.pos = saved;
                if (is_rule_def) break;
            }

            const prefix = self.parsePrefix() orelse break;
            if (count < elems.len) {
                elems[count] = prefix;
                count += 1;
            }
        }

        if (count == 0) {
            self.err_msg = "expected at least one element in sequence";
            return null;
        }
        if (count == 1) return elems[0];

        // Build Sequence node
        const seq = nodes.makeSequence() orelse return null;
        const s = nodes.asSequence(seq).?;
        s.elements = self.buildVector(elems[0..count]) orelse return null;
        s.count = count;
        return seq;
    }

    fn parsePrefix(self: *PegParser) ?Oop {
        self.spacing();
        if (self.pos >= self.src.len) return null;

        if (self.matchChar('&')) {
            self.spacing();
            const suffix = self.parseSuffix() orelse return null;
            return nodes.makePeekFor(suffix);
        }
        if (self.matchChar('!')) {
            self.spacing();
            const suffix = self.parseSuffix() orelse return null;
            return nodes.makePeekNot(suffix);
        }
        return self.parseSuffix();
    }

    fn parseSuffix(self: *PegParser) ?Oop {
        var primary = self.parsePrimary() orelse return null;
        self.spacing();
        if (self.pos < self.src.len) {
            const ch = self.peek();
            if (ch == '*') {
                self.advance();
                primary = nodes.makeStar(primary) orelse return null;
            } else if (ch == '+') {
                self.advance();
                primary = nodes.makePlus(primary) orelse return null;
            } else if (ch == '?') {
                self.advance();
                primary = nodes.makeQuery(primary) orelse return null;
            }
        }
        return primary;
    }

    fn parsePrimary(self: *PegParser) ?Oop {
        self.spacing();
        if (self.pos >= self.src.len) return null;

        const ch = self.peek();

        // Parenthesized expression
        if (ch == '(') {
            self.advance();
            self.spacing();
            const expr = self.parseExpression() orelse return null;
            self.spacing();
            if (!self.matchChar(')')) {
                self.err_msg = "expected ')'";
                return null;
            }
            return expr;
        }

        // Capture: < expression >
        if (ch == '<') {
            self.advance();
            self.spacing();
            const expr = self.parseExpression() orelse return null;
            self.spacing();
            if (!self.matchChar('>')) {
                self.err_msg = "expected '>'";
                return null;
            }
            return nodes.makeCapture(expr);
        }

        // String literal
        if (ch == '\'' or ch == '"') {
            return self.parseLiteral();
        }

        // Character class
        if (ch == '[') {
            return self.parseClass();
        }

        // Dot
        if (ch == '.') {
            self.advance();
            return nodes.makeDot();
        }

        // Action block
        if (ch == '{') {
            return self.parseAction();
        }

        // Identifier (rule reference)
        if (isIdentStart(ch)) {
            var ident = self.parseIdentifier() orelse return null;
            const sel = self.id.intern(ident.ptr());
            return nodes.makeName(sel);
        }

        return null;
    }

    fn parseLiteral(self: *PegParser) ?Oop {
        const quote = self.peek();
        self.advance();
        const start = self.pos;
        while (self.pos < self.src.len and self.src[self.pos] != quote) {
            if (self.src[self.pos] == '\\' and self.pos + 1 < self.src.len) {
                self.pos += 2; // skip escape
            } else {
                self.pos += 1;
            }
        }
        const end = self.pos;
        if (self.pos < self.src.len) self.advance(); // skip closing quote
        return nodes.makeStringLit(self.src[start..end]);
    }

    fn parseClass(self: *PegParser) ?Oop {
        self.advance(); // skip [
        const table = self.allocClassTable() orelse return null;
        @memset(table, 0);

        var negate = false;
        if (self.pos < self.src.len and self.src[self.pos] == '^') {
            negate = true;
            self.pos += 1;
        }

        while (self.pos < self.src.len and self.src[self.pos] != ']') {
            const c = self.src[self.pos];
            self.pos += 1;
            if (self.pos + 1 < self.src.len and self.src[self.pos] == '-' and self.src[self.pos + 1] != ']') {
                self.pos += 1; // skip -
                const end = self.src[self.pos];
                self.pos += 1;
                var i: usize = c;
                while (i <= end) : (i += 1) {
                    table[i] = 1;
                }
            } else {
                table[c] = 1;
            }
        }
        if (self.pos < self.src.len) self.pos += 1; // skip ]

        if (negate) {
            for (0..256) |i| {
                table[i] = if (table[i] == 0) 1 else 0;
            }
        }

        return nodes.makeCharClass(table);
    }

    fn parseAction(self: *PegParser) ?Oop {
        self.advance(); // skip {
        const start = self.pos;
        var depth: usize = 1;
        while (self.pos < self.src.len and depth > 0) {
            if (self.src[self.pos] == '{') depth += 1;
            if (self.src[self.pos] == '}') depth -= 1;
            if (depth > 0) self.pos += 1;
        }
        const end = self.pos;
        if (self.pos < self.src.len) self.pos += 1; // skip }
        return nodes.makeAction(self.src[start..end]);
    }

    // -- Lexer helpers --

    const SentinelStr = struct {
        buf: [256:0]u8,

        fn ptr(self: *SentinelStr) [*:0]const u8 {
            return @ptrCast(&self.buf);
        }
    };

    fn parseIdentifier(self: *PegParser) ?SentinelStr {
        if (self.pos >= self.src.len or !isIdentStart(self.peek())) return null;
        const start = self.pos;
        while (self.pos < self.src.len and isIdentChar(self.src[self.pos])) {
            self.pos += 1;
        }
        const len = self.pos - start;
        if (len >= 256) return null;
        var result: SentinelStr = undefined;
        @memcpy(result.buf[0..len], self.src[start .. start + len]);
        result.buf[len] = 0;
        return result;
    }

    fn spacing(self: *PegParser) void {
        while (self.pos < self.src.len) {
            const ch = self.src[self.pos];
            if (ch == ' ' or ch == '\t' or ch == '\n' or ch == '\r') {
                self.pos += 1;
            } else if (ch == '#') {
                // Line comment
                while (self.pos < self.src.len and self.src[self.pos] != '\n') {
                    self.pos += 1;
                }
            } else break;
        }
    }

    fn peek(self: *PegParser) u8 {
        return self.src[self.pos];
    }

    fn advance(self: *PegParser) void {
        self.pos += 1;
    }

    fn matchChar(self: *PegParser, ch: u8) bool {
        if (self.pos < self.src.len and self.src[self.pos] == ch) {
            self.pos += 1;
            return true;
        }
        return false;
    }

    fn isIdentStart(ch: u8) bool {
        return (ch >= 'a' and ch <= 'z') or (ch >= 'A' and ch <= 'Z') or ch == '_';
    }

    fn isIdentChar(ch: u8) bool {
        return isIdentStart(ch) or (ch >= '0' and ch <= '9') or ch == '-';
    }

    // -- Build a vector id object from a Zig slice ---

    fn buildVector(self: *PegParser, items: []Oop) Oop {
        // Allocate a vector object: [vtable_slot][size: usize][elem0][elem1]...
        const vec_size = @sizeOf(usize) + @sizeOf(Oop) * items.len;
        const v = self.id.alloc(self.id.vector_vt, vec_size) orelse return null;
        // Set size field
        const size_ptr: *usize = @ptrCast(@alignCast(v));
        size_ptr.* = items.len;
        // Set elements
        const base: [*]u8 = @ptrCast(@alignCast(v));
        const elems: [*]Oop = @ptrCast(@alignCast(base + @sizeOf(usize)));
        for (items, 0..) |item, i| {
            elems[i] = item;
        }
        return v;
    }

    fn allocClassTable(self: *PegParser) ?*[256]u8 {
        if (self.class_count >= self.class_tables.len) return null;
        // Use page allocator for the table
        const raw = std.heap.page_allocator.alloc(u8, 256) catch return null;
        const ptr: *[256]u8 = @ptrCast(raw.ptr);
        self.class_tables[self.class_count] = ptr;
        self.class_count += 1;
        return ptr;
    }
};
