# 75 — Code quality and responsibility cleanup

> **Status: proposed for review, 2026-09-06.** This document specifies items
> 1–6 from the code-quality review requested by the user. It records proposed
> implementation choices; no code is changed or acceptance gate closed by
> writing it. Timing acceptance remains pending. Documentation disposition is
> specified separately in [Design 76](76-documentation-cleanup-and-archives.md).

> **Follow-up review (2026-09-15):** [Design 78](78-invariant-preserving-refactoring-review.md)
> rechecks these findings against current source under an invariant-preserving
> refactor scope. It excludes the worker-arity correction from that batch and
> adds per-candidate semantic and lifetime gates. This document remains the
> original proposal.

## 1. Outcome and scope

Make common rules have one implementation, give related runtime state a clear
owner, and make production modules describe their actual responsibilities.
The first changes should be small enough to review independently. The larger
changes should reduce how much a maintainer must understand before editing the
compiler or host.

This continues [Design 72](72-rust-mechanical-consolidation.md), which already
shared digest/scalar mechanics, C output escaping, private ABI carriers, and
receipt checks. Those completed changes are the starting point.

The six work areas are:

| Item | Change | Character of the work |
| --- | --- | --- |
| 1 | Share selector arity and source-string decoding | Mechanical sharing plus one explicit correctness correction |
| 2 | Share discovery of private symbols in the selected runtime library | Unsafe-mechanics consolidation with existing lifetime and cache policy |
| 3 | Separate Codegen compilation, native materialization, and publication responsibilities | Incremental module and state-ownership refactor |
| 4 | Separate host command parsing, project workflow, and runtime startup | Incremental module refactor |
| 5 | Separate relational evaluation from native adapters and simplify result writing | Mechanical sharing plus removal of redundant value copies |
| 6 | Retire migration-era lint suppressions and comments | Audited deletion and current responsibility documentation |

The LSP extraction mentioned as a later opportunity in the review is outside
these six items. Compiler optimization experiments and concurrency architecture
remain in [Design 74](74-malleable-multicore-execution-and-optimization.md).

## 2. Contracts that carry through every change

Read these requirements with the existing native-entry contract in
[60E](60e-resumable-registration-and-entry.md), lifetime and reclamation rules in
[60J](60j-runtime-identity-and-history-compaction.md), native-carrier ownership in
[60N](60n-portable-resumable-native-carrier.md), and project-publication contract
in [65](65-self-describing-project-publications.md). This cleanup does not replace
those authorities.

- Preserve ordinary dispatch, custom lookup, inheritance, DNU, method mutation,
  and reflection. Consolidating Rust implementation must not bypass a Birnam
  send or replace current lookup with a cached native method address.
- An active Frame keeps the implementation it entered; future sends see current
  dispatch. Replaced code remains alive while a Frame, Block, lease, or another
  legitimate owner can still use it.
- Preserve object identity, evaluation and error ordering, rooted objects,
  callback reentry, and native-entry barriers. Shorter Rust code is not evidence
  that a shorter rooting lifetime is safe.
- Preserve the public IdApi layout, native calling conventions, exported
  symbols, durable bytes, exact format admission, and generated-code contracts.
  Arbitrary native methods remain supported without compiler recovery metadata.
- Preserve the independent Rust cold bootstrap. A shared source implementation
  must not introduce a dependency on a later-loaded provider or installed
  Birnam artifact. No new dynamic library is proposed.
- Keep independent format readers, golden fixtures, and compiler semantic
  oracles independent. Their duplication is evidence against correlated errors.
- Keep current owner-thread requirements. Sharing implementation does not make
  process-global state safe for multiple threaded Runtimes.

The two intended departures from purely mechanical behavior are the worker
selector correction in item 1 and the allocation reduction in item 5. Record
them separately from module moves, with their own focused verification. Any
other discovered semantic discrepancy needs its own stated resolution.

