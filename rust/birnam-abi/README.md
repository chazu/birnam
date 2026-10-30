# Native boundary and format evidence

The [invariants](../../INVARIANTS.md) define the message-passing requirements.
`birnam-abi` supplies the native calling convention, byte-transfer helpers and
the stateless OS loader used by Siphon and Id. These implementation helpers do
not introduce borrowing or ownership rules into Birnam.

The old module tables, twenty IdApi schema manifests, their C headers and the
central production-construction catalog are deleted. Do not restore their
layouts, factory catalogs or fingerprint checks to repair callers. Production Rust modules use the message protocol; the aggregate Runtime is gone.

Retained evidence:

- [Oop-only Id fixtures](../../tests/fixtures/oop-id-v2/) and the
  [independent continuation caller](../../tests/fixtures/oop-continuation/)
  exercise the current message calling convention, custom lookup, bootstrap
  and native callback boundaries. `scripts/oop-id-slice-test.sh` and
  `scripts/id-continuation-test.py` run these checks.
- [Stackless/recovery formats](../../tests/fixtures/stackless-s2/README.md) retain registration,
  frame, generation and image fixtures. Their old registration-envelope header
  records the frozen format history; it is not a current runtime calling convention.
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
