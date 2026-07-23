//! Child-process proof that a singleton-owning native rung cannot initialize a
//! second implementation in one process.

const floor_adapter = @import("floor_adapter_test.zig");
const rung_adapter = @import("rung_adapter_test.zig");

pub fn main() void {
    const api = floor_adapter.api();
    _ = rung_adapter.loadSelected(api, "artifact-inspector");

    // The adapter must reject this before touching either deliberately invalid
    // path. The child-process harness requires SIGABRT and this diagnostic.
    _ = rung_adapter.load(api, .{
        .rung = "artifact-inspector",
        .implementation = .rust,
        .canonical_path = "/must-not-open/libbirnam_id_artifact.dylib",
        .explicit_path = "/must-not-open/libbirnam_id_artifact_rust.dylib",
    });
}
