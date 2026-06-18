//! birnam --lsp — minimal Language Server Protocol server (Phase: LSP P0).
//!
//! Host decision (see TASKS.md): a THIN zig JSON-RPC envelope living in the
//! same binary, delegating all semantics to the already-booted PEG / lang
//! frontend + fact store. No second parser, no external file index.
//!
//! P0 scope — parse-only diagnostics:
//!   - stdio JSON-RPC 2.0 with `Content-Length` framing
//!   - initialize / initialized / shutdown / exit
//!   - textDocument/didOpen | didChange (full sync) | didClose
//!   - publishDiagnostics from PEG parse failures
//!
//! How diagnostics work: we send `parseExpr:` to the lang module entity (it is
//! tagged LANG_VT, where the method is installed). On a syntax error the parser
//! returns an ERROR_VT entity whose `:peg-position` attribute is the furthest
//! byte offset the PEG reached — already tracked by the engine. We convert that
//! offset to an LSP position and publish a single diagnostic. On success the
//! result is an AST node (also vtable-tagged), so we discriminate strictly on
//! `:vtable == ERROR_VT`, not on the mere presence of a vtable.
//!
//! KNOWN P0 LIMITATIONS (follow-ups, not blockers):
//!   - Each parse asserts CST/AST facts into the live store; over a long
//!     editing session this grows unbounded. A prune hook belongs on the
//!     didChange path (the REPL prunes at each line boundary; this loop does
//!     not yet).
//!   - Positions are computed in BYTES, not UTF-16 code units. ASCII sources
//!     are exact; multi-byte UTF-8 will be off. LSP spec default is UTF-16.
//!   - Only the first/furthest parse error is reported. No static checks yet
//!     (unbound-var scope walk + arity are the next P0 increment).

const std = @import("std");
const id_api_mod = @import("kernel/id_api.zig");
const ref_mod = @import("kernel/ref.zig");
const ke = @import("kernel/kernel_eids.zig");
const interner_mod = @import("kernel/interner.zig");

const IdApi = id_api_mod.IdApi;
const Ref = ref_mod.Ref;

/// Reserved words offered as a completion floor even before anything is parsed.
const KEYWORD_FLOOR = [_][]const u8{ "let", "nil", "true", "false", "self", "super" };

/// Run the server loop over stdio until `exit` (or EOF). `lang_eid` is the
/// loaded lang module entity (LANG_VT) that answers `parseExpr:`.
pub fn serve(allocator: std.mem.Allocator, api: IdApi, lang_eid: u64) !void {
    if (lang_eid == 0) {
        std.debug.print("[birnam --lsp] lang module not loaded; cannot parse\n", .{});
        return;
    }

    var server = Server{
        .allocator = allocator,
        .api = api,
        .lang_eid = lang_eid,
        .interner = id_api_mod.internerPtr(),
        .docs = std.StringHashMap([]u8).init(allocator),
        .stdin = std.fs.File.stdin(),
        .stdout = std.fs.File.stdout(),
    };
    defer server.deinit();

    const sel_parse = api.intern("parseExpr:".ptr, "parseExpr:".len);
    server.sel_parse = sel_parse;

    while (true) {
        const body = readMessage(allocator, server.stdin) catch break orelse break;
        defer allocator.free(body);
        server.handle(body) catch |e| {
            std.debug.print("[birnam --lsp] handler error: {s}\n", .{@errorName(e)});
        };
        if (server.should_exit) break;
    }
}

