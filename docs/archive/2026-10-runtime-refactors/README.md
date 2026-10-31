# Completed runtime refactors and historical investigations

Archived 2026-10-30 while consolidating the language requirements. These documents
preserve earlier reasoning, experiments, measurements and migration sequences.
Their words “current,” “required,” “pending,” and “remaining” describe their
recorded period. They are not architectural authority or instructions to restore
retired machinery.

Use [INVARIANTS.md](../../../INVARIANTS.md) for requirements,
[ARCHITECTURE.md](../../../ARCHITECTURE.md) for the current implementation,
[the runtime reference](../../contracts/runtime.md) for continuing Method,
editing, execution and image behavior, and [byte messages](../../contracts/bytes.md)
for current bulk transfer behavior. Versioned formats remain in the
[design index](../../design/README.md).

The original text is preserved apart from this archive notice and relative-link
repairs. Measurements retain their original scope; measurement data files were
not rewritten. The table records the former paths so old references are traceable.

Additional snapshots preserve active documents before the subsequent 2026-10-30
documentation repair. Their original paths now contain rewritten current guidance:

- [Birnam implementation roadmap](birnam-implementation-roadmap-20261030.md),
  including the old extraction instructions and dated progress narrative.
- [NativeBackend reference](native-backend-reference-20261030.md), including
  removed API descriptions and obsolete implementation details.

