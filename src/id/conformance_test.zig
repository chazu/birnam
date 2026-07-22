//! External IdApi conformance shared by every floor implementation.
//! This root must not import id.zig.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");

test "conformance harness selects one identified floor and initializes it once" {
    const first = floor.api();
    const second = floor.api();

    try std.testing.expect(first == second);
    try std.testing.expectEqual(@as(usize, 1), floor.initCallCount());
    try std.testing.expect(floor.retainedHandle() != null);
    try std.testing.expect(first.import("Object") != null);
    try std.testing.expect(first.import("Integer") != null);
    try std.testing.expect(first.import("String") != null);
}
