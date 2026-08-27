//! Fatal policy for failures reached through exported C ABI entrypoints.

/// Terminate the process for an invariant failure at an exported boundary.
///
/// This deliberately cannot return or unwind. It matches the native
/// oracle's fatal treatment of corrupt runtime invariants; recoverable language
/// errors must instead be represented as Birnam objects before this boundary.
#[cold]
pub(crate) fn fatal_invariant(message: &str) -> ! {
    eprintln!("birnam-id invariant failed: {message}");
    std::process::abort()
}

/// Terminate after crossing an explicitly configured compatibility ceiling.
/// The frozen IdApi intern operation has no recoverable error carrier, so a
/// deterministic resource diagnostic is safer than returning an invalid Oop.
#[cold]
pub(crate) fn fatal_resource_limit(message: &str) -> ! {
    eprintln!("birnam-id resource limit exceeded: {message}");
    std::process::abort()
}

/// Enforce an exported-boundary invariant without creating an unwind path.
pub(crate) fn require_invariant(condition: bool, message: &str) {
    if !condition {
        fatal_invariant(message);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn successful_boundary_checks_do_not_abort() {
        super::require_invariant(true, "unreachable test failure");
    }
}