## 3. Item 1 — Shared language byte rules

### 3.1 Selector arity

The same classification appears in the [compiler](../../rust/birnam-codegen/src/lib.rs),
[Codegen runtime](../../rust/birnam-codegen-runtime/src/lib.rs), and
[portable code](../../rust/birnam-portable-code/src/portable_code.rs).
The [host](../../rust/birnam-host/src/lib.rs) has a separate predicate used when
validating an isolate's entry selector.

The host currently considers a selector beginning with `_` and containing no
colon to accept one argument. The other implementations consider it unary,
with zero arguments. For example, `_private` passes the host's arity-one test.
This is a validation inconsistency established by source inspection; the review
did not run an end-to-end worker reproduction.

Proposed implementation:

1. Put the total byte-classification operation in a small pure `selector`
   module in `birnam-artifact`, available through its existing provider-free
   view. These rules are already used by the portable-program grammar, and all
   affected crates have an existing route to Artifact code.
2. The shared operation returns the colon count when nonzero, zero for an empty
   selector or one beginning with an ASCII letter or `_`, and one otherwise.
   It allocates nothing and does not validate whether a name is admissible.
3. Keep `portable_program::selector_arity` as the checked format interface.
   It must still reject empty, NUL-containing, or oversized names and preserve
   its quantity conversion and error ordering before using shared mechanics.
4. Replace the repeated unchecked operations. Make the host test the shared
   result for equality with one, explicitly correcting underscore handling.

Do not make portable names require UTF-8 or source-spellable identifiers. The
physical format deliberately admits a broader name space than source syntax.
This helper shares the existing classification; it does not redesign it.

Verify unary, underscore, binary, keyword, empty, and non-ASCII byte inputs at
their respective interfaces. Add a focused worker-entry rejection case for a
zero-argument underscore selector. Keep malformed-format cases in the checked
Artifact tests rather than weakening them to match permissive callers.

### 3.2 Source-string escape decoding

The byte-decoding loops in `Emitter::emit_string_literal`,
`portable::decode_string`, and the runtime's `decode_string_literal` recognize
the same escapes. They differ in how they admit the surrounding literal:

| Caller | Existing admission behavior |
| --- | --- |
| Ordinary emitter | Removes the first and last byte when the input length is at least two |
| Portable encoder | Requires opening and closing quotes; returns its format-specific error |
| Codegen runtime | Requires opening and closing quotes; returns `None` on rejection |

Add a shared literal-body decoder to the pure Codegen crate. Each caller keeps
its admission behavior and supplies the inner bytes. The shared implementation
retains the current initial Vec capacity, recognized escapes, embedded NULs,
unknown escapes, and trailing-backslash behavior. This is distinct from the
already-shared escaping of bytes into generated C.

Keep the generated C and portable decoded bytes unchanged for existing inputs.
Use explicit expected-byte cases through the real emitter and portable encoder,
including malformed literal envelopes. Avoid rewriting validation as part of
the extraction.

## 4. Item 2 — Exact runtime-library symbol discovery

[TCC](../../rust/birnam-tcc/src/lib.rs),
[Codegen](../../rust/birnam-codegen-runtime/src/lib.rs), and
[Image](../../rust/birnam-image/src/lib.rs) repeat `DlInfo`, `dladdr`, platform
loader flags, and opening the library that supplied `IdApi.dlsym`. TCC repeats
that sequence for registration and retirement functions.

This exact-library behavior matters: the host deliberately loads the selected
floor with local symbol visibility. Looking up a private symbol through global
loader order could select a different library.

The proposed home is a small stateless `native_library` helper module in
`birnam-abi`, which these callers already use. It owns the platform carrier and
the mechanics of opening the library containing a supplied address. Symbol
conversion remains typed at the caller. It adds no IdApi field or C export.

Keep policy outside the mechanical helper:

