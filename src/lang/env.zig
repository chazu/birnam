const std = @import("std");

pub const Oop = ?*anyopaque;

pub const NonLocalReturn = error{NonLocalReturn};

pub const Env = struct {
    bindings: std.StringHashMap(Oop),
    parent: ?*Env,
    self_ref: Oop,
    /// For non-local return: set by method frame, read by evaluator.
    return_value: Oop,
    /// If true, this is a method frame that catches non-local returns.
    is_method_frame: bool,

    pub fn init(allocator: std.mem.Allocator, parent: ?*Env) Env {
        return .{
            .bindings = std.StringHashMap(Oop).init(allocator),
            .parent = parent,
            .self_ref = if (parent) |p| p.self_ref else null,
            .return_value = null,
            .is_method_frame = false,
        };
    }

    pub fn deinit(self: *Env) void {
        self.bindings.deinit();
    }

    pub fn define(self: *Env, name: []const u8, value: Oop) void {
        self.bindings.put(name, value) catch {};
    }

    pub fn lookup(self: *const Env, name: []const u8) ?Oop {
        if (self.bindings.get(name)) |v| return v;
        if (self.parent) |p| return p.lookup(name);
        return null;
    }

    pub fn set(self: *Env, name: []const u8, value: Oop) bool {
        if (self.bindings.getPtr(name)) |ptr| {
            ptr.* = value;
            return true;
        }
        if (self.parent) |p| return p.set(name, value);
        return false;
    }
};
