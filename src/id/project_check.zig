//! Read-only compile checking for resolved Birnam projects.
//!
//! The resolver has already validated manifests and the module graph. This
//! adapter asks the ordinary compile-only backend to materialize each module in
//! a private temporary directory, records compiler diagnostics, then removes
//! the directory. It never evaluates project source or publishes an artifact.

const std = @import("std");
const idapi = @import("idapi.zig");
const project = @import("project.zig");

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

pub const schema = "birnam-check-v1";

pub const Diagnostic = struct {
    kind: []const u8,
    message: []const u8,
    detail: []const u8,
    path: []const u8,
    line: ?usize,
    column: ?usize,
};

pub const Result = struct {
    arena: std.heap.ArenaAllocator,
    project_root: []const u8,
    manifest_path: []const u8,
    resolution_digest: []const u8,
    checked_modules: usize,
    diagnostics: []const Diagnostic,

    pub fn deinit(self: *Result) void {
        self.arena.deinit();
    }

    pub fn ok(self: *const Result) bool {
        return self.diagnostics.len == 0;
    }
};

fn compilerText(api: *IdApi, arena: std.mem.Allocator) []const u8 {
    const codegen = api.import("Codegen") orelse return "compiler did not provide a diagnostic";
    const value = api.send(codegen, api.intern("lastError"), &.{}, 0);
    if (value == api.nilObj or api.vtableOf(value) != api.string_vt)
        return "compiler did not provide a diagnostic";
    var length: usize = 0;
    const bytes = api.stringBytes(value, &length);
    return arena.dupe(u8, bytes[0..length]) catch "compiler diagnostic allocation failed";
}

fn parsedDiagnostic(arena: std.mem.Allocator, module: project.Module, detail: []const u8) Diagnostic {
    const first_line = if (std.mem.indexOfScalar(u8, detail, '\n')) |end| detail[0..end] else detail;
    var parts = std.mem.splitScalar(u8, first_line, ':');
    const line_text = parts.next() orelse "";
    const column_text = parts.next() orelse "";
    const line = std.fmt.parseUnsigned(usize, std.mem.trim(u8, line_text, " \t"), 10) catch null;
    const column = std.fmt.parseUnsigned(usize, std.mem.trim(u8, column_text, " \t"), 10) catch null;
    const message = if (line != null and column != null)
        std.mem.trim(u8, parts.rest(), " \t")
    else
        first_line;
    return .{
        .kind = "compileFailed",
        .message = arena.dupe(u8, message) catch "compile failed",
        .detail = detail,
        .path = module.path,
        .line = line,
        .column = column,
    };
}

pub fn check(api: *IdApi, allocator: std.mem.Allocator, resolution: *const project.Resolution) !Result {
    var arena = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const a = arena.allocator();
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var token_value: u64 = undefined;
    std.crypto.random.bytes(std.mem.asBytes(&token_value));
    const temp_base = std.posix.getenv("TMPDIR") orelse "/tmp";
    const temp_root = try std.fs.path.join(a, &.{ temp_base, try std.fmt.allocPrint(a, "birnam-check-{x}", .{token_value}) });
    try std.fs.cwd().makePath(temp_root);
    defer std.fs.cwd().deleteTree(temp_root) catch {};

    var diagnostics: std.ArrayList(Diagnostic) = .{};
    for (resolution.modules) |module| {
        const filename = try std.fmt.allocPrint(a, "libbirnam_{s}.dylib", .{module.name});
        const output_path = try std.fs.path.join(a, &.{ temp_root, filename });
        const compiled = api.send(codegen, api.intern("compile:output:root:"), &[_]Oop{
            api.mkString(module.source.ptr, module.source.len),
            api.mkString(output_path.ptr, output_path.len),
            api.mkString(module.project_root.ptr, module.project_root.len),
        }, 3);
        if (compiled == api.nilObj) {
            const detail = compilerText(api, a);
            try diagnostics.append(a, parsedDiagnostic(a, module, detail));
        }
    }

    return .{
        .arena = arena,
        .project_root = try a.dupe(u8, resolution.project_root),
        .manifest_path = try a.dupe(u8, resolution.manifest_path),
        .resolution_digest = try a.dupe(u8, resolution.digest),
        .checked_modules = resolution.modules.len,
        .diagnostics = try diagnostics.toOwnedSlice(a),
    };
}

