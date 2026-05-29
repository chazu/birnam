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

    // -- Hello dylib (Phase 5 smoke-test module) -----------------------------

    const hello_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/hello/hello.zig"),
        .target = target,
        .optimize = optimize,
    });
    const hello_lib = b.addLibrary(.{
        .name = "birnam_hello",
        .root_module = hello_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(hello_lib);

    // -- REPL dylib (Phase 6) -------------------------------------------------

    const repl_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/repl/repl.zig"),
        .target = target,
        .optimize = optimize,
    });
    const repl_lib = b.addLibrary(.{
        .name = "birnam_repl",
        .root_module = repl_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(repl_lib);

    // -- Query dylib (Phase 7) ------------------------------------------------

    const query_mod = b.createModule(.{
        .root_source_file = b.path("src/modules/query/query.zig"),
        .target = target,
        .optimize = optimize,
    });
    const query_lib = b.addLibrary(.{
        .name = "birnam_query",
        .root_module = query_mod,
        .linkage = .dynamic,
    });
    b.installArtifact(query_lib);

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

    const fs_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/fact_store_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const fs_tests = b.addTest(.{ .root_module = fs_test_mod });
    const run_fs_tests = b.addRunArtifact(fs_tests);
    test_step.dependOn(&run_fs_tests.step);

    const boot_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/bootstrap_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const boot_tests = b.addTest(.{ .root_module = boot_test_mod });
    const run_boot_tests = b.addRunArtifact(boot_tests);
    test_step.dependOn(&run_boot_tests.step);

    const str_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/strings_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const str_tests = b.addTest(.{ .root_module = str_test_mod });
    const run_str_tests = b.addRunArtifact(str_tests);
    test_step.dependOn(&run_str_tests.step);

    // Module integration tests depend on the hello dylib being installed.
    const mod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const mod_tests = b.addTest(.{ .root_module = mod_test_mod });
    const run_mod_tests = b.addRunArtifact(mod_tests);
    run_mod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_mod_tests.step);

    const repl_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/repl_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const repl_tests = b.addTest(.{ .root_module = repl_test_mod });
    const run_repl_tests = b.addRunArtifact(repl_tests);
    run_repl_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_repl_tests.step);

    const query_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/query_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const query_tests = b.addTest(.{ .root_module = query_test_mod });
    const run_query_tests = b.addRunArtifact(query_tests);
    test_step.dependOn(&run_query_tests.step);

    const qmod_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/query_module_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const qmod_tests = b.addTest(.{ .root_module = qmod_test_mod });
    const run_qmod_tests = b.addRunArtifact(qmod_tests);
    run_qmod_tests.step.dependOn(b.getInstallStep());
    test_step.dependOn(&run_qmod_tests.step);

    const rule_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/rule_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const rule_tests = b.addTest(.{ .root_module = rule_test_mod });
    const run_rule_tests = b.addRunArtifact(rule_tests);
    test_step.dependOn(&run_rule_tests.step);

    const agg_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/aggregate_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const agg_tests = b.addTest(.{ .root_module = agg_test_mod });
    const run_agg_tests = b.addRunArtifact(agg_tests);
    test_step.dependOn(&run_agg_tests.step);

    const sq_test_mod = b.createModule(.{
        .root_source_file = b.path("src/kernel/saved_query_test.zig"),
        .target = target,
        .optimize = optimize,
    });
    const sq_tests = b.addTest(.{ .root_module = sq_test_mod });
    const run_sq_tests = b.addRunArtifact(sq_tests);
    test_step.dependOn(&run_sq_tests.step);

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
