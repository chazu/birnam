//! ReleaseSafe performance probe for one selected artifact-inspector rung.
//!
//! The timer starts after both dylibs are loaded and after one warm-up
//! inspection. Each measured operation reads and validates the same inert
//! descriptor through the public Artifact protocol.

const std = @import("std");
const idapi = @import("idapi.zig");
const floor = @import("floor_adapter_test.zig");
const rung = @import("rung_adapter_test.zig");

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const path = args.next() orelse return error.MissingArtifactPath;
    const iterations_text = args.next() orelse "5000";
    if (args.next() != null) return error.TooManyArguments;
    const iterations = try std.fmt.parseUnsigned(usize, iterations_text, 10);
    if (iterations == 0) return error.InvalidIterationCount;

    const api = rung.loadSelected(floor.api(), "artifact-inspector");
    const artifact = api.import("Artifact") orelse return error.ArtifactUnavailable;
    const inspect = api.intern("inspect:");
    const identity = api.intern("benchmark-candidate");
    const path_value = api.mkString(path.ptr, path.len);

    const warm = api.send(artifact, inspect, &.{path_value}, 1);
    if (warm == api.nilObj) return error.WarmupInspectionFailed;

    var timer = try std.time.Timer.start();
    var last: idapi.Oop = api.nilObj;
    for (0..iterations) |_| {
        last = api.send(artifact, inspect, &.{path_value}, 1);
        if (last == api.nilObj) return error.InspectionFailed;
    }
    const elapsed_ns = timer.read();
    if (api.send(last, api.intern("identity"), &.{}, 0) != identity)
        return error.IdentityMismatch;

    var output_buffer: [512]u8 = undefined;
    var stdout = std.fs.File.stdout().writer(&output_buffer);
    try stdout.interface.print(
        "{{\"schema\":1,\"case\":\"artifact.inspect\",\"iterations\":{d},\"elapsed_ns\":{d},\"ns_per_inspection\":{d}}}\n",
        .{ iterations, elapsed_ns, elapsed_ns / iterations },
    );
    try stdout.interface.flush();
}
