# 76 — Documentation cleanup and archival policy

> **Status: proposed for review, 2026-09-06.** The user requested a separate
> design for documentation cleanup following the code-quality assessment.
> No document is moved, deleted, or declared superseded by this proposal alone.
> Code cleanup is specified in [Design 75](75-code-quality-and-responsibility.md).
> Existing design decisions, Beads dependencies, and pending timing acceptance
> retain their current status.

## 1. Outcome

A reader should be able to establish what Birnam currently does, find the
contract for a proposed change, and distinguish that contract from an old plan.
Today several entry documents present different stages of the implementation
as current. Retired application plans and completed reviews also occupy the
same directory as current contracts.

Correct the conflicting summaries first. Archive clearly historical material
intact. Where history still contains a surviving rule, give that rule a current
home before moving the history.

This is an information-organization change. It does not decide concurrency,
weaken malleability, change the native interface, or lower performance goals.
In particular, selected Self-style optimization is a direction; a proposed
experiment or concurrency mechanism does not become implemented through an
editorial status change.

## 2. Give each kind of document one job

| Location | Job | What belongs elsewhere |
| --- | --- | --- |
| Root README | Introduction, quick usage, supported platform, links into the system | Detailed stage-by-stage delivery history |
| `docs/misc/DESIGN.md` | Concise current goals and architecture/status index | Competing copies of complete semantic and byte contracts |
| `docs/guide/` | Current user-facing behavior and examples | Retired application plans and implementation review transcripts |
| `docs/design/` | Accepted contracts, proposals under review, current evidence and decision records | Superseded execution plans whose surviving rules have another owner |
| `docs/archive/` | Historical plans, experiments, resolved review rationale, superseded milestones | Current work-selection authority |
| `CHANGELOG.md`, `docs/measurements/` | Dated delivered changes and preserved evidence | Future promises presented as accepted results |
| Beads and `docs/misc/TASKS.md` | Authoritative acceptance/dependencies and a short navigation summary | A second independently maintained issue backlog |

Reuse `DESIGN.md` as the current architecture index rather than adding another
overview with the same job. Its entries should state the current capability,
link the owning contract, and distinguish implemented, selected direction,
proposed, and historical material. Implementation references are evidence of
what code does; they do not silently amend an accepted contract if a discrepancy
is found.

The README may retain a short platform statement and architecture summary for
orientation. Detailed status belongs at the linked owner. The goal is to avoid
several evolving copies, not eliminate useful introductory repetition.

## 3. Correct current entry documents

### 3.1 README and architecture index

The [README](../../README.md) stackless design summary still ends at S2,
image-v5, and future suspension. [DESIGN.md](../misc/DESIGN.md) retains the Zig
cold compiler, a pending Rust migration, and S2 implementation readiness.

Rewrite those summaries from the current implementation and accepted records:

- The production native implementation, compiler bridge, host, and cold source
  build are Rust-based; authored Birnam modules are compiled during bootstrap.
- Stackless execution, host suspension, one-shot continuation machinery, and
  portable code recovery are delivered within their documented contracts.
- Current images use v8; portable meaning and its native carrier have distinct
  roles. Do not reduce that relationship to an old source-only recovery model.
- Blocking process isolates exist. A general reactor, fairness policy, and
  multiple threaded Runtimes are not delivered concurrency capabilities.
- Native synchronous calls still have entry-barrier and lifetime constraints;
  stacklessness does not make arbitrary native call chains suspendable.
- Self is the selected optimization direction, arbitrary native IdApi use
  remains required, and specific mechanisms in Design 74 remain proposals.

Use [Design 60](60-stackless-native-execution.md),
[Design 62](62-rust-native-rungs-migration-plan.md),
[Design 60O](60o-image-v8-identity-preservation.md), and
[Design 74](74-malleable-multicore-execution-and-optimization.md) as the
navigation starting points. Preserve the current ownership of detailed rules
in the linked designs. Old statements embedded within otherwise current
designs need a targeted status correction or historical label when found.

### 3.2 Platform support

[PLATFORM-SUPPORT.md](../misc/PLATFORM-SUPPORT.md) still references a Zig oracle
lane and its continuing acceptance requirements. Reconcile it with the
completed Rust cutover and the actual installer/verification graph.

Keep arm64 macOS as supported and Linux as planned. Describe Linux requirements
in terms of target-aware compilation, native ABI and root handling, library
loading and artifact discovery, cold builds, runtime behavior, and distribution
verification. Label the old non-PIC TCC probe as historical evidence if retained;
one historical link failure is not the complete current porting assessment.
Do not present unused non-macOS code branches as Linux qualification.

### 3.3 Work and acceptance status

Keep Beads as the active backlog. Preserve the existing correctness/timing split
and design-review gates. Rewrite stale chronological instructions in summaries
as pointers to current tracker state where needed; avoid copying all ticket
statuses into this new index. Source changes, installed artifacts, and release
qualification must remain distinguishable.

## 4. Archival disposition

The destinations below are proposed directories. Retain numbered filenames and
document contents except for the status/navigation changes needed by the move.