fn writeJsonString(writer: anytype, value: []const u8) !void {
    try writer.writeByte('"');
    for (value) |byte| switch (byte) {
        '"' => try writer.writeAll("\\\""),
        '\\' => try writer.writeAll("\\\\"),
        '\n' => try writer.writeAll("\\n"),
        '\r' => try writer.writeAll("\\r"),
        '\t' => try writer.writeAll("\\t"),
        0...8, 11...12, 14...0x1f => try writer.print("\\u00{x:0>2}", .{byte}),
        else => try writer.writeByte(byte),
    };
    try writer.writeByte('"');
}

pub fn writeJson(result: *const Result, writer: anytype) !void {
    try writer.writeAll("{\"schema\":");
    try writeJsonString(writer, schema);
    try writer.print(",\"ok\":{},\"projectRoot\":", .{result.ok()});
    try writeJsonString(writer, result.project_root);
    try writer.writeAll(",\"manifest\":");
    try writeJsonString(writer, result.manifest_path);
    try writer.writeAll(",\"resolutionDigest\":");
    try writeJsonString(writer, result.resolution_digest);
    try writer.print(",\"checkedModules\":{d},\"diagnostics\":[", .{result.checked_modules});
    for (result.diagnostics, 0..) |diagnostic, i| {
        if (i != 0) try writer.writeByte(',');
        try writer.writeAll("{\"kind\":");
        try writeJsonString(writer, diagnostic.kind);
        try writer.writeAll(",\"message\":");
        try writeJsonString(writer, diagnostic.message);
        try writer.writeAll(",\"detail\":");
        try writeJsonString(writer, diagnostic.detail);
        try writer.writeAll(",\"path\":");
        try writeJsonString(writer, diagnostic.path);
        if (diagnostic.line) |line| try writer.print(",\"line\":{d}", .{line});
        if (diagnostic.column) |column| try writer.print(",\"column\":{d}", .{column});
        try writer.writeByte('}');
    }
    try writer.writeAll("]}\n");
}

pub fn writeFailureJson(diagnostic: project.Diagnostic, writer: anytype) !void {
    try writer.writeAll("{\"schema\":");
    try writeJsonString(writer, schema);
    try writer.writeAll(",\"ok\":false,\"projectRoot\":null,\"manifest\":null,\"resolutionDigest\":null,\"checkedModules\":0,\"diagnostics\":[{\"kind\":");
    try writeJsonString(writer, diagnostic.kind);
    try writer.writeAll(",\"message\":");
    try writeJsonString(writer, diagnostic.message);
    try writer.writeAll(",\"detail\":");
    try writeJsonString(writer, diagnostic.message);
    try writer.writeAll(",\"path\":null}]}\n");
}

pub fn writeHuman(result: *const Result, writer: anytype) !void {
    if (result.ok()) {
        try writer.print("checked {d} module{s}; no compile diagnostics\n", .{
            result.checked_modules,
            if (result.checked_modules == 1) "" else "s",
        });
        return;
    }
    for (result.diagnostics) |diagnostic| {
        if (diagnostic.line) |line| {
            try writer.print("{s}:{d}:{d}: {s}: {s}\n", .{
                diagnostic.path,
                line,
                diagnostic.column orelse 1,
                diagnostic.kind,
                diagnostic.message,
            });
        } else {
            try writer.print("{s}: {s}: {s}\n", .{ diagnostic.path, diagnostic.kind, diagnostic.message });
        }
    }
}