- Image and Codegen cache a process-lifetime handle, then perform symbol lookup.
- TCC caches individual resolved function addresses and retains their library
  references. A failed lookup is cached differently from the handle-based path.
- The first consolidation preserves these differences, loader flags, null
  checks, and library-retention behavior. It does not add retries or automatic
  `dlclose`, or silently combine caches into one new global singleton.

Document the ownership of every acquired handle, including an intentionally
retained handle. Use typed C strings at fixed-symbol call sites where this
preserves their admitted inputs. Do not tighten a variable-input interface
merely to make its type easier to share.

Verify lookup from a locally loaded selected floor, missing symbols, relocated
prefixes, and the existing TCC/Codegen dylib audits. Target-dependent flags and
`dladdr` declarations get one source owner, which helps Linux work; ELF loading,
artifact naming, native roots, and full Linux acceptance remain separate work.

## 5. Item 3 — Codegen responsibility and native-code lifetime

### 5.1 What native materialization means

Native materialization makes a description of Birnam code executable in this
process. Fresh materialization emits C, compiles it with TCC, connects runtime
symbols and captured objects, registers execution metadata, and provides code
that Birnam can call. Image recovery can also map an existing native carrier.

```text
Portable meaning / lowered program
        → native compilation or mapping
        → runtime binding and registration
        → callable code with a live owner
        → retirement when nobody can execute it
```

Compilation finishing does not establish when the resulting memory can be
freed. A suspended Frame or escaped Block may still need an older method's
native code after the method is replaced.

### 5.2 Proposed responsibility split

The current Codegen runtime combines source/CST analysis, C emission
orchestration, TCC materialization, image recovery, candidate publication,
Store compensation, reflection, and several root and retirement registries.
The synchronous, named, and resumable-block materialization paths repeat
compile/bind/relocate work and manually coordinate failure handling.

Use private modules within the existing crate, initially retaining its public
and Birnam-visible interfaces:

| Responsibility | Owns | Must obtain from another owner |
| --- | --- | --- |
| Source and emission | CST adaptation, source metadata, deterministic emission plans | Runtime operations needed to obtain source objects |
| Native materialization | Compilation/mapping orchestration, private symbol binding, registration stages, materialization cache and CodeBinding lifecycle operations | Floor admission and retirement results; TCC allocation/release operations |
| Publication and recovery | Candidate transitions, generation composition, activation/rollback, Store compensation, recovery orchestration | Prepared materializations and explicit retention operations |
| Reflection and exported methods | Birnam-visible entry methods and result/error adaptation | Operations supplied by the above modules |

The floor remains the authority on active execution and retirement eligibility;
TCC remains the owner of its native allocation; Codegen coordinates its retained
references. A Codegen module does not replace either ownership authority.

First extract materialization together with the state it manages. Follow with
publication and source/reflection code in separate moves. A collection of new
files that still mutates each other's registries directly does not satisfy the
responsibility change.

### 5.3 Make lifecycle operations explicit

Before moving cleanup code, inventory each existing path against this table:

| Situation | Required distinction |
| --- | --- |
| Compile or bind fails before runtime admission | Dispose only resources owned by the unfinished compilation |
| A runtime registration stage has begun | Cancel/compensate that stage before releasing referenced resources |
| Code has become callable or acquired retained owners | Withdraw associations and request retirement through the existing protocol |
| Cache entry is evicted | Release cache retention; do not infer that Frames, Blocks, or bindings released theirs |
| Floor reports the generation can be forgotten | Release remaining eligible native ownership in the existing order |

The inventory must identify registration state, roots, leases, native-library
handles, cache entries, and which operation can call back into Birnam. Preserve
existing path-specific behavior during extraction. If that audit finds a path
that does not meet its current contract, document and fix it as a separate
correctness change rather than disguising it as a move.

Concentrate repeated compilation setup and stage completion after the move-only
slice passes. Prefer explicit finish/cancel/retire operations for transitions
that may send messages or fail. Rust scope guards are appropriate for local
resources with infallible cleanup, not as an automatic replacement for runtime
compensation or code retirement.

