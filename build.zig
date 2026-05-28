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
    //
    // No kernel sources yet. Test step is a no-op until Phase 1 (HAMT spike)
    // introduces something testable.

    const test_step = b.step("test", "Run unit tests");
    _ = test_step;
}