const Server = struct {
    allocator: std.mem.Allocator,
    api: IdApi,
    lang_eid: u64,
    interner: ?*interner_mod.Interner,
    sel_parse: u64 = 0,
    docs: std.StringHashMap([]u8),
    stdin: std.fs.File,
    stdout: std.fs.File,
    should_exit: bool = false,

    fn deinit(self: *Server) void {
        var it = self.docs.iterator();
        while (it.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
            self.allocator.free(entry.value_ptr.*);
        }
        self.docs.deinit();
    }

    fn handle(self: *Server, body: []const u8) !void {
        var parsed = std.json.parseFromSlice(std.json.Value, self.allocator, body, .{}) catch return;
        defer parsed.deinit();
        const root = parsed.value;
        if (root != .object) return;
        const obj = root.object;

        const method = (obj.get("method") orelse return).string;
        const id = obj.get("id"); // present for requests, absent for notifications

        if (std.mem.eql(u8, method, "initialize")) {
            try self.replyInitialize(id);
        } else if (std.mem.eql(u8, method, "shutdown")) {
            try self.replyResultNull(id);
        } else if (std.mem.eql(u8, method, "exit")) {
            self.should_exit = true;
        } else if (std.mem.eql(u8, method, "textDocument/documentSymbol")) {
            try self.documentSymbol(id, obj.get("params"));
        } else if (std.mem.eql(u8, method, "textDocument/completion")) {
            try self.completion(id, obj.get("params"));
        } else if (std.mem.eql(u8, method, "textDocument/didOpen")) {
            try self.didOpen(obj.get("params") orelse return);
        } else if (std.mem.eql(u8, method, "textDocument/didChange")) {
            try self.didChange(obj.get("params") orelse return);
        } else if (std.mem.eql(u8, method, "textDocument/didClose")) {
            try self.didClose(obj.get("params") orelse return);
        }
        // initialized / $/* notifications and anything else: ignore.
    }

    // -- LSP responses -------------------------------------------------------

    fn replyInitialize(self: *Server, id: ?std.json.Value) !void {
        // textDocumentSync = 1 (Full): client resends whole doc on change.
        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"id\":");
        try writeId(w, id);
        try w.writeAll(",\"result\":{\"capabilities\":{\"textDocumentSync\":1,\"documentSymbolProvider\":true,\"completionProvider\":{\"triggerCharacters\":[\":\"]}},\"serverInfo\":{\"name\":\"birnam-lsp\",\"version\":\"0.1.0\"}}}");
        try self.send(buf.items);
    }

    fn replyResultNull(self: *Server, id: ?std.json.Value) !void {
        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"id\":");
        try writeId(w, id);
        try w.writeAll(",\"result\":null}");
        try self.send(buf.items);
    }

    // -- Document lifecycle --------------------------------------------------

    fn didOpen(self: *Server, params: std.json.Value) !void {
        if (params != .object) return;
        const td = (params.object.get("textDocument") orelse return);
        if (td != .object) return;
        const uri = (td.object.get("uri") orelse return).string;
        const text = (td.object.get("text") orelse return).string;
        try self.storeDoc(uri, text);
        try self.publishDiagnostics(uri, text);
    }

    fn didChange(self: *Server, params: std.json.Value) !void {
        if (params != .object) return;
        const td = (params.object.get("textDocument") orelse return);
        if (td != .object) return;
        const uri = (td.object.get("uri") orelse return).string;
        const changes = (params.object.get("contentChanges") orelse return);
        if (changes != .array or changes.array.items.len == 0) return;
        // Full sync: the last change carries the entire new document text.
        const last = changes.array.items[changes.array.items.len - 1];
        if (last != .object) return;
        const text = (last.object.get("text") orelse return).string;
        try self.storeDoc(uri, text);
        try self.publishDiagnostics(uri, text);
    }

    fn didClose(self: *Server, params: std.json.Value) !void {
        if (params != .object) return;
        const td = (params.object.get("textDocument") orelse return);
        if (td != .object) return;
        const uri = (td.object.get("uri") orelse return).string;
        if (self.docs.fetchRemove(uri)) |kv| {
            self.allocator.free(kv.key);
            self.allocator.free(kv.value);
        }
        // Clear diagnostics for the closed document.
        try self.publishEmpty(uri);
    }

    fn storeDoc(self: *Server, uri: []const u8, text: []const u8) !void {
        const text_copy = try self.allocator.dupe(u8, text);
        const gop = try self.docs.getOrPut(uri);
        if (gop.found_existing) {
            self.allocator.free(gop.value_ptr.*);
            gop.value_ptr.* = text_copy;
        } else {
            gop.key_ptr.* = try self.allocator.dupe(u8, uri);
            gop.value_ptr.* = text_copy;
        }
    }

    // -- Diagnostics ---------------------------------------------------------

    fn publishDiagnostics(self: *Server, uri: []const u8, text: []const u8) !void {
        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"");
        try writeJsonString(w, uri);
        try w.writeAll("\",\"diagnostics\":[");

        // A `.birnam` unit is `;`-separated top-forms (parseExpr: handles ONE
        // expression). Split exactly as file-in does, parse each form, and map
        // a form-local failure offset back into the buffer. NB: each parse
        // asserts CST/AST facts into the live store (see KNOWN LIMITATIONS).
        var first = true;
        var start: usize = 0;
        while (start < text.len) {
            const end = formEnd(text, start);
            const form = text[start..end];
            if (!formIsBlank(form)) {
                const src = self.api.make_string(form.ptr, form.len);
                const result = self.api.send1(ref_mod.mkEid(self.lang_eid), self.sel_parse, src);
                if (self.parseErrorPos(result)) |local| {
                    const off = start + @min(local, form.len);
                    const pos = byteToPosition(text, off);
                    if (!first) try w.writeByte(',');
                    first = false;
                    // Single-character marker at the furthest-failure point.
                    try w.print("{{\"range\":{{\"start\":{{\"line\":{d},\"character\":{d}}},\"end\":{{\"line\":{d},\"character\":{d}}}}},\"severity\":1,\"source\":\"birnam\",\"message\":\"syntax error near here\"}}", .{ pos.line, pos.character, pos.line, pos.character + 1 });
                }
            }
            if (end >= text.len) break;
            start = end + 1; // skip the ';'
        }

        try w.writeAll("]}}");
        try self.send(buf.items);
    }

    fn publishEmpty(self: *Server, uri: []const u8) !void {
        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"");
        try writeJsonString(w, uri);
        try w.writeAll("\",\"diagnostics\":[]}}");
        try self.send(buf.items);
    }

    /// If `result` is an ERROR_VT entity, return its `:peg-position` byte
    /// offset. Otherwise (AST node on success, or NIL) return null.
    fn parseErrorPos(self: *Server, result: Ref) ?usize {
        if (!ref_mod.isEid(result)) return null;
        const eid = ref_mod.untagEid(result);
        const vt = self.api.query_eavt(eid, ke.ATTR_VTABLE);
        if (!ref_mod.isEid(vt) or ref_mod.untagEid(vt) != ke.ERROR_VT) return null;
        const pos_ref = self.api.query_eavt(eid, ke.ATTR_PEG_POSITION);
        if (!ref_mod.isSmallInt(pos_ref)) return 0; // error without a position → top of file
        const n = ref_mod.untagSmallInt(pos_ref);
        return if (n < 0) 0 else @intCast(n);
    }

    // -- documentSymbol ------------------------------------------------------

    /// Outline the top-level `let` bindings of a document. Each `;`-separated
    /// form is parsed; a form whose root is AST_LET_VT contributes one
    /// Variable symbol (name via the interner, range = the form's extent).
    fn documentSymbol(self: *Server, id: ?std.json.Value, params: ?std.json.Value) !void {
        const text: []const u8 = blk: {
            const p = params orelse break :blk "";
            if (p != .object) break :blk "";
            const td = p.object.get("textDocument") orelse break :blk "";
            if (td != .object) break :blk "";
            const uri = (td.object.get("uri") orelse break :blk "").string;
            break :blk (self.docs.get(uri) orelse "");
        };

        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"id\":");
        try writeId(w, id);
        try w.writeAll(",\"result\":[");

        var first = true;
        var start: usize = 0;
        while (start < text.len) {
            const end = formEnd(text, start);
            const form = text[start..end];
            if (!formIsBlank(form)) {
                const root = self.parseForm(form);
                if (self.letName(root)) |name| {
                    // A form may carry leading whitespace/newlines (the byte
                    // after the previous `;`). Skip it so the symbol's line is
                    // the line the `let` actually starts on.
                    var s = start;
                    while (s < end and (text[s] == ' ' or text[s] == '\t' or text[s] == '\r' or text[s] == '\n')) s += 1;
                    const sp = byteToPosition(text, s);
                    const ep = byteToPosition(text, end);
                    if (!first) try w.writeByte(',');
                    first = false;
                    // SymbolKind.Variable = 13.
                    try w.writeAll("{\"name\":\"");
                    try writeJsonString(w, name);
                    try w.print("\",\"kind\":13,\"range\":{{\"start\":{{\"line\":{d},\"character\":{d}}},\"end\":{{\"line\":{d},\"character\":{d}}}}},\"selectionRange\":{{\"start\":{{\"line\":{d},\"character\":{d}}},\"end\":{{\"line\":{d},\"character\":{d}}}}}}}", .{ sp.line, sp.character, ep.line, ep.character, sp.line, sp.character, ep.line, ep.character });
                }
            }
            if (end >= text.len) break;
            start = end + 1;
        }

        try w.writeAll("]}");
        try self.send(buf.items);
    }

    /// Parse one top-form, returning the (desugared) AST root Ref (or NIL).
    fn parseForm(self: *Server, form: []const u8) Ref {
        const src = self.api.make_string(form.ptr, form.len);
        return self.api.send1(ref_mod.mkEid(self.lang_eid), self.sel_parse, src);
    }

    /// If `root` is an AST_LET_VT node, return its bound name (via the
    /// interner). Otherwise null.
    fn letName(self: *Server, root: Ref) ?[]const u8 {
        if (!ref_mod.isEid(root)) return null;
        const eid = ref_mod.untagEid(root);
        const vt = self.api.query_eavt(eid, ke.ATTR_VTABLE);
        if (!ref_mod.isEid(vt) or ref_mod.untagEid(vt) != ke.AST_LET_VT) return null;
        const name_ref = self.api.query_eavt(eid, ke.ATTR_AST_NAME);
        if (!ref_mod.isEid(name_ref)) return null;
        const interner = self.interner orelse return null;
        return interner.nameOf(ref_mod.untagEid(name_ref));
    }

    // -- completion ----------------------------------------------------------

    /// Prefix-filtered completion. Candidates: the keyword floor plus every
    /// interned name (selectors, identifiers, attributes) — the running image
    /// already interned each on parse. Filtered by the identifier prefix under
    /// the cursor so the list stays small and relevant.
    fn completion(self: *Server, id: ?std.json.Value, params: ?std.json.Value) !void {
        var text: []const u8 = "";
        var offset: usize = 0;
        if (params) |p| if (p == .object) {
            if (p.object.get("textDocument")) |td| if (td == .object) {
                if (td.object.get("uri")) |u| if (u == .string) {
                    text = self.docs.get(u.string) orelse "";
                };
            };
            if (p.object.get("position")) |pos| if (pos == .object) {
                const line: usize = if (pos.object.get("line")) |l| (if (l == .integer) @intCast(@max(0, l.integer)) else 0) else 0;
                const char: usize = if (pos.object.get("character")) |c| (if (c == .integer) @intCast(@max(0, c.integer)) else 0) else 0;
                offset = positionToByte(text, line, char);
            };
        };

        const prefix = identPrefix(text, offset);

        var seen = std.StringHashMap(void).init(self.allocator);
        defer seen.deinit();

        var buf = std.ArrayList(u8){};
        defer buf.deinit(self.allocator);
        const w = buf.writer(self.allocator);
        try w.writeAll("{\"jsonrpc\":\"2.0\",\"id\":");
        try writeId(w, id);
        try w.writeAll(",\"result\":[");

        var first = true;
        // Keyword floor (always considered).
        for (KEYWORD_FLOOR) |kw| {
            if (!matches(prefix, kw)) continue;
            if ((try seen.getOrPut(kw)).found_existing) continue;
            try self.writeCompletionItem(w, &first, kw, 14); // CompletionItemKind.Keyword
        }

        // Interned names — only when the user has typed a prefix, to avoid
        // dumping the whole symbol table on an empty trigger.
        if (prefix.len > 0) {
            if (self.interner) |interner| {
                var count: usize = 0;
                var it = interner.names();
                while (it.next()) |entry| {
                    if (count >= 300) break;
                    const name = entry.name;
                    if (name.len == 0 or !matches(prefix, name)) continue;
                    const gop = try seen.getOrPut(name);
                    if (gop.found_existing) continue;
                    // CompletionItemKind: Method=2 for keyword selectors (have
                    // a ':'), Variable=6 otherwise.
                    const kind: u8 = if (std.mem.indexOfScalar(u8, name, ':') != null) 2 else 6;
                    try self.writeCompletionItem(w, &first, name, kind);
                    count += 1;
                }
            }
        }

        try w.writeAll("]}");
        try self.send(buf.items);
    }

    fn writeCompletionItem(self: *Server, w: anytype, first: *bool, label: []const u8, kind: u8) !void {
        _ = self;
        if (!first.*) try w.writeByte(',');
        first.* = false;
        try w.writeAll("{\"label\":\"");
        try writeJsonString(w, label);
        try w.print("\",\"kind\":{d}}}", .{kind});
    }

    // -- Transport -----------------------------------------------------------

    fn send(self: *Server, json: []const u8) !void {
        var hdr: [64]u8 = undefined;
        const header = try std.fmt.bufPrint(&hdr, "Content-Length: {d}\r\n\r\n", .{json.len});
        try self.stdout.writeAll(header);
        try self.stdout.writeAll(json);
    }
};

