# Local project value helpers

This Rust library contains concrete project value contracts, policy input/output
projections over the existing rooted Id Scope, path/identity validation, digest
helpers and the existing bounded Git-command/cache-validation utility. It has no
native module descriptor, project registry, borrowed owner payload or mutable
runtime state. Native ProjectBuild and HostProject communicate through Id; they
do not import each other's implementations.

Generated publication source and staging algorithms remain private to ProjectBuild.
Discovery/source walks remain in HostProject. This helper library adds no runtime
service or second policy implementation.