Keep registry access short. No exclusive Rust borrow or newly consolidated lock
may remain live across an ordinary send, callback, allocation that can invoke
GC hooks, or reentrant native operation. Preserve the existing GC-visible roots
while moving storage; a raw Oop in a Rust struct is not automatically a root.

### 5.4 Relationship to future work

This organization gives a future optimizer or compiler backend a smaller
materialization responsibility to integrate with. TCC remains the backend in
this change. Portable meaning and native recovery metadata retain their current
contracts; no optimizer, deoptimization mechanism, or new native-yield strategy
is introduced.

State ownership should be visible enough to support a later threaded-Runtime
design. Initially preserve singleton scope and owner-thread entry. Replacing
all registries with one mutex would not establish safe concurrency, and moving
compiler policy from Birnam into Rust would work against self-hosting.

### 5.5 Verification

Use the existing portable materialization, CodeBinding, image lifecycle, and
bounded-churn paths. Exercise failed compilation, failed registration,
compensation, cache eviction, active older implementations, retained Blocks,
suspended Frames, mapped/rebuilt images, replacement, rollback, and eventual
reclamation. Add focused failure-path coverage only where the ownership audit
finds a meaningful gap. Reuse production entry interfaces rather than testing
new private files merely because they were extracted.

## 6. Item 4 — Host modules by responsibility

The [host implementation](../../rust/birnam-host/src/lib.rs) includes command
parsing, Git/cache acquisition, project discovery and publication, executable
resources, library loading, image startup, worker startup, and REPL execution.
Its public `HostRuntime` can remain the host's coordinating interface.

Proposed private module groups:

| Group | Contents |
| --- | --- |
| Command parsing | Option types, argument parsers, command classification, diagnostics |
| Project workflow | Dependency/cache operations, source observations, publication construction and validation, project command orchestration |
| Runtime startup | Executable-relative resources, loaded libraries, startup policy, image/worker entry, execution anchors |

Move pure parsing and project-data functions first, retaining public re-exports
and exact error behavior. Move each corresponding test group with its existing
interface. Then separate project operations from startup and loading. Keep
module internals private or narrowly crate-visible; avoid broad visibility just
to make a split compile.

Project semantics remain authored in Birnam where they are today. In
particular, ordinary run/install must consume a successful publication without
re-reading source or reinterpreting the manifest. Preserve atomic publication,
offline dependency rules, failure precedence, native-library lifetime, and the
caller-owned GC anchor through synchronous execution.

This gives Linux resource changes and project changes separate implementation
homes. It does not introduce a generic command framework or a new platform
interface before actual target behavior requires one.

## 7. Item 5 — Relational evaluation and output adaptation

The [relational crate](../../rust/birnam-relational/src/lib.rs) mixes pure query
evaluation with Store callbacks, root management, native input parsing, and
repeated output-buffer writing. Preserve its authority-free evaluation model:
Store supplies facts; the evaluator computes ordered results.

First consolidate the repeated checked writing of `ClassifiedValue` results.
The helper must preserve capacity failure, successful `written` assignment,
output order, and the existing status values. Earlier input validation and
Store collection remain in their existing order. Do not construct a Rust slice
from a null output pointer merely because the caller supplied zero capacity.

Several sites use `iter().cloned()` on an owned result Vec before calling
`record`. A String value owns bytes, although its output record only records
their length. Prefer borrowing values for record conversion; use consuming
iteration only where ownership transfer is already natural. No clone should be
needed just to read a value's scalar fields or byte length.

Record that allocation reduction separately from helper extraction. It should
not change the query algorithm, deduplication/equality rules, returned order, or
the lifetime of GC-visible objects. It carries no measured speedup claim.

