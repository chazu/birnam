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
}