// -- Form splitting (mirrors lang.zig `formEnd`) ------------------------------

/// Index of the next top-level `;` at or after `start`, or `src.len`. Skips
/// `;` inside string literals, `//` line comments, `/* */` block comments, and
/// nested `[...]`. Kept in sync with the file-in splitter in lang.zig.
fn formEnd(src: []const u8, start: usize) usize {
    var i = start;
    var depth: i32 = 0;
    while (i < src.len) {
        const c = src[i];
        if (c == '"') {
            i += 1;
            while (i < src.len and src[i] != '"') {
                if (src[i] == '\\' and i + 1 < src.len) i += 2 else i += 1;
            }
            i += 1;
            continue;
        }
        if (c == '/' and i + 1 < src.len and src[i + 1] == '/') {
            i += 2;
            while (i < src.len and src[i] != '\n') : (i += 1) {}
            continue;
        }
        if (c == '/' and i + 1 < src.len and src[i + 1] == '*') {
            i += 2;
            while (i + 1 < src.len and !(src[i] == '*' and src[i + 1] == '/')) : (i += 1) {}
            i += 2;
            continue;
        }
        if (c == '[') depth += 1;
        if (c == ']') depth -= 1;
        if (c == ';' and depth == 0) return i;
        i += 1;
    }
    return src.len;
}