Then move the pure value/query implementation into an evaluation module and
leave Store collection, Oop conversion, native exports, and root providers in
adapter modules. Preserve the existing public Rust interface through
re-exports. Test the evaluator through semantic cases and the adapters through
capacity, malformed-input, status-precedence, empty-result, and String-output
cases. Keep the existing query-matching regressions.

## 8. Item 6 — Warnings and current module documentation

Several floor modules have `allow(dead_code)` at module scope, justified by
R2/S1 migration stages that have finished. Execution, Relational, and Codegen
also have opening descriptions that understate their current responsibilities.

Work one crate or module group at a time:

1. Remove a broad suppression in a diagnostic pass and classify the resulting
   warnings against production callers, C exports, generated code, and tests.
2. Delete confirmed obsolete private code, or retain a narrow allowance with
   a current, specific reason. Public and native interfaces need a use and
   compatibility audit before removal; an in-tree search alone is insufficient.
3. Put test-only helpers under test configuration when that actually describes
   their use. Do not hide independent fixture interfaces from the builds that
   need them.
4. Rewrite module summaries around current responsibility, ownership, and
   callback constraints. Keep historical milestones in design history.

The Codegen compatibility switch and adapter globals still have current uses;
their migration-era names are not evidence that they can be deleted. Do not
replace this audit with broad automated lint fixes or arbitrary method-size
rules. Local iterator, match, and error-propagation simplifications are useful
only when evaluation, allocation, and failure behavior remain understood.

## 9. Implementation sequence and completion

The labels below are proposed slices within this design, not new tracker IDs.
Beads remains authoritative for work selection and acceptance.

| Slice | Deliverable | Dependency |
| --- | --- | --- |
| Q1 | Shared arity and literal mechanics, with isolated worker-arity correction | Review of item 1 |
| Q2 | Shared exact-floor discovery, unchanged caller cache/retention policy | Review of item 2 |
| Q3a | Materialization ownership/failure inventory and move-only extraction | Review of item 3 |
| Q3b | Shared setup and explicit lifecycle coordination; publication/source split | Q3a; reuse Q2 where applicable |
| Q4a | Host parser and pure project-data module moves | Review of item 4; coordinate with Q1's host edit |
| Q4b | Host project orchestration separated from startup | Q4a |
| Q5a | Relational output helper and separately recorded redundant-copy removal | Review of item 5 |
| Q5b | Pure evaluator and native adapter separation | Q5a |
| Q6 | Audited suppressions/deletions and current module summaries | Review of item 6; finish summaries after relevant module moves |

Recommended first implementations are Q1, Q2, and Q5a. Host separation can
proceed independently in small moves. Establish Q3a before consolidating its
lifecycle operations. Q6 can begin early on untouched floor modules and finish
as other module responsibilities settle. No dependency on a new compiler
backend, Linux implementation, or accepted Design 74 mechanism is introduced.

For each slice, run focused existing tests plus regressions justified by its
behavioral risks. For the completed code work, run `mise run verify` and
`mise run test`, using private build prefixes. The required coverage includes
the existing cold-archive/relocation, native ABI/export, portable image,
materialization, CodeBinding, and bounded-churn checks; reuse evidence from the
canonical gate rather than rerunning identical checks without a reason.

Completion means all six responsibilities are addressed, the intended duplicate
production mechanics have one source owner, external contracts are preserved
except for the stated arity correction, and verification covers the final code.
Module count or line-count reduction alone is not acceptance. Correctness and
implementation evidence do not close pending timing or release acceptance.

## 10. Review decisions

This proposal recommends the helper homes and responsibility split above.
Review should settle those choices, the explicit worker-arity correction, and
the scope of the materialization ownership inventory. Exact private filenames
can follow the final ownership grouping; introducing new runtime services or
widening native interfaces is outside this design.

Writing this document does not start implementation, archive documentation,
modify the current performance epic, or accept the proposed optimization and
concurrency mechanisms in Design 74.
