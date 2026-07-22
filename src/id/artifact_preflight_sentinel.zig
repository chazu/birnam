//! A real dynamic-library container for the preflight authority test. Its
//! pointer-free descriptor is appended by the test harness. Merely inspecting
//! the file must never load it or call either activation entry point.

const std = @import("std");

export fn module_init(_: *anyopaque, _: ?*anyopaque) callconv(.c) ?*anyopaque {
    std.process.abort();
}

export fn __birnam_unit_init() callconv(.c) void {
    std.process.abort();
}
