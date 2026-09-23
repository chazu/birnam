# Native boundary and format evidence

[Design 90](../docs/design/90-functional-modules.md) is the current
refactoring design: opaque objects and Id messages are the common module boundary.
`birnam-abi` contains only that protocol, borrowed-buffer helpers and the
stateless OS loader used by Siphon and Id.

The old module tables, twenty IdApi schema manifests, their C headers and the
central production-construction catalog are deleted. Do not restore their
layouts, factory catalogs or fingerprint checks to repair callers. The complete
language is intentionally unbuildable during this deletion-first transition.

Retained evidence:

- [Id protocol](fixtures/id-protocol-v1/README.md), its C header and independent
  C-entry round trip exercise the common native message boundary.
- [Stackless/recovery formats](fixtures/stackless-s2/README.md) retain registration,
  frame, generation and image fixtures, including the registration-envelope C
  header and layout check. These describe recovery formats, not a module API catalog.
- [Portable Code formats](fixtures/portable-code-v1/README.md) retain independent
  binary readers and fixtures for programs, bindings and images.
- `rust-*-dylib-v1.json` inventories describe exported symbols. Publishers deleted
  in Design 84 are removed from those inventories. Retained owner implementations
  still require message entrypoints before they can be loaded in production.

The other module-specific documents and fixture READMEs in this directory are
historical descriptions of the removed architecture, not current implementation
requirements. References to removed headers and manifests can be read from
[the last checkpoint before this cleanup](https://github.com/chazu/birnam/tree/4f76ba42/abi)
or with `git show 4f76ba42:abi/PATH`. Their layouts must not become a parallel
protocol again. Deoptimization/concurrency semantic fixtures remain in the tree;
fixtures that used the deleted headers need new message-based harnesses.
