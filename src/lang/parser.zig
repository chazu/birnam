const std = @import("std");
const ast = @import("ast.zig");

pub const ParseError = error{
    UnexpectedToken,
    UnexpectedEof,
    OutOfMemory,
    InvalidInteger,
};

pub const SourcePos = struct {
    line: usize,
    col: usize,
};

pub const Parser = struct {
    source: []const u8,
    pos: usize,
    arena: std.mem.Allocator,

    pub fn init(source: []const u8, arena: std.mem.Allocator) Parser {
        return .{
            .source = source,
            .pos = 0,
            .arena = arena,
        };
    }

    /// Compute line and column for the current position (1-based).
    pub fn currentPos(self: *const Parser) SourcePos {
        return posToLineCol(self.source, self.pos);
    }

    pub fn posToLineCol(source: []const u8, pos: usize) SourcePos {
        var line: usize = 1;
        var col: usize = 1;
        const end = @min(pos, source.len);
        for (source[0..end]) |ch| {
            if (ch == '\n') {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        return .{ .line = line, .col = col };
    }

    // -- Top-level parsing ----------------------------------------------------

    /// Parse the entire input as a sequence of top-level forms separated by `.`
    pub fn parseTopLevel(self: *Parser) ParseError!?*ast.Node {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.source.len) return null;

        var stmts: std.ArrayList(*ast.Node) = .{};

        const first = try self.parseTopLevelForm() orelse return null;
        stmts.append(self.arena, first) catch return error.OutOfMemory;

        while (self.pos < self.source.len) {
            self.skipWhitespaceAndComments();
            if (self.pos < self.source.len and self.source[self.pos] == '.') {
                self.pos += 1;
                self.skipWhitespaceAndComments();
                if (self.pos >= self.source.len) break;
                const stmt = try self.parseTopLevelForm() orelse break;
                stmts.append(self.arena, stmt) catch return error.OutOfMemory;
            } else {
                break;
            }
        }

        if (stmts.items.len == 1) return stmts.items[0];
        return ast.makeSequence(self.arena, stmts.items) catch return error.OutOfMemory;
    }

    /// Parse a top-level form: type definition, method definition, or statement
    fn parseTopLevelForm(self: *Parser) ParseError!?*ast.Node {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.source.len) return null;

        // Try to detect type definition, method definition, or global assignment.
        // These start with an uppercase identifier.
        const saved = self.pos;
        if (self.pos < self.source.len and std.ascii.isUpper(self.source[self.pos])) {
            if (self.parseIdentifier()) |type_name| {
                self.skipWhitespaceAndComments();

                // Global assignment: Name := expr
                if (self.pos + 1 < self.source.len and self.source[self.pos] == ':' and self.source[self.pos + 1] == '=') {
                    self.pos += 2; // skip ':='
                    self.skipWhitespaceAndComments();
                    const value = try self.parseKeywordSend() orelse return error.UnexpectedEof;
                    return ast.makeNode(self.arena, .global_assign, .{ .global_assign = .{
                        .name = type_name,
                        .value = value,
                    } }) catch return error.OutOfMemory;
                }

                // Type definition: TypeName : ParentType ( slot1 slot2 )
                if (self.pos < self.source.len and self.source[self.pos] == ':' and
                    (self.pos + 1 >= self.source.len or self.source[self.pos + 1] != '='))
                {
                    // Check it's not a keyword send (keyword: has alpha after colon)
                    // Type def has `: ` followed by uppercase parent name
                    const colon_saved = self.pos;
                    self.pos += 1; // skip ':'
                    self.skipWhitespaceAndComments();
                    if (self.pos < self.source.len and std.ascii.isUpper(self.source[self.pos])) {
                        // This looks like a type definition
                        self.pos = colon_saved;
                        return self.parseTypeDef(type_name);
                    }
                    self.pos = colon_saved;
                }

                // Method definition: TypeName selector [body]
                // Look ahead for a '[' that would indicate a method body
                if (self.tryParseMethodDef(type_name)) |method_def| {
                    return method_def;
                } else |err| switch (err) {
                    error.NotAMethodDef => {},
                    else => |e| return e,
                }

                // Not a type/method def, backtrack to parse as regular statement
                self.pos = saved;
            }
        }

        return self.parseStatement();
    }

    /// Parse a type definition: `TypeName : ParentType ( slot1 slot2 )`
    /// Called after type_name has been parsed and we've verified `:` + uppercase follows.
    fn parseTypeDef(self: *Parser, type_name: []const u8) ParseError!*ast.Node {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.source.len or self.source[self.pos] != ':') return error.UnexpectedToken;
        self.pos += 1; // skip ':'
        self.skipWhitespaceAndComments();

        const parent = self.parseIdentifier() orelse return error.UnexpectedToken;
        self.skipWhitespaceAndComments();

        // Parse optional slot list: ( slot1 slot2 )
        var slots: std.ArrayList([]const u8) = .{};
        if (self.pos < self.source.len and self.source[self.pos] == '(') {
            self.pos += 1; // skip '('
            self.skipWhitespaceAndComments();
            while (self.pos < self.source.len and self.source[self.pos] != ')') {
                const slot = self.parseIdentifier() orelse break;
                slots.append(self.arena, slot) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
            }
            if (self.pos >= self.source.len or self.source[self.pos] != ')') return error.UnexpectedToken;
            self.pos += 1; // skip ')'
        }

        return ast.makeTypeDef(self.arena, type_name, parent, slots.items) catch return error.OutOfMemory;
    }

    const MethodDefError = ParseError || error{NotAMethodDef};

    /// Try to parse a method definition: `TypeName selector [body]`
    /// Returns NotAMethodDef if this doesn't look like a method def.
    fn tryParseMethodDef(self: *Parser, type_name: []const u8) MethodDefError!*ast.Node {
        const saved = self.pos;

        // Try to parse a method selector pattern + '[' body ']'
        // Unary: TypeName name [body]
        // Binary: TypeName op: param [body]
        // Keyword: TypeName key1: p1 key2: p2 [body]

        var selector_buf: std.ArrayList(u8) = .{};
        var params: std.ArrayList([]const u8) = .{};

        // Try keyword/binary pattern first
        if (self.parseKeyword()) |kw| {
            // Keyword method: key1: p1 key2: p2 ... [body]
            selector_buf.appendSlice(self.arena, kw) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
            const param = self.parseIdentifier() orelse {
                self.pos = saved;
                return error.NotAMethodDef;
            };
            params.append(self.arena, param) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();

            // More keyword parts?
            while (self.parseKeyword()) |kw2| {
                selector_buf.appendSlice(self.arena, kw2) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
                const p2 = self.parseIdentifier() orelse {
                    self.pos = saved;
                    return error.NotAMethodDef;
                };
                params.append(self.arena, p2) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
            }
        } else if (self.parseBinarySelector()) |bin_sel| {
            // Binary method: TypeName op param [body]
            selector_buf.appendSlice(self.arena, bin_sel) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
            const param = self.parseIdentifier() orelse {
                self.pos = saved;
                return error.NotAMethodDef;
            };
            params.append(self.arena, param) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
        } else if (self.parseIdentifier()) |name| {
            // Unary method: TypeName name [body]
            selector_buf.appendSlice(self.arena, name) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
        } else {
            self.pos = saved;
            return error.NotAMethodDef;
        }

        // Expect '[' for method body
        if (self.pos >= self.source.len or self.source[self.pos] != '[') {
            self.pos = saved;
            return error.NotAMethodDef;
        }

        // Parse the body as a block (reuse block parser)
        const block_node = self.parseBlock() catch |err| {
            self.pos = saved;
            return err;
        };

        return ast.makeMethodDef(
            self.arena,
            type_name,
            selector_buf.items,
            params.items,
            block_node.data.block.body,
        ) catch return error.OutOfMemory;
    }

    /// Parse a single statement (keyword-level expression, or assignment), including cascades.
    fn parseStatement(self: *Parser) ParseError!?*ast.Node {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.source.len) return null;

        // Check for assignment: identifier `:=` expr
        const saved = self.pos;
        if (self.parseIdentifier()) |name| {
            self.skipWhitespaceAndComments();
            if (self.matchStr(":=")) {
                self.skipWhitespaceAndComments();
                const value = try self.parseKeywordSend() orelse return error.UnexpectedEof;
                return ast.makeAssign(self.arena, name, value) catch return error.OutOfMemory;
            }
            // Not an assignment, backtrack
            self.pos = saved;
        }

        const expr = try self.parseKeywordSend() orelse return null;

        // Check for cascade: expr ; msg1 ; msg2 ...
        self.skipWhitespaceAndComments();
        if (self.pos < self.source.len and self.source[self.pos] == ';') {
            return self.parseCascade(expr);
        }

        return expr;
    }

    /// Parse cascade messages: `; msg1 ; msg2 ...`
    /// The first expression has already been parsed and is either a send or a primary.
    fn parseCascade(self: *Parser, first_expr: *ast.Node) ParseError!*ast.Node {
        // Extract the receiver and first message from the first expression.
        // The first_expr should be a send node — extract receiver + first message.
        var messages: std.ArrayList(ast.Node.CascadeMsg) = .{};
        var receiver: *ast.Node = undefined;

        if (first_expr.tag == .send) {
            receiver = first_expr.data.send.receiver;
            messages.append(self.arena, .{
                .selector = first_expr.data.send.selector,
                .args = first_expr.data.send.args,
            }) catch return error.OutOfMemory;
        } else {
            // Can't cascade on a non-send
            return error.UnexpectedToken;
        }

        while (self.pos < self.source.len and self.source[self.pos] == ';') {
            self.pos += 1; // skip ';'
            self.skipWhitespaceAndComments();

            // Parse the next message (keyword, binary, or unary)
            // We parse a "message tail" — first check keyword, then binary, then unary
            var sel_buf: std.ArrayList(u8) = .{};
            var args: std.ArrayList(*ast.Node) = .{};

            if (self.parseKeyword()) |kw| {
                // Keyword cascade message
                sel_buf.appendSlice(self.arena, kw) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
                const arg = try self.parseBinarySend() orelse return error.UnexpectedEof;
                args.append(self.arena, arg) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();

                while (self.parseKeyword()) |kw2| {
                    sel_buf.appendSlice(self.arena, kw2) catch return error.OutOfMemory;
                    self.skipWhitespaceAndComments();
                    const arg2 = try self.parseBinarySend() orelse return error.UnexpectedEof;
                    args.append(self.arena, arg2) catch return error.OutOfMemory;
                    self.skipWhitespaceAndComments();
                }
            } else if (self.parseBinarySelector()) |bin_sel| {
                // Binary cascade message
                sel_buf.appendSlice(self.arena, bin_sel) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
                const arg = try self.parseUnarySend() orelse return error.UnexpectedEof;
                args.append(self.arena, arg) catch return error.OutOfMemory;
            } else if (self.parseIdentifier()) |name| {
                // Unary cascade message
                sel_buf.appendSlice(self.arena, name) catch return error.OutOfMemory;
            } else {
                return error.UnexpectedToken;
            }

            messages.append(self.arena, .{
                .selector = sel_buf.items,
                .args = args.items,
            }) catch return error.OutOfMemory;

            self.skipWhitespaceAndComments();
        }

        return ast.makeNode(self.arena, .cascade, .{ .cascade = .{
            .receiver = receiver,
            .messages = messages.items,
        } }) catch return error.OutOfMemory;
    }

    // -- Expression parsing (precedence climbing) -----------------------------

    /// keyword send: binary (key: binary)*
    fn parseKeywordSend(self: *Parser) ParseError!?*ast.Node {
        const receiver = try self.parseBinarySend() orelse return null;

        self.skipWhitespaceAndComments();

        // Check for keyword message: identifier ending in ':'
        var keywords: std.ArrayList(u8) = .{};
        var args: std.ArrayList(*ast.Node) = .{};
        var found_keyword = false;

        while (self.pos < self.source.len) {
            const saved = self.pos;
            if (self.parseKeyword()) |kw| {
                keywords.appendSlice(self.arena, kw) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
                const arg = try self.parseBinarySend() orelse return error.UnexpectedEof;
                args.append(self.arena, arg) catch return error.OutOfMemory;
                found_keyword = true;
                self.skipWhitespaceAndComments();
            } else {
                self.pos = saved;
                break;
            }
        }

        if (found_keyword) {
            return ast.makeSend(self.arena, receiver, keywords.items, args.items) catch return error.OutOfMemory;
        }
        return receiver;
    }

    /// binary send: unary (binop unary)*
    fn parseBinarySend(self: *Parser) ParseError!?*ast.Node {
        var receiver = try self.parseUnarySend() orelse return null;

        while (true) {
            self.skipWhitespaceAndComments();
            if (self.pos >= self.source.len) break;

            if (self.parseBinarySelector()) |sel| {
                self.skipWhitespaceAndComments();
                const arg = try self.parseUnarySend() orelse return error.UnexpectedEof;
                receiver = ast.makeSend(self.arena, receiver, sel, &.{arg}) catch return error.OutOfMemory;
            } else {
                break;
            }
        }

        return receiver;
    }

    /// unary send: primary (identifier)*  where identifier is NOT followed by ':'
    fn parseUnarySend(self: *Parser) ParseError!?*ast.Node {
        var receiver = try self.parsePrimary() orelse return null;

        while (true) {
            self.skipWhitespaceAndComments();
            if (self.pos >= self.source.len) break;

            const saved = self.pos;
            if (self.parseIdentifier()) |name| {
                // Make sure it's not a keyword (no ':' follows) and not `:=`
                if (self.pos < self.source.len and self.source[self.pos] == ':') {
                    self.pos = saved;
                    break;
                }
                receiver = ast.makeSend(self.arena, receiver, name, &.{}) catch return error.OutOfMemory;
            } else {
                break;
            }
        }

        return receiver;
    }

    /// primary: integer | string | variable | block | '(' expr ')'
    fn parsePrimary(self: *Parser) ParseError!?*ast.Node {
        self.skipWhitespaceAndComments();
        if (self.pos >= self.source.len) return null;

        const ch = self.source[self.pos];

        // Negative integer: '-' followed by digit
        if (ch == '-' and self.pos + 1 < self.source.len and std.ascii.isDigit(self.source[self.pos + 1])) {
            return self.parseInteger();
        }

        // Integer
        if (std.ascii.isDigit(ch)) {
            return self.parseInteger();
        }

        // String literal
        if (ch == '\'') {
            return self.parseString();
        }

        // Character literal: $a, $\n, etc.
        if (ch == '$') {
            return self.parseCharLiteral();
        }

        // Symbol literal or array literal: #foo, #at:put:, #'spaced', #(1 2 3)
        if (ch == '#') {
            // Array literal: #( ... )
            if (self.pos + 1 < self.source.len and self.source[self.pos + 1] == '(') {
                return self.parseArrayLiteral();
            }
            return self.parseSymbol();
        }

        // Block literal: [ ... ]
        if (ch == '[') {
            return self.parseBlock();
        }

        // Return expression: ^value
        if (ch == '^') {
            self.pos += 1;
            self.skipWhitespaceAndComments();
            const value = try self.parseKeywordSend() orelse return error.UnexpectedEof;
            return ast.makeReturnExpr(self.arena, value) catch return error.OutOfMemory;
        }

        // Parenthesized expression
        if (ch == '(') {
            self.pos += 1;
            self.skipWhitespaceAndComments();
            const expr = try self.parseKeywordSend() orelse return error.UnexpectedEof;
            self.skipWhitespaceAndComments();
            if (self.pos < self.source.len and self.source[self.pos] == ')') {
                self.pos += 1;
            } else {
                return error.UnexpectedToken;
            }
            return expr;
        }

        // Variable
        if (std.ascii.isAlphabetic(ch) or ch == '_') {
            if (self.parseIdentifier()) |name| {
                return ast.makeVariable(self.arena, name) catch return error.OutOfMemory;
            }
        }

        return null;
    }

    /// Parse a block: `[body]` or `[:p1 :p2 | body]` or `[:p1 | | tmp | body]`
    fn parseBlock(self: *Parser) ParseError!*ast.Node {
        if (self.pos >= self.source.len or self.source[self.pos] != '[') return error.UnexpectedToken;
        self.pos += 1; // skip '['
        self.skipWhitespaceAndComments();

        // Parse block parameters: :param1 :param2 ... |
        var params: std.ArrayList([]const u8) = .{};
        if (self.pos < self.source.len and self.source[self.pos] == ':') {
            while (self.pos < self.source.len and self.source[self.pos] == ':') {
                self.pos += 1; // skip ':'
                const name = self.parseIdentifier() orelse return error.UnexpectedToken;
                params.append(self.arena, name) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
            }
            // Expect '|' after params
            if (self.pos >= self.source.len or self.source[self.pos] != '|') return error.UnexpectedToken;
            self.pos += 1; // skip '|'
            self.skipWhitespaceAndComments();
        }

        // Parse optional temps: | tmp1 tmp2 |
        var temps: std.ArrayList([]const u8) = .{};
        if (self.pos < self.source.len and self.source[self.pos] == '|') {
            self.pos += 1; // skip '|'
            self.skipWhitespaceAndComments();
            while (self.pos < self.source.len and self.source[self.pos] != '|') {
                const name = self.parseIdentifier() orelse break;
                temps.append(self.arena, name) catch return error.OutOfMemory;
                self.skipWhitespaceAndComments();
            }
            if (self.pos >= self.source.len or self.source[self.pos] != '|') return error.UnexpectedToken;
            self.pos += 1; // skip closing '|'
            self.skipWhitespaceAndComments();
        }

        // Parse body statements
        var body: std.ArrayList(*ast.Node) = .{};
        while (self.pos < self.source.len and self.source[self.pos] != ']') {
            const stmt = try self.parseStatement() orelse break;
            body.append(self.arena, stmt) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
            if (self.pos < self.source.len and self.source[self.pos] == '.') {
                self.pos += 1;
                self.skipWhitespaceAndComments();
            }
        }

        if (self.pos >= self.source.len or self.source[self.pos] != ']') return error.UnexpectedToken;
        self.pos += 1; // skip ']'

        return ast.makeBlock(self.arena, params.items, temps.items, body.items) catch return error.OutOfMemory;
    }

    /// Parse a character literal: `$a`, `$\n`, `$\\`, `$ ` etc.
    fn parseCharLiteral(self: *Parser) ParseError!*ast.Node {
        if (self.pos >= self.source.len or self.source[self.pos] != '$') return error.UnexpectedToken;
        self.pos += 1; // skip '$'
        if (self.pos >= self.source.len) return error.UnexpectedEof;

        var ch: u8 = undefined;
        if (self.source[self.pos] == '\\') {
            // Escape sequence
            self.pos += 1;
            if (self.pos >= self.source.len) return error.UnexpectedEof;
            ch = switch (self.source[self.pos]) {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '0' => 0,
                else => self.source[self.pos],
            };
        } else {
            ch = self.source[self.pos];
        }
        self.pos += 1;
        return ast.makeNode(self.arena, .char_lit, .{ .char_lit = ch }) catch return error.OutOfMemory;
    }

    /// Parse an array literal: `#(1 2 3)` — elements are primary expressions separated by whitespace.
    fn parseArrayLiteral(self: *Parser) ParseError!*ast.Node {
        if (self.pos >= self.source.len or self.source[self.pos] != '#') return error.UnexpectedToken;
        self.pos += 1; // skip '#'
        if (self.pos >= self.source.len or self.source[self.pos] != '(') return error.UnexpectedToken;
        self.pos += 1; // skip '('
        self.skipWhitespaceAndComments();

        var elements: std.ArrayList(*ast.Node) = .{};
        while (self.pos < self.source.len and self.source[self.pos] != ')') {
            const elem = try self.parsePrimary() orelse break;
            elements.append(self.arena, elem) catch return error.OutOfMemory;
            self.skipWhitespaceAndComments();
        }

        if (self.pos >= self.source.len or self.source[self.pos] != ')') return error.UnexpectedToken;
        self.pos += 1; // skip ')'

        return ast.makeNode(self.arena, .array_lit, .{ .array_lit = .{ .elements = elements.items } }) catch return error.OutOfMemory;
    }

    // -- Token parsers --------------------------------------------------------

    fn parseInteger(self: *Parser) ParseError!*ast.Node {
        const start = self.pos;
        // Handle negative sign
        if (self.pos < self.source.len and self.source[self.pos] == '-') {
            self.pos += 1;
        }
        // Check for radix notation: digits followed by 'r'
        const digit_start = self.pos;
        while (self.pos < self.source.len and std.ascii.isDigit(self.source[self.pos])) {
            self.pos += 1;
        }
        if (self.pos == digit_start) return error.InvalidInteger;

        if (self.pos < self.source.len and self.source[self.pos] == 'r') {
            // Radix notation: NrDIGITS
            const radix_str = self.source[digit_start..self.pos];
            const radix = std.fmt.parseInt(u8, radix_str, 10) catch return error.InvalidInteger;
            self.pos += 1; // skip 'r'
            const val_start = self.pos;
            while (self.pos < self.source.len and isRadixDigit(self.source[self.pos], radix)) {
                self.pos += 1;
            }
            if (self.pos == val_start) return error.InvalidInteger;
            const is_neg = self.source[start] == '-';
            const val = std.fmt.parseInt(i64, self.source[val_start..self.pos], radix) catch return error.InvalidInteger;
            return ast.makeIntegerLit(self.arena, if (is_neg) -val else val) catch return error.OutOfMemory;
        }

        // Plain decimal
        const val = std.fmt.parseInt(i64, self.source[start..self.pos], 10) catch return error.InvalidInteger;
        return ast.makeIntegerLit(self.arena, val) catch return error.OutOfMemory;
    }

    fn parseString(self: *Parser) ParseError!*ast.Node {
        if (self.pos >= self.source.len or self.source[self.pos] != '\'') return error.UnexpectedToken;
        self.pos += 1; // skip opening quote

        var buf: std.ArrayList(u8) = .{};

        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (ch == '\'') {
                // Check for escaped quote ''
                if (self.pos + 1 < self.source.len and self.source[self.pos + 1] == '\'') {
                    buf.append(self.arena, '\'') catch return error.OutOfMemory;
                    self.pos += 2;
                } else {
                    self.pos += 1; // skip closing quote
                    return ast.makeStringLit(self.arena, buf.items) catch return error.OutOfMemory;
                }
            } else {
                buf.append(self.arena, ch) catch return error.OutOfMemory;
                self.pos += 1;
            }
        }
        return error.UnexpectedEof; // unterminated string
    }

    fn parseSymbol(self: *Parser) ParseError!*ast.Node {
        if (self.pos >= self.source.len or self.source[self.pos] != '#') return error.UnexpectedToken;
        self.pos += 1; // skip '#'

        if (self.pos >= self.source.len) return error.UnexpectedEof;

        // #'quoted symbol'
        if (self.source[self.pos] == '\'') {
            self.pos += 1;
            var buf: std.ArrayList(u8) = .{};
            while (self.pos < self.source.len) {
                const c = self.source[self.pos];
                if (c == '\'') {
                    if (self.pos + 1 < self.source.len and self.source[self.pos + 1] == '\'') {
                        buf.append(self.arena, '\'') catch return error.OutOfMemory;
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                        return ast.makeNode(self.arena, .symbol_lit, .{ .symbol_lit = .{ .value = buf.items } }) catch return error.OutOfMemory;
                    }
                } else {
                    buf.append(self.arena, c) catch return error.OutOfMemory;
                    self.pos += 1;
                }
            }
            return error.UnexpectedEof;
        }

        // #identifier or #keyword:keyword: (multiple keywords form one symbol)
        const start = self.pos;
        if (self.pos < self.source.len and (std.ascii.isAlphabetic(self.source[self.pos]) or self.source[self.pos] == '_')) {
            while (self.pos < self.source.len) {
                const c = self.source[self.pos];
                if (std.ascii.isAlphanumeric(c) or c == '_') {
                    self.pos += 1;
                } else if (c == ':') {
                    self.pos += 1; // include colon
                    // Continue to parse more keyword parts
                    if (self.pos < self.source.len and (std.ascii.isAlphabetic(self.source[self.pos]) or self.source[self.pos] == '_')) {
                        continue;
                    }
                    break;
                } else break;
            }
            if (self.pos > start) {
                return ast.makeNode(self.arena, .symbol_lit, .{ .symbol_lit = .{ .value = self.source[start..self.pos] } }) catch return error.OutOfMemory;
            }
        }

        // #+ #, #~= etc. — binary selector symbols
        if (self.pos < self.source.len and isBinaryChar(self.source[self.pos])) {
            self.pos += 1;
            if (self.pos < self.source.len and isBinaryChar(self.source[self.pos])) {
                self.pos += 1;
            }
            return ast.makeNode(self.arena, .symbol_lit, .{ .symbol_lit = .{ .value = self.source[start..self.pos] } }) catch return error.OutOfMemory;
        }

        return error.UnexpectedToken;
    }

    fn parseIdentifier(self: *Parser) ?[]const u8 {
        if (self.pos >= self.source.len) return null;
        const ch = self.source[self.pos];
        if (!std.ascii.isAlphabetic(ch) and ch != '_') return null;

        const start = self.pos;
        while (self.pos < self.source.len) {
            const c = self.source[self.pos];
            if (std.ascii.isAlphanumeric(c) or c == '_') {
                self.pos += 1;
            } else break;
        }
        return self.source[start..self.pos];
    }

    /// Parse a keyword token: identifier followed by ':'
    fn parseKeyword(self: *Parser) ?[]const u8 {
        if (self.pos >= self.source.len) return null;
        const ch = self.source[self.pos];
        if (!std.ascii.isAlphabetic(ch) and ch != '_') return null;

        const start = self.pos;
        while (self.pos < self.source.len) {
            const c = self.source[self.pos];
            if (std.ascii.isAlphanumeric(c) or c == '_') {
                self.pos += 1;
            } else break;
        }
        if (self.pos < self.source.len and self.source[self.pos] == ':') {
            // Make sure it's not ':='
            if (self.pos + 1 < self.source.len and self.source[self.pos + 1] == '=') {
                self.pos = start;
                return null;
            }
            self.pos += 1; // include the ':'
            return self.source[start..self.pos];
        }
        self.pos = start;
        return null;
    }

    /// Parse a binary selector: one or two characters from +-*/\<>=~&|@%,!?
    fn parseBinarySelector(self: *Parser) ?[]const u8 {
        if (self.pos >= self.source.len) return null;
        const ch = self.source[self.pos];

        if (!isBinaryChar(ch)) return null;
        // Don't consume '^' (return) or '(' or ')'
        if (ch == '^') return null;

        const start = self.pos;
        self.pos += 1;
        // Allow two-character binary selectors
        if (self.pos < self.source.len and isBinaryChar(self.source[self.pos])) {
            // But not if second char starts a negative number
            if (self.source[self.pos] == '-' or self.source[self.pos] == '(') {
                // don't consume
            } else {
                self.pos += 1;
            }
        }
        return self.source[start..self.pos];
    }

    fn matchStr(self: *Parser, needle: []const u8) bool {
        if (self.pos + needle.len > self.source.len) return false;
        if (std.mem.eql(u8, self.source[self.pos .. self.pos + needle.len], needle)) {
            self.pos += needle.len;
            return true;
        }
        return false;
    }

    fn skipWhitespaceAndComments(self: *Parser) void {
        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (ch == ' ' or ch == '\t' or ch == '\n' or ch == '\r') {
                self.pos += 1;
            } else if (ch == '"') {
                // Comment: skip to closing "
                self.pos += 1;
                while (self.pos < self.source.len and self.source[self.pos] != '"') {
                    self.pos += 1;
                }
                if (self.pos < self.source.len) self.pos += 1; // skip closing "
            } else break;
        }
    }
};

fn isBinaryChar(ch: u8) bool {
    return switch (ch) {
        '+', '-', '*', '/', '\\', '<', '>', '=', '~', '&', '|', '@', '%', ',', '!', '?' => true,
        else => false,
    };
}

fn isRadixDigit(ch: u8, radix: u8) bool {
    if (radix <= 10) {
        return ch >= '0' and ch < '0' + radix;
    }
    if (ch >= '0' and ch <= '9') return true;
    const upper = std.ascii.toUpper(ch);
    return upper >= 'A' and upper < 'A' + (radix - 10);
}
