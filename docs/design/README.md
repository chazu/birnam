# Detailed references and proposals

[INVARIANTS.md](../../INVARIANTS.md) is the design authority.
[ARCHITECTURE.md](../../ARCHITECTURE.md) maps the current implementation, and the
[runtime reference](../contracts/runtime.md) collects continuing behavior from
the completed refactors. This directory is not an automatic work queue.

## Message and format references

- [Optional Store and Relational](optional-store-and-relational.md), with the
  current [Store](../../rust/birnam-store/README.md) and
  [Relational](../../rust/birnam-relational/README.md) message references.
- [Project manifests](51-project-manifests-and-dependency-resolution.md),
  [project commands](52-project-check-build-and-run.md),
  [language tools](53-project-aware-language-tools.md),
  [relocatable bundles](54-relocatable-bundles.md),
  [embedded files](56-embedded-files.md),
  [project policy](57-project-policy-in-birnam.md), and
  [self-describing artifacts](65-self-describing-project-publications.md).
- [Artifact framing](60i-stackless-artifact-byte-grammar.md),
  [PortableProgram/image-v7](60l-portable-program-and-image-v7-byte-grammar.md),
  [CodeObject messages](60m-portable-code-object-protocol.md),
  [native resumable encoding](60n-portable-resumable-native-carrier.md), and
  [image-v8 identity](60o-image-v8-identity-preservation.md).
  These constrain the formats they describe, not native runtime architecture.
- [PEG behavior](63-peg-behavior-freeze.md) and
  [pipeline combinators](64-pipeline-threading-and-combinators.md).
- [Concurrency design](80-concurrency-domains-and-structured-tasks.md) and
  [cleanup transitions](80n-cancellation-to-cleanup-transition.md).
  Start with the [concurrency guide](../guide/07-concurrency.md) for shipped behavior;
  the design also discusses future capabilities.
- [Discarded-clone](90-discarded-clone.md) and
  [numeric-predicate](90-numeric-predicate.md) optimization behavior.

## Proposals and development direction

[Map deletion](70-persistent-map-deletion.md) is a proposal.
[Scalable ordering](73-scalable-ordering-contract.md) records agreed semantics
and outstanding work. [A Birnam implementation](81-toward-a-birnam-implementation.md)
is the longer-term objective, subject to independent source bootstrap.
These documents do not authorize unrelated runtime machinery.

## Implementation notes and evidence

Other notes in this directory describe specific facilities, algorithms or dated
checks. They are subordinate to the invariants; historical test results do not
prove current integration. [Benchmark methodology](46-benchmarks.md),
[application performance](application-performance.md) and
[Logue performance](logue-performance.md) describe measurement scope.

Completed migration sequences, retired designs and superseded investigations
are in the [archive](../archive/README.md), including the
[October consolidation](../archive/2026-10-runtime-refactors/README.md).
