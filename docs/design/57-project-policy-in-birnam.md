# 57 — Project Policy in Birnam

> **Status:** First vertical slice implemented, 2026-07-16. Resolution,
> dependency-aware artifact invalidation, artifact-record shape, and publication
> authorization now execute in `src/id/project-policy.birnam`.

This slice advances the self-hosting goal without moving effectful operations
into the image. The host remains responsible for filesystem traversal and
source/CST observation, hashing, compiler and wrapped-process launch, file
copying, TOML serialization, and the final atomic rename. Birnam owns the
decisions made from those observations.

## Boundary

The project adapter produces a value-shaped snapshot containing:

- discovered projects, manifests, source declarations, and dependency origins;
- resolved module metadata, ordered source parts, source/revision digests;
- the prior artifact record with on-disk checksums observed by the host.

`ProjectPolicy` consumes that snapshot and returns values. It owns provider
selection, dependency closure and diagnostics, dependency-first ordering, reuse
versus compile decisions, dirty-dependency propagation, the current artifact
record schema/fields, and the publish decision.

The seam is intentionally narrow. A policy call is one in-process message over
an already collected value; it does not add filesystem traversal or process
launch overhead to the build hot path. The host still performs the expensive
effects only for the actions Birnam selected.

## Artifact rules

An artifact is reusable only when the policy finds a compatible prior record,
matching ordered source metadata and dependency metadata, an existing artifact
whose observed checksum equals its recorded checksum, and no dirty dependency.
A compile decision marks the module dirty in dependency-first order, so later
dependents compile while unrelated modules remain reusable.

The policy returns a record-shaped value after the host has materialized
artifacts. Zig serializes that value to `artifacts.toml`; it does not choose the
schema, ABI field, artifact rows, or native-row shape. Publication is a second
policy call after staging and record creation; only then does the host perform
the rename transaction.

## Tests and next seam

`modules/project-policy-test.birnam` covers resolution diagnostics, clean reuse,
dependency invalidation, record shape, and publication rejection/approval.
The project smoke test continues to cover source-part changes, tamper
invalidation, wrapped libraries, run modes, and failure preservation.

The next worthwhile self-hosting slice is policy for install/run artifact
selection and native-record validation, keeping their filesystem copies and
runtime loading as adapters. Avoid moving hashing, compiler invocation, or
atomic rename into Birnam merely for symmetry: those are effect boundaries, not
project policy.