/// True if a form carries no code — only whitespace and/or comments. Such
/// forms (blank lines, trailing `;`, comment-only lines) must not be parsed,
/// else parseExpr: would report a spurious "empty input" failure.
fn formIsBlank(form: []const u8) bool {
    var i: usize = 0;
    while (i < form.len) {
        const c = form[i];
        if (c == ' ' or c == '\t' or c == '\r' or c == '\n') {
            i += 1;
            continue;
        }
        if (c == '/' and i + 1 < form.len and form[i + 1] == '/') {
            i += 2;
            while (i < form.len and form[i] != '\n') : (i += 1) {}
            continue;
        }
        if (c == '/' and i + 1 < form.len and form[i + 1] == '*') {
            i += 2;
            while (i + 1 < form.len and !(form[i] == '*' and form[i + 1] == '/')) : (i += 1) {}
            i += 2;
            continue;
        }
        return false; // a real token
    }
    return true;
}

const Position = struct { line: usize, character: usize };

/// Convert a byte offset into an LSP (line, character) position. Counts bytes
/// per line — exact for ASCII; UTF-16 conversion is a P0 follow-up.
fn byteToPosition(text: []const u8, offset: usize) Position {
    var line: usize = 0;
    var col: usize = 0;
    var i: usize = 0;
    const stop = @min(offset, text.len);
    while (i < stop) : (i += 1) {
        if (text[i] == '\n') {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    return .{ .line = line, .character = col };
}

/// Inverse of `byteToPosition`: (line, character) → byte offset, clamped to
/// the text length. Counts bytes (ASCII-exact; UTF-16 is a follow-up).
fn positionToByte(text: []const u8, line: usize, character: usize) usize {
    var cur_line: usize = 0;
    var i: usize = 0;
    while (i < text.len and cur_line < line) : (i += 1) {
        if (text[i] == '\n') cur_line += 1;
    }
    var col: usize = 0;
    while (i < text.len and col < character and text[i] != '\n') : (i += 1) col += 1;
    return i;
}

/// The identifier run ending at `offset` (Birnam identifier chars). Drives
/// prefix-filtered completion. A leading `:` is excluded, so typing `at` (mid
/// `at:put:`) still matches keyword selectors.
fn identPrefix(text: []const u8, offset: usize) []const u8 {
    const end = @min(offset, text.len);
    var start = end;
    while (start > 0) {
        const c = text[start - 1];
        const ok = (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or
            (c >= '0' and c <= '9') or c == '_';
        if (!ok) break;
        start -= 1;
    }
    return text[start..end];
}

/// Case-sensitive prefix match. Empty prefix matches everything.
fn matches(prefix: []const u8, candidate: []const u8) bool {
    return std.mem.startsWith(u8, candidate, prefix);
}

// -- JSON helpers -------------------------------------------------------------

fn writeId(w: anytype, id: ?std.json.Value) !void {
    if (id) |v| switch (v) {
        .integer => |n| try w.print("{d}", .{n}),
        .string => |s| {
            try w.writeByte('"');
            try writeJsonString(w, s);
            try w.writeByte('"');
        },
        else => try w.writeAll("null"),
    } else try w.writeAll("null");
}

fn writeJsonString(w: anytype, s: []const u8) !void {
    for (s) |c| switch (c) {
        '"' => try w.writeAll("\\\""),
        '\\' => try w.writeAll("\\\\"),
        '\n' => try w.writeAll("\\n"),
        '\r' => try w.writeAll("\\r"),
        '\t' => try w.writeAll("\\t"),
        0...8, 11, 12, 14...31 => try w.print("\\u{x:0>4}", .{c}),
        else => try w.writeByte(c),
    };
}

// -- Framing ------------------------------------------------------------------

/// Read one `Content-Length`-framed JSON-RPC message. Returns the owned body
/// bytes, or null on clean EOF. Caller frees.
fn readMessage(allocator: std.mem.Allocator, file: std.fs.File) !?[]u8 {
    var content_length: ?usize = null;
    var line_buf = std.ArrayList(u8){};
    defer line_buf.deinit(allocator);

    // Header block: lines terminated by CRLF, ended by a blank line.
    while (true) {
        line_buf.clearRetainingCapacity();
        const got = try readLine(allocator, file, &line_buf);
        if (!got) return null; // EOF before any header
        const line = std.mem.trimRight(u8, line_buf.items, "\r");
        if (line.len == 0) break; // end of headers
        const prefix = "Content-Length:";
        if (std.ascii.startsWithIgnoreCase(line, prefix)) {
            const v = std.mem.trim(u8, line[prefix.len..], " \t");
            content_length = std.fmt.parseInt(usize, v, 10) catch null;
        }
    }

    const len = content_length orelse return error.MissingContentLength;
    const body = try allocator.alloc(u8, len);
    errdefer allocator.free(body);
    var read: usize = 0;
    while (read < len) {
        const n = try file.read(body[read..]);
        if (n == 0) return error.UnexpectedEof;
        read += n;
    }
    return body;
}

/// Read bytes up to and including the next '\n' into `out` (without the '\n').
/// Returns false on EOF with no bytes read.
fn readLine(allocator: std.mem.Allocator, file: std.fs.File, out: *std.ArrayList(u8)) !bool {
    var one: [1]u8 = undefined;
    var any = false;
    while (true) {
        const n = try file.read(&one);
        if (n == 0) return any; // EOF
        any = true;
        if (one[0] == '\n') return true;
        try out.append(allocator, one[0]);
    }
}