| Material | Proposed disposition | Prerequisite |
| --- | --- | --- |
| Designs 55, 58, 59 | Move together to `docs/archive/digla/` | Repair links; preserve historical commit references |
| `docs/misc/DEMO.md` | Move to `docs/archive/legacy-kernel/DEMO.md` | Redirect current demo navigation to README/guides |
| Historical `docs/misc/VISION.md` mechanisms | Preserve in `docs/archive/legacy-kernel/VISION.md` | Identify still-accepted aspirations in current goals |
| Reviews 60HA, 60KA, 60LA, 60MA, 60NA, 60OA | Move to `docs/archive/reviews/` | Confirm accepted outcomes are in their canonical designs |
| Design 50 and related 48A/49A/49B/50A review history | Move to `docs/archive/work-breakdowns/` and `docs/archive/reviews/` respectively | Identify surviving isolate/publication rules and repair their references |
| Design 61 rewrite history | Move to `docs/archive/migrations/` | Preserve and reconcile current native-runtime rules before moving |
| `docs/misc/DATALOG.md` | Mark historical status immediately during cleanup; archive only after coverage audit | Reconcile useful query semantics with current guide and implementation |
| `docs/guide/style-guide-review.md` | Keep pending disposition; later archive in `docs/archive/reviews/` | Record how unresolved adoption recommendations were handled |

### 4.1 Retired Digla and old demo

[55](55-birnam-digla.md),
[58](58-terminal-repl-and-programming-environment.md), and
[59](59-digla-adversarial-architecture-review.md) explicitly identify the removed
experiment as historical. Together they contain 1,751 lines in the reviewed
checkout. Their architectural lessons and recorded commits are worth retaining;
their old application paths should not look like current source references.

Move the cluster intact and update its internal links, README, and DESIGN
references. Where a source file was deliberately removed, link to its recorded
historical revision rather than creating a placeholder source file. Preserve
current terminal REPL, socket REPL, LSP, and Termbox guidance.

[DEMO.md](../misc/DEMO.md) already declares its 375-line demonstration historical.
Archive it as a whole. Its old source extension, CLI, and fact-canonical model
must not be advertised as current usage.

### 4.2 Historical vision and current goals

[VISION.md](../misc/VISION.md) marks its old authority model historical, but
later still calls a Datomic threading architecture locked and sets an older
performance aspiration. These claims are particularly confusing during the
current concurrency and optimization inquiry.

Create a short disposition list before moving it: aspiration retained in current
goals, mechanism superseded by a named design, or open proposal with no current
selection. Preserve the old document in the archive rather than rewriting its
history to match today's architecture.

Current goals must explicitly retain malleability, independent cold bootstrap,
a path to self-hosting, arbitrary native participation, and execution speed as
close as practical to a system such as Objective-C. That objective is not a
numerical acceptance threshold or a measured performance claim. Link Design 74
for the selected Self direction and open concurrency choices. Do not replace
the old locked architecture with a new lock that has not been agreed.

### 4.3 Resolved reviews and retired rollout plans

The six resolved 60-series reviews record incorporated outcomes:
[60HA](60ha-stackless-production-acceptance-and-rollout-adversarial-review.md),
[60KA](60ka-portable-codeobjects-adversarial-review.md),
[60LA](60la-portable-program-and-image-v7-adversarial-review.md),
[60MA](60ma-portable-code-object-protocol-adversarial-review.md),
[60NA](60na-portable-resumable-native-carrier-adversarial-review.md), and
[60OA](60oa-image-v8-adversarial-review.md). Keep the review rationale and
decision dates intact, and leave a backlink from each accepted design.

[Design 50](50-five-phase-live-modules-and-isolates-implementation-plan.md)
explicitly retires its five-phase plan, but
[Design 49](49-live-module-evolution.md) still references representation choices
there. Before moving Plan 50 and its related review history, map each referenced
rule to a current owner or explicitly historical discussion. Blocking isolate
and live-publication contracts must not disappear with the rollout plan.

### 4.4 Completed Rust rewrite

[Design 61](61-rust-libid-rewrite-plan.md) contains valuable ABI, unsafe/reentry,
GC, and native-lifetime rules mixed with 2,442 lines of completed migration
history. Its Zig-oracle and image-compatibility status statements are outdated.

Inventory its surviving rules and their superseding amendments. Where a current
design already owns a rule, link it. Put otherwise orphaned current rules in a
concise `docs/misc/RUNTIME-CONTRACT.md`, with explicit provenance and links to
any later amendments. That document is justified only for rules that otherwise
lack a current home; it must not duplicate the entire 60-series contract set.
Do not copy old permanent-identity or image-admission requirements verbatim
when later designs changed them.

Then archive the completed phase checklists, parity matrix, estimates, and
execution record. Keep [Design 62](62-rust-native-rungs-migration-plan.md)
current: despite its filename, it has already been reduced to a useful short
description of the Rust implementation and bootstrap.

### 4.5 Query and style material

[DATALOG.md](../misc/DATALOG.md) references removed Zig implementation files and
says clause constants match by identity. The current
[fact-store guide](../guide/03-fact-store.md) and Rust query value comparison
describe value matching for boxed values. Compare the old document's topics
against current behavior: clauses, joins, historical reads, predicates,
negation, disjunction, rules, aggregates, and pull behavior. Record implemented,
superseded, and unimplemented topics separately.

