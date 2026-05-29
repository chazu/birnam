//! Tiny line parser for Phase 6 REPL.
//!
//! Grammar:
//!   line       = directive | unary | keyword | empty
//!   directive  = "quit" | "exit"
//!   unary      = ident
//!   keyword    = ident ":" arg
//!   arg        = integer | string | ident
//!   integer    = [-]?[0-9]+
//!   string     = "\"" .* "\""
//!   ident      = [A-Za-z_][A-Za-z_0-9]*
//!
//! Throwaway. Replaced by the real reader when Phase 10 lands.

const std = @import("std");

pub const Form = enum { empty, quit, unary, keyword };

pub const Arg = union(enum) {
    integer: i64,
    string: []const u8,
    ident: []const u8,
};

pub const Parsed = struct {
    form: Form,
    selector_text: []const u8 = "", // full selector incl. trailing ":" for keyword
    arg: ?Arg = null,
};

pub fn parse(line: []const u8) Parsed {
    const trimmed = std.mem.trim(u8, line, " \t\r\n");
    if (trimmed.len == 0) return .{ .form = .empty };
    if (std.mem.eql(u8, trimmed, "quit") or std.mem.eql(u8, trimmed, "exit")) {
        return .{ .form = .quit };
    }
    const colon_pos = std.mem.indexOfScalar(u8, trimmed, ':');
    if (colon_pos == null) {
        return .{ .form = .unary, .selector_text = trimmed };
    }
    const sel_end = colon_pos.? + 1; // include the colon
    const sel = trimmed[0..sel_end];
    const rest = std.mem.trim(u8, trimmed[sel_end..], " \t");
    if (rest.len == 0) {
        return .{ .form = .keyword, .selector_text = sel, .arg = null };
    }
    const arg = parseArg(rest);
    return .{ .form = .keyword, .selector_text = sel, .arg = arg };
}

fn parseArg(text: []const u8) ?Arg {
    if (text.len == 0) return null;
    if (text[0] == '"' and text[text.len - 1] == '"' and text.len >= 2) {
        return .{ .string = text[1 .. text.len - 1] };
    }
    if (std.fmt.parseInt(i64, text, 10)) |n| {
        return .{ .integer = n };
    } else |_| {}
    return .{ .ident = text };
}
