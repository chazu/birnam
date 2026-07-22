//! Shared identity constants for installable Birnam compiled-unit artifacts.

const builtin = @import("builtin");

/// Bumped when the generated dylib ABI or its loader contract changes.
pub const abi = "birnam-artifact-generation-v1";

/// The target spelling embedded in every compiled unit and compared by the
/// inert artifact inspector before any unit initializer or entry point runs.
pub const target = @tagName(builtin.cpu.arch) ++ "-" ++ @tagName(builtin.os.tag);

/// Unique prefix for the length-delimited, pointer-free descriptor embedded in
/// every compiled unit. The minimal inspector reads this record from file bytes;
/// it never has to map the artifact merely to discover its requirements.
pub const descriptor_magic = "BIRNAM-ARTIFACT-DESCRIPTOR-V1";
