# Birnam

An Odin reboot of Birnam: a malleable `id` object universe with native compilation, stackful fibers, and source-bearing native images.

This checkout currently contains design documents, not an implemented runtime.

- [Architecture](ARCHITECTURE.md) is the consolidated record of the agreed requirements and directions.
- [First milestone](docs/first-milestone.md) specifies the proposed seed-language scope and acceptance evidence: an Odin interpreter running Birnam-authored PEG.
- [Seed invocation proposal](docs/seed-invocation.md) makes the first milestone's native boundary and dispatch sequence concrete for review. It is a proposal, not an accepted permanent ABI.
- [Dispatch equivalence analysis](docs/dispatch-equivalence.md) examines whether replaceable lookup can replace the proposed extra send/bind layers, with explicit assumptions and a counterexample.
- [Lookup, binding, and sending review](docs/dispatch-design-review.md) is a standalone treatment for external language-design review, comparing plain Id, receiver-aware lookup, and independent bind/send hooks.
- [CBPV compiler core experiment](docs/cbpv-experiment.md) records a direction to try, with invariant constraints, a bounded trial, and adoption/rejection criteria. CBPV is not an adopted architectural requirement.
- [Review notes](Birnam%20Runtime%20Architecture%20%E2%80%94%20Review%20Notes.md) preserve the discussion and decisions.
- [Collector research](Collector%20Research.md) and [image-loading prior art](Image%20Loading%20Prior%20Art.md) distinguish external evidence from implementation proposals.

The earlier [architecture record](Birnam%20Runtime%20Architecture%282%29.md) is retained as discussion history. Use the consolidated architecture for current requirements. Historical sources in `/Users/chazu/dev/zig/birnam` inform syntax and public behavior; they are not build dependencies or an implementation plan. The reboot must not use the previous Rust implementation, compiler, or Lang seed.
