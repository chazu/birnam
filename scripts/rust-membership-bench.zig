//! External reporter and threshold gate for the optimized Rust membership bench.

const std = @import("std");

const case_count = 10;
const population_count = 3;

const BenchSample = extern struct {
    live_objects: usize,
    vtables: usize,
    raw_ns_per_call: f64,
    loop_ns_per_call: f64,
    ns_per_call: f64,
};

const BenchReport = extern struct {
    schema: usize,
    iterations: usize,
    repetitions: usize,
    case_count: usize,
    population_count: usize,
    samples: [case_count][population_count]BenchSample,
    scaling_exponents: [case_count]f64,
};

const BenchFn = *const fn (*BenchReport) callconv(.c) c_int;
const case_names = [_][]const u8{
    "objectMembership",
    "vtableOf",
    "vtableOf.method",
    "vtableOf.foreign",
    "vtableOwner",
    "vtableOwner.method",
    "vtableOwner.foreign",
    "isSmallInt",
    "isFloat",
    "isWord",
};

fn checkThresholds(report: *const BenchReport) !void {
    for (report.scaling_exponents, case_names) |exponent, name| {
        if (exponent > 0.35) {
            std.debug.print("rust-membership: {s} scaling exponent {d:.4} exceeds 0.35\n", .{
                name,
                exponent,
            });
            return error.MembershipScalingRegression;
        }
    }

    // Design 46/R0.5 maxima plus the established 10% micro threshold. Only
    // operations with direct Zig equivalents receive an absolute comparison;
    // the new Method/foreign rejection rows are slope-gated here.
    const zig_limits = [_]struct { case: usize, maxima: [3]f64 }{
        .{ .case = 0, .maxima = .{ 1.823 * 1.10, 3.892 * 1.10, 4.946 * 1.10 } },
        .{ .case = 1, .maxima = .{ 1.951 * 1.10, 3.940 * 1.10, 4.987 * 1.10 } },
        .{ .case = 4, .maxima = .{ 4.148 * 1.10, 8.794 * 1.10, 11.338 * 1.10 } },
        .{ .case = 7, .maxima = .{ 2.883 * 1.10, 2.665 * 1.10, 2.715 * 1.10 } },
        .{ .case = 8, .maxima = .{ 2.466 * 1.10, 2.342 * 1.10, 2.774 * 1.10 } },
        .{ .case = 9, .maxima = .{ 2.709 * 1.10, 2.602 * 1.10, 2.828 * 1.10 } },
    };
    for (zig_limits) |limit| {
        for (report.samples[limit.case], limit.maxima, 0..) |sample, maximum, population| {
            if (sample.ns_per_call > maximum) {
                std.debug.print(
                    "rust-membership: {s}[{}] {d:.3} ns exceeds Zig+10% {d:.3} ns\n",
                    .{ case_names[limit.case], population, sample.ns_per_call, maximum },
                );
                return error.MembershipAbsoluteRegression;
            }
        }
    }
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const run_bench = dylib.lookup(BenchFn, "birnam_id_test_membership_bench") orelse
        return error.MissingMembershipBenchHook;

    var report: BenchReport = undefined;
    if (run_bench(&report) != 1) return error.MembershipBenchFailed;
    if (report.schema != 2 or
        report.iterations != 524_288 or
        report.repetitions != 5 or
        report.case_count != case_count or
        report.population_count != population_count)
    {
        return error.WrongMembershipBenchHeader;
    }
    try checkThresholds(&report);

    const stdout = std.fs.File.stdout().deprecatedWriter();
    try stdout.print(
        "{{\"schema\":2,\"floor_identity\":\"rust-id-r3.2\"," ++
            "\"mode\":\"release\",\"iterations\":{},\"repetitions\":{},\"cases\":[",
        .{ report.iterations, report.repetitions },
    );
    for (case_names, 0..) |name, case_index| {
        if (case_index != 0) try stdout.writeByte(',');
        try stdout.print("{{\"name\":\"{s}\",\"samples\":[", .{name});
        for (report.samples[case_index], 0..) |sample, sample_index| {
            if (sample_index != 0) try stdout.writeByte(',');
            try stdout.print(
                "{{\"live_objects\":{},\"vtables\":{},\"raw_ns_per_call\":{d:.3}," ++
                    "\"loop_ns_per_call\":{d:.3},\"ns_per_call\":{d:.3}}}",
                .{
                    sample.live_objects,
                    sample.vtables,
                    sample.raw_ns_per_call,
                    sample.loop_ns_per_call,
                    sample.ns_per_call,
                },
            );
        }
        try stdout.print("],\"scaling_exponent\":{d:.4}}}", .{
            report.scaling_exponents[case_index],
        });
    }
    try stdout.writeAll("]}\n");
}
