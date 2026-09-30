# Native boundary and format evidence

[Design 90](../../docs/design/90-functional-modules.md) is the current
refactoring design: opaque objects and Id messages are the common module boundary.
`birnam-abi` contains only that protocol, borrowed-buffer helpers and the
stateless OS loader used by Siphon and Id.

The old module tables, twenty IdApi schema manifests, their C headers and the
central production-construction catalog are deleted. Do not restore their
layouts, factory catalogs or fingerprint checks to repair callers. Production Rust modules use the message protocol; the aggregate Runtime is gone.

Retained evidence:

- [Id protocol](tests/fixtures/id-protocol-v1/README.md), its C header and independent
  C-entry round trip exercise the common native message boundary.
- [Stackless/recovery formats](../../tests/fixtures/stackless-s2/README.md) retain registration,
  frame, generation and image fixtures, including the registration-envelope C
  header and layout check. These describe recovery formats, not a module API catalog.
- [Portable Code formats](../../tests/fixtures/portable-code-v1/README.md) retain independent
  binary readers and fixtures for programs, bindings and images.
- Each native crate's `exports.json` inventories its exported symbols. Retired
  publishers are absent from those inventories. The native-message gate verifies
  each module's descriptor exports and dependencies.

The retired module-table specifications, IdApi fixture directories and old binary
clients are preserved only in [Git history](https://github.com/chazu/birnam/tree/cffba722/abi).
Their layouts are not current interfaces. Older semantic tests must either use
the production message boundary or be decommissioned after their useful coverage
has been accounted for.