| Former path | Archived document |
| --- | --- |
| `docs/design/45-compiled-units.md` | [Compiled Units Are Primary ](45-compiled-units.md) |
| `docs/design/47-compiled-unit-lifecycle.md` | [47 — CompiledUnit Lifecycle: Staged Dispatch Generations ](47-compiled-unit-lifecycle.md) |
| `docs/design/60-stackless-native-execution.md` | [60 — Stackless Native Execution ](60-stackless-native-execution.md) |
| `docs/design/60a-stackless-execution-semantics.md` | [60A — Stackless Execution Semantics ](60a-stackless-execution-semantics.md) |
| `docs/design/60b-stackless-machine-and-frames.md` | [60B — Stackless Machine, Frames, and GC ](60b-stackless-machine-and-frames.md) |
| `docs/design/60c-stepmethod-calling-abi-options.md` | [60C — The StepMethod Calling Contract ](60c-stepmethod-calling-abi-options.md) |
| `docs/design/60d-stackless-concurrency-fit.md` | [60D — Stackless Concurrency Fit and Boundaries ](60d-stackless-concurrency-fit.md) |
| `docs/design/60e-resumable-registration-and-entry.md` | [60E — Resumable Registration and the Entry Seam ](60e-resumable-registration-and-entry.md) |
| `docs/design/60f-resumable-compiler-lowering.md` | [60F — Resumable Compiler Lowering ](60f-resumable-compiler-lowering.md) |
| `docs/design/60h-stackless-production-acceptance-and-rollout.md` | [60H — Stackless Production Acceptance and Rollout ](60h-stackless-production-acceptance-and-rollout.md) |
| `docs/design/60j-runtime-identity-and-history-compaction.md` | [60J — Runtime Identity and History Compaction ](60j-runtime-identity-and-history-compaction.md) |
| `docs/design/60k-portable-codeobjects.md` | [60K — Reified Portable CodeObjects ](60k-portable-codeobjects.md) |
| `docs/design/60p-v2-control-completion.md` | [60P — Complete V2 control and retire Frame layout V1 ](60p-v2-control-completion.md) |
| `docs/design/62-rust-native-rungs-migration-plan.md` | [Rust-native runtime ](62-rust-native-rungs-migration-plan.md) |
| `docs/design/75-code-quality-and-responsibility.md` | [75 — Code quality and responsibility cleanup ](75-code-quality-and-responsibility.md) |
| `docs/design/90-functional-modules.md` | [90 — Functional modules, minimal dependencies ](90-functional-modules.md) |
| `docs/design/90-numeric-chain-review.md` | [Design 90 — Numeric chains and loop reuse reassessment ](90-numeric-chain-review.md) |
| `docs/design/90-self-optimization-review.md` | [Design 90 — Self optimization review and allocation-policy correction ](90-self-optimization-review.md) |
| `docs/design/abi-overhead-investigation.md` | [ABI overhead: measured owner-message costs ](abi-overhead-investigation.md) |
| `docs/design/addition-send-breakdown.md` | [Remaining sends in Integer addition ](addition-send-breakdown.md) |
| `docs/design/addition-send-investigations.md` | [Remaining addition-send investigations ](addition-send-investigations.md) |
| `docs/design/alien-recipes-migration-performance.md` | [Alien recipe ownership: qualification and accepted startup cost ](alien-recipes-migration-performance.md) |
| `docs/design/application-cpu-profile.md` | [Application CPU profile — 2026-10-11 ](application-cpu-profile.md) |
| `docs/design/authored-access-costs.md` | [Authored access: runtime bookkeeping costs ](authored-access-costs.md) |
| `docs/design/basic-live-image-loading.md` | [Basic live image loading without automatic rollback ](basic-live-image-loading.md) |
| `docs/design/boolean-selection-simplification.md` | [Boolean selection without comparison boxes ](boolean-selection-simplification.md) |
| `docs/design/bulk-capture-handoff.md` | [Bulk capture handoff ](bulk-capture-handoff.md) |
| `docs/design/candidate-policy-migration.md` | [Candidate policy migration ](candidate-policy-migration.md) |
| `docs/design/capture-access-simplification.md` | [Capture access simplification ](capture-access-simplification.md) |
| `docs/design/capture-receiver-query.md` | [Query capture borrowing on the receiver ](capture-receiver-query.md) |
| `docs/design/closure-construction-simplification.md` | [Closure construction simplification ](closure-construction-simplification.md) |
| `docs/design/closure-machinery-necessity.md` | [Closure machinery: purposes, necessary behavior, and excess guarantees ](closure-machinery-necessity.md) |
| `docs/design/collection-search.md` | [First-match collection search ](collection-search.md) |
| `docs/design/compiler-metadata-ownership.md` | [Compiler metadata ownership: inspection before durable state ](compiler-metadata-ownership.md) |
| `docs/design/coordination-cleanup.md` | [Coordination cleanup ](coordination-cleanup.md) |
| `docs/design/execution-name-access.md` | [Execution selector access — 2026-10-11 ](execution-name-access.md) |
| `docs/design/execution-region-semantics.md` | [Reusable behavioral proofs and conditional send bounds ](execution-region-semantics.md) |
| `docs/design/explicit-image-adoption.md` | [Image restoration and explicit generation adoption ](explicit-image-adoption.md) |
| `docs/design/fixed-abi-simplification.md` | [Fixed native calls without signature/container machinery ](fixed-abi-simplification.md) |
| `docs/design/fixed-names-and-local-protection.md` | [Fixed helper names and protection within a call ](fixed-names-and-local-protection.md) |
| `docs/design/fixed-selector-digests.md` | [Fixed selector-name digests ](fixed-selector-digests.md) |
| `docs/design/formally-checked-performance.md` | [Formally checked GC and performance changes ](formally-checked-performance.md) |
| `docs/design/generation-lifecycle-review.md` | [Generation lifecycle: retain ownership, reduce mandatory coordination ](generation-lifecycle-review.md) |
| `docs/design/generation-post-simplification-review.md` | [Generation after lifecycle simplification ](generation-post-simplification-review.md) |
| `docs/design/helper-lifetime-and-boxing.md` | [Helper lifetime and numeric boxing ](helper-lifetime-and-boxing.md) |
| `docs/design/id-default-lookup-fact.md` | [Id default-lookup fact — 2026-10-22 ](id-default-lookup-fact.md) |
| `docs/design/id-send-correction-20261023.md` | [Restore the Id send contract ](id-send-correction-20261023.md) |
| `docs/design/integer-performance-investigation.md` | [Public Integer arithmetic performance investigation ](integer-performance-investigation.md) |
| `docs/design/integer-send-totality.md` | [Where Integer addition's sends come from ](integer-send-totality.md) |
| `docs/design/integer-word-conversion-investigation.md` | [Integer–Word conversion investigation ](integer-word-conversion-investigation.md) |
| `docs/design/invocation-capture-investigation.md` | [Invocation and capture investigation ](invocation-capture-investigation.md) |
| `docs/design/json-number-conversion.md` | [JSON numeric conversion — 2026-10-12 ](json-number-conversion.md) |
| `docs/design/json-unescape-investigation.md` | [JSON string decoding — 2026-10-12 ](json-unescape-investigation.md) |
| `docs/design/json-value-conversion.md` | [JSON value conversion — 2026-10-12 ](json-value-conversion.md) |
| `docs/design/lifecycle-record-owners.md` | [Lifecycle records own their state ](lifecycle-record-owners.md) |
| `docs/design/logical-candidate-discard.md` | [Logical discard, explicit files and optional diagnostics ](logical-candidate-discard.md) |
| `docs/design/logue-latency.md` | [Logue interactive latency diagnosis — 2026-10-22 ](logue-latency.md) |
| `docs/design/logue-message-log.md` | [Bounded Logue messages and post-fix profiling ](logue-message-log.md) |
| `docs/design/logue-reprofile-20261022.md` | [Logue runtime reprofile, 2026-10-22 ](logue-reprofile-20261022.md) |
| `docs/design/logue-runtime-work-summary.md` | [Logue runtime work: current state and continuation ](logue-runtime-work-summary.md) |
| `docs/design/lsp-and-globals-owners.md` | [LSP ownership and the Globals publication boundary ](lsp-and-globals-owners.md) |
| `docs/design/method-binding-constructor.md` | [Shared Method binding constructor ](method-binding-constructor.md) |
| `docs/design/method-code-admission.md` | [Admit closure code shape once per Method ](method-code-admission.md) |
| `docs/design/method-invocation-owner.md` | [Authored method construction policy ](method-invocation-owner.md) |
| `docs/design/method-lookup.md` | [Ordinary method lookup — 2026-10-12 ](method-lookup.md) |
| `docs/design/method-publication-owner.md` | [Method publication ownership ](method-publication-owner.md) |
| `docs/design/microbenchmarks.md` | [Short paired performance measurements ](microbenchmarks.md) |
| `docs/design/minimal-id-stackless-machinery.md` | [Necessary machinery for Id, stackless execution and concurrency ](minimal-id-stackless-machinery.md) |
| `docs/design/minimal-mutation-and-coordination.md` | [Ordinary mutation and explicit coordination ](minimal-mutation-and-coordination.md) |
| `docs/design/module-owned-registry.md` | [Module owns its registry ](module-owned-registry.md) |
| `docs/design/module-scoped-candidates.md` | [Module-scoped candidate changes ](module-scoped-candidates.md) |
| `docs/design/native-module-decomposition-roadmap.md` | [Native module decomposition roadmap ](native-module-decomposition-roadmap.md) |
| `docs/design/native-value-foundation.md` | [Native value foundation ](native-value-foundation.md) |
| `docs/design/one-way-image-application.md` | [One-way image application ](one-way-image-application.md) |
| `docs/design/oop-compiler-execution-integration.md` | [Oop compiler and Execution integration ](oop-compiler-execution-integration.md) |
| `docs/design/oop-continuation-id.md` | [Continuation-aware Id: design and proof checkpoint ](oop-continuation-id.md) |
| `docs/design/oop-custom-lookup-tail-contract.md` | [Custom lookup breaks tail-call reuse in the Oop migration ](oop-custom-lookup-tail-contract.md) |
| `docs/design/oop-entry-seam-experiments.md` | [Id and Execution entry experiments ](oop-entry-seam-experiments.md) |
| `docs/design/oop-id-slowdown-diagnosis-20261023.md` | [Why the Oop-only native slice is slower ](oop-id-slowdown-diagnosis-20261023.md) |
| `docs/design/oop-invariant-migration-20261023.md` | [Completing the Oop-only invariant migration ](oop-invariant-migration-20261023.md) |
| `docs/design/oop-migration-architecture-cleanup.md` | [Architecture cleanup before completing the Oop migration ](oop-migration-architecture-cleanup.md) |
| `docs/design/oop-only-id-probes.md` | [Oop-only Id: allocation and interning probes ](oop-only-id-probes.md) |
| `docs/design/oop-only-id-slice-20261023.md` | [Oop-only Id: bootstrap-to-GC qualification ](oop-only-id-slice-20261023.md) |
| `docs/design/oop-only-id.md` | [Oop-only Id ](oop-only-id.md) |
| `docs/design/oop-payload-opacity-20261023.md` | [Opaque payloads without Storage classification ](oop-payload-opacity-20261023.md) |
| `docs/design/oop-resumable-milestone.md` | [Connected Oop resumable execution ](oop-resumable-milestone.md) |
| `docs/design/oop-runtime-simplification-20261023.md` | [Simpler Oop-only object storage and call roots ](oop-runtime-simplification-20261023.md) |
| `docs/design/oop-tail-routing-diagnosis.md` | [Tail-call regression: routing diagnosis ](oop-tail-routing-diagnosis.md) |
| `docs/design/operation-dispatch-and-global-resolution.md` | [Operation dispatch and global resolution ](operation-dispatch-and-global-resolution.md) |
| `docs/design/opt-in-generation-baselines.md` | [Opt-in generation baselines and observational validation ](opt-in-generation-baselines.md) |
| `docs/design/optional-provider-loading.md` | [Optional provider loading and production cutover ](optional-provider-loading.md) |
| `docs/design/optional-store-image.md` | [Optional Store participation in Image ](optional-store-image.md) |
| `docs/design/ordinary-candidate-activation.md` | [Candidate activation is an ordinary send ](ordinary-candidate-activation.md) |
| `docs/design/ordinary-edit-and-snapshot.md` | [Ordinary editing and snapshots of edited worlds ](ordinary-edit-and-snapshot.md) |
| `docs/design/outbound-bridge-retirement.md` | [Retire the general outbound ABI bridge ](outbound-bridge-retirement.md) |
| `docs/design/owned-globals.md` | [Globals owns its bindings ](owned-globals.md) |
| `docs/design/prepared-operation-admission.md` | [Prepared edits observe values, not a global mutation count ](prepared-operation-admission.md) |
| `docs/design/presentation-repl-startup.md` | [Presentation REPL startup: prepare code once ](presentation-repl-startup.md) |
| `docs/design/private-runtime-indexes.md` | [Private runtime integer indexes ](private-runtime-indexes.md) |
| `docs/design/publication-failure-boundary.md` | [Publication failure boundary experiment ](publication-failure-boundary.md) |
| `docs/design/range-handler-reuse.md` | [Range handler reuse — 2026-10-12 ](range-handler-reuse.md) |
| `docs/design/reflect-without-store.md` | [Reflect without mandatory Store ](reflect-without-store.md) |
| `docs/design/repl-owner-migration-performance.md` | [REPL ownership migration: qualified ](repl-owner-migration-performance.md) |
| `docs/design/representation-batch-investigation.md` | [Representation query investigation ](representation-batch-investigation.md) |
| `docs/design/representation-validation-and-lifetime.md` | [Representation validation and remaining lifetime work ](representation-validation-and-lifetime.md) |
| `docs/design/result-lifetime-handoff-investigation.md` | [Result lifetime handoff investigation ](result-lifetime-handoff-investigation.md) |
| `docs/design/retained-compilation-records.md` | [Retained compilation records own compiler metadata ](retained-compilation-records.md) |
| `docs/design/reusable-branches-and-native-inputs.md` | [Reusable branch handlers and native input protection ](reusable-branches-and-native-inputs.md) |
| `docs/design/root-send-reduction.md` | [Further root-send reduction ](root-send-reduction.md) |
| `docs/design/runtime-and-abi-simplification.md` | [Runtime and ABI simplification ](runtime-and-abi-simplification.md) |
| `docs/design/runtime-costs-exposed-by-logue.md` | [Language-wide costs exposed by Logue — 2026-10-22 ](runtime-costs-exposed-by-logue.md) |
| `docs/design/runtime-simplification-results.md` | [Runtime simplification implementation and measurements ](runtime-simplification-results.md) |
| `docs/design/runtime-storage-simplification.md` | [Runtime storage simplification with unchanged malleability ](runtime-storage-simplification.md) |
| `docs/design/rust-bootstrap-migration-audit.md` | [Remaining Rust modules and cold bootstrap ](rust-bootstrap-migration-audit.md) |
| `docs/design/scoped-generation-transactions.md` | [Scoped generation transactions ](scoped-generation-transactions.md) |
| `docs/design/send-reduction-breakage-diagnosis.md` | [Send-reduction validation failures: diagnosis and remediation ](send-reduction-breakage-diagnosis.md) |
| `docs/design/shared-invocation-investigation.md` | [Shared method invocation: adapter necessity investigation ](shared-invocation-investigation.md) |
| `docs/design/slot-and-string-cost.md` | [Ordinary-slot and String ordering costs ](slot-and-string-cost.md) |
| `docs/design/socket-owner-migration-performance.md` | [Socket ownership migration: regression and qualified remediation ](socket-owner-migration-performance.md) |
| `docs/design/source-registry-id-boundary.md` | [Source registry: Id-only ownership design check ](source-registry-id-boundary.md) |
| `docs/design/stable-guard-batching.md` | [Stable guard batching ](stable-guard-batching.md) |
| `docs/design/standalone-cells.md` | [Authored Cells independent of Store storage ](standalone-cells.md) |
| `docs/design/store-independent-core-completion.md` | [Store-independent core: completion plan ](store-independent-core-completion.md) |
| `docs/design/store-planner-experiment.md` | [Store versus persistent snapshots: build-planning experiment ](store-planner-experiment.md) |
| `docs/design/store-refactor-concerns.md` | [Store refactor: concerns, experiments and proposed solutions ](store-refactor-concerns.md) |
| `docs/design/store-removal-audit.md` | [Store removal: invariant audit before method publication ](store-removal-audit.md) |
| `docs/design/supplied-method-templates.md` | [Supplied method templates ](supplied-method-templates.md) |
| `docs/design/task-field-migration-performance.md` | [Task field migration: regression and approved remediation ](task-field-migration-performance.md) |
| `docs/documentation-review.md` | [Documentation review ](documentation-review-20260930.md) |
| `docs/misc/DESIGN.md` | [Birnam Language Design Goals ](language-design.md) |
| `docs/validation.md` | [Validation ](validation-history.md) |
| `docs/design/42-object-canonical-relational-derived.md` | [42 — The Object Model is Canonical; the Relational Reading is Derived](42-object-canonical-relational-derived.md) |
| `docs/design/37-the-relational-reading.md` | [The relational reading — datalog over the id-stack store](37-the-relational-reading.md) |
