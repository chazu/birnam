# Rust-native runtime

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Birnam's runtime and build authority are Rust-only.

The Cargo workspace owns the `IdApi` floor, every native dynamic rung, the
host, project workflow, image recovery, and the compiler bridge. The installer
builds that workspace from source, compiles the authored `.birnam` bootstrap
modules with the Rust host, and publishes one executable-relative closure.

The runtime load order is:

```text
libid -> Artifact -> Store -> Peg -> Lang -> Tcc -> Codegen -> Prelude
      -> Collection -> Error -> Isolate -> Relational -> Alien -> Image
```

`src/id/*.birnam` and `modules/*.birnam` remain authored language sources. They
are not alternate native implementations: the Rust host compiles them into
self-describing modules during installation.

Verification is provided by `cargo test --workspace --locked --offline`,
`scripts/rust-driver-native-smoke.sh`, and
`scripts/rust-driver-cold-archive-smoke.sh`. No Zig compiler, build file,
oracle, rollback prefix, or cross-floor fixture participates in the workflow.