Retain useful current explanations in the guide or an explicitly current query
contract. Archive the old phase narrative once that comparison is complete.
Neither the historical text nor an editorial cleanup may silently change query
semantics or declare all old features implemented.

The [style guide](../guide/style-guide-DRAFT.md) remains a draft, and its
[review](../guide/style-guide-review.md) still has unresolved adoption work.
Consolidate useful readability advice with current idioms, separate hard syntax
rules from judgment, and record each adoption recommendation's disposition.
Performance assertions should cite preserved benchmark evidence with its
qualification, rather than becoming universal rules about map dispatch or
method decomposition. Preserve the executable style proof. Archive the review
only after its open disposition is recorded; leave unaccepted guidance labeled
as draft.

## 5. Contracts and evidence to preserve

An old filename or a completed implementation is not enough reason to archive
a design. [60I](60i-stackless-artifact-byte-grammar.md) still supplies canonical
framing, and [60L](60l-portable-program-and-image-v7-byte-grammar.md) owns
PortableProgram and related bytes inherited by later designs.
[60O](60o-image-v8-identity-preservation.md) retains parts of the preceding
grammar. Preserve these reachable dependencies and their exact anchors.

Keep current semantic, native-entry, ownership, portable-code, and lifetime
contracts accessible; keep benchmark methodology, immutable evidence, and
accepted performance tradeoffs. Preserve the status of proposals 73, 74, 75,
and 76 and all unresolved acceptance criteria. The existing archive of the old
task tracker is already history and needs no second copy.

Source-code comments owned by item 6 of Design 75 should be corrected with the
related code change. Documentation cleanup may flag them, but should avoid
competing edits to the same implementation files.

## 6. Move and link procedure

For each archival batch:

1. Search incoming paths and heading fragments across docs, README, scripts,
   manifests, and tracked issue references. Record the original path, new path,
   disposition, date, and current successor in an archive index.
2. Move the complete record and update relative links inside it. Preserve
   chronology, decisions, measurements, and historical qualifications.
3. Update current tracked links to the new historical location or to the current
   contract, according to what the link is meant to support. Keep immutable
   evidence and historical issue text intact; use a compatibility pointer when
   a retained reference cannot sensibly be edited.
4. Leave a short forwarding document at a widely referenced old path where
   appropriate. It must identify the record as historical, point to its archive,
   and identify the current contract. Retain known linked headings as pointers
   if needed; do not keep a second full copy under the old path.
5. Run path and fragment checks and inspect the resulting navigation manually.
   Classify pre-existing historical broken links separately from newly broken
   links. Historical source references should resolve to recorded revisions
   where that revision is known.

Use `docs/archive/README.md` as the archive index. A compact table is enough;
do not create a new database or a second work tracker. Preserve stable numbered
document names so references to a design number remain useful after a move.

Guide changes need an additional distribution check. The build driver copies
`docs/guide/` into the installed prefix. Verify that the current user guide
remains present and navigable there and that archival links are not advertised
as bundled guides when their targets are outside the shipped documentation.

## 7. Sequence and acceptance

The following are proposed implementation slices, not tracker IDs:

| Slice | Result | Dependency |
| --- | --- | --- |
| D1 | Correct README, DESIGN, platform, and other misleading current-status summaries | Review of this proposal; verify against source/current contracts |
| D2 | Archive index, Digla/demo moves, and resolved 60-review moves | D1; incoming-link inventory |
| D3 | Vision disposition and surviving Design 50/61 rule ownership | D1; source/contract comparison |
| D4 | Archive old vision, rollout, and rewrite history | D3; rule coverage and link repair |
| D5 | Datalog coverage reconciliation and style-review disposition | D1; record unresolved topics explicitly |
| D6 | Final navigation, archive, and installed-guide verification | D2, D4, D5 |

Documentation work is independent of implementing Design 75. It does not need
a working optimizer, Linux port, or concurrency mechanism. Coordinate links
with module moves if those proceed later.

Acceptance requires:

- Current entry points agree about Rust bootstrap, supported platform, image
  format, stackless capability, and the limits of delivered concurrency.
- Every moved record has a clear historical status and reachable successor or
  an explicit statement that its direction was retired.
- Every surviving normative rule has a current owner; no accepted rule or
  evidence was dropped to reduce document count.
- Links and known heading fragments affected by the change work. No duplicated
  full text remains at forwarding paths. Installed guide navigation still works.
- Query/style reconciliation does not assert unverified behavior or silently
  adopt draft recommendations.
- Beads acceptance, pending timing, and current design-review decisions retain
  their prior meaning.

For ordinary Markdown edits and archive moves, use focused link, fragment,
whitespace, and navigation checks. Run the relevant documentation installation
or generation checks when guide packaging or published documentation changes;
a full runtime test suite is unnecessary for a path-only archive move.

This document proposes disposition and sequencing. The actual corrections,
moves, contract extraction, and guide verification remain implementation work
after review.
