//! Dependency-free shell for the prospective Rust libid implementation.
//!
//! R1.1 establishes only the pinned, offline-buildable workspace. The cdylib
//! identity export and panic policy belong to later R1 checkpoints.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_shell_builds() {
        assert_eq!(core::mem::size_of::<usize>(), 8);
    }
}
