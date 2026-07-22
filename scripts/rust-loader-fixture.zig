//! Tiny reloadable module used only by the R3.1 Rust loader gate.

var state_value: usize = 0;

export fn birnam_fixture_module_init(_: *anyopaque, incoming: ?*anyopaque) callconv(.c) ?*anyopaque {
    if (incoming == null) {
        state_value = 1;
        return @ptrCast(&state_value);
    }
    if (incoming != @as(?*anyopaque, @ptrCast(&state_value))) return null;
    state_value += 1;
    return incoming;
}

export fn birnam_fixture_state_value(state: ?*anyopaque) callconv(.c) usize {
    if (state != @as(?*anyopaque, @ptrCast(&state_value))) return 0;
    return state_value;
}
