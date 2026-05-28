const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -- Shell executable ----------------------------------------------------

    const shell_mod = b.createModule(.{
        .root_source_file = b.path("src/shell.zig"),
        .target = target,
        .optimize = optimize,
    });

    const shell = b.addExecutable(.{
        .name = "birnam",
        .root_module = shell_mod,
    });
    b.installArtifact(shell);

    // -- Tests ---------------------------------------------------------------

    const test_step = b.step("test", "Run unit tests");

    const hamt_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/hamt_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const hamt_tests = b.addTest(.{ .root_module = hamt_test_mod });
    const run_hamt_tests = b.addRunArtifact(hamt_tests);
    test_step.dependOn(&run_hamt_tests.step);

    const rrb_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/rrb_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const rrb_tests = b.addTest(.{ .root_module = rrb_test_mod });
    const run_rrb_tests = b.addRunArtifact(rrb_tests);
    test_step.dependOn(&run_rrb_tests.step);

    const ref_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/ref_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ref_tests = b.addTest(.{ .root_module = ref_test_mod });
    const run_ref_tests = b.addRunArtifact(ref_tests);
    test_step.dependOn(&run_ref_tests.step);

    const ot_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/object_table_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const ot_tests = b.addTest(.{ .root_module = ot_test_mod });
    const run_ot_tests = b.addRunArtifact(ot_tests);
    test_step.dependOn(&run_ot_tests.step);

    // -- Benchmarks (ReleaseFast, separate step) ------------------------------

    const bench_step = b.step("bench", "Run microbenchmarks (ReleaseFast)");

    const hamt_for_bench = b.createModule(.{
        .root_source_file = b.path("src/kernel/hamt.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const hamt_bench_mod = b.createModule(.{
        .root_source_file = b.path("bench/hamt_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    hamt_bench_mod.addImport("hamt", hamt_for_bench);
    const hamt_bench_exe = b.addExecutable(.{
        .name = "hamt_bench",
        .root_module = hamt_bench_mod,
    });
    const run_hamt_bench = b.addRunArtifact(hamt_bench_exe);
    bench_step.dependOn(&run_hamt_bench.step);

    const rrb_for_bench = b.createModule(.{
        .root_source_file = b.path("src/kernel/rrb.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    const rrb_bench_mod = b.createModule(.{
        .root_source_file = b.path("bench/rrb_bench.zig"),
        .target = target,
        .optimize = .ReleaseFast,
    });
    rrb_bench_mod.addImport("rrb", rrb_for_bench);
    const rrb_bench_exe = b.addExecutable(.{
        .name = "rrb_bench",
        .root_module = rrb_bench_mod,
    });
    const run_rrb_bench = b.addRunArtifact(rrb_bench_exe);
    bench_step.dependOn(&run_rrb_bench.step);
}
