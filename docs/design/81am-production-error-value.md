# 81AM — Shared lazy Error value policy

> **Status:** Implemented, following System output in 81AL.

Move Prelude's is_error, make_error and numeric_error helpers into
`birnam-error-value`, with its tiny intern adapter. Prelude imports ABI adapters
at every existing call; no local policy or forwarding helper remains. This owns
the existing lazy factory dispatch and exact recognition, not Error's payload,
public methods or exception handling. Error's own make_error allocates a payload
and is outside this slice.

The provider depends on prepared String v1 and loads immediately before Prelude.
Initialization only enables an immutable 32-byte descriptor. It creates no
objects, methods, roots or layouts and retains no IdApi, selectors or Oops. Both
operations receive the caller's selected live table. Recognition returns a C int;
construction receives borrowed kind/message byte pointers and lengths. The kind
is already NUL-terminated under the former helper's contract. Pass extents without
scanning, copying, interpreting or adding callbacks, and retain no buffers.

Copy the policy bodies unchanged. Recognition skips all callbacks for null,
checks the live ErrorVt export, imports its current value and, if nonnull,
compares it with the subsequent vtableOf result. Preserve that imported snapshot
even if a later callback changes the export. Do not send isError or accept an
inherited Error behavior.

Construction interns the supplied kind, then uses String's live constructor,
then checks/imports Error. Those first two callbacks still occur when Error is
absent. An absent/null factory returns the then-current nil. Otherwise intern
kind:message: freshly and send the two obtained Oops, including null/alias values,
to the previously imported factory. Preserve callback selection/order, factory
snapshots and arbitrary send results without caching, validating or rooting more
values. Calls may reenter and collect under the existing owner-thread contract.

Integer/Float/Word contain similar policy copies but have earlier preparation
and activation boundaries. Audit them separately before migrating their consumers;
do not change their startup order as part of this Prelude slice. All remaining
Prelude Block and System callers use the extracted policy now.

Use independent C consumer and callback traces against installed 81AL. Exercise
absent/null/replaced exports, exact/inherited recognition, live nil, null/alias
construction results, fresh/custom factory dispatch, mutation during callbacks,
reentry, GC and reload. Verify fresh external debug startup, a source-free compiled
module, a saved v8 native Code image and a baked executable before committing.
No release validation, new language functionality or IdApi/layout/image changes.

## Validation

- All three policy bodies and the intern adapter match 81AL after normalizing
  formatting. Every remaining Prelude function body is unchanged. Its Block and
  System callers use the native dependency at the former helper calls, with no
  local forwarding helpers or duplicated policy.
- The independent C oracle passes 114 recognition/construction combinations:
  96 condition/body recognition cases and 18 kind/message/result combinations.
  Complete consumer and callback traces match installed 81AL, development
  libraries and the fresh installed prefix. Null values, absent/null exports,
  negative presence flags, exact/inherited behavior, null/alias constructor
  arguments and arbitrary factory results preserve the prior behavior.
- Absent Error still interns the kind and constructs the message before checking
  the export. Null factory import returns current nil. Kind interning can replace
  String's constructor and context; construction can publish a replacement
  factory before lookup. A later selector callback changes the export without
  changing the already imported factory used for that send.
- Ordinary factory replacement and effectful custom lookup remain observable.
  Nested construction and callback-triggered GC pass. Recognition keeps its
  imported behavior snapshot when vtableOf changes the export, including with GC
  during those callbacks. Precise collection preserves explicitly rooted values.
- Each remaining System consumer reaches its existing error construction path:
  file reads/writes, environment access, source loading, native module loading
  and stdin reads. The read-error case uses a directory as stdin. The native
  Block loop tests cover errors returned by its condition and body. Existing
  Text View consumer checks and the saved-image source-load diagnostic exercise
  the other shared recognition call sites.
- Direct descriptor calls use alternating supplied tables, preserve the message
  byte pointer passed to String construction, and accept empty message extents.
  No table is retained by Error Value. Reloaded Prelude still supplies its
  original execution table. Provider initialization is checked with allocation,
  method-registration and root-registration callbacks forbidden.
- Admission rejects null, wrong version/size, either missing operation and
  retargeting; the same pointer can be attached again. The native operation
  resolves into Error Value. Development and installed libraries match the
  four-symbol inventory, relocatable install name and system-only linkage.
  Prelude's only export change adds Error Value attachment.
- Focused System Output (680 cases in each of three modes), Text View (682),
  System GC (496 in each of three modes), Printing (1,104 in each of two modes),
  Boolean (34), Float (676 arithmetic cases) and Word (81 equality cases) pass,
  including installed native checks. All 145 stdlib assertions pass. ABI tests
  (four), Prelude identity (one), host tests (86, one ignored), builder tests
  (four), and ABI/Error Value Clippy with warnings denied pass.
- The fresh external debug prefix contains 36 native libraries and 28 compiled
  modules. All 1,635 source hashes stayed unchanged during installation; this
  validation record was added afterward. Installed package metadata and all 64
  required libraries match the updated install-smoke inventory. Neither release
  validation nor the full install-smoke script was run.
- Installed 81AL and the new prefix both pass dynamic startup, a source-free
  compiled module, a saved v8 native Code image and a baked executable. They
  exercise actual Error kind/message values, retained aliases, precise GC,
  source-load recognition/diagnostics and fresh factory replacements returning
  a retained Error, a String and nil. Error Value is observed loaded during the
  new startup, saved-image and baked runs.

Receipts are `.cargo-target/81am-{check,build,compare,error-value,system-output,
text-view,system-gc,printing,boolean,float,word,unit,clippy,prefix,prefix-native,
image-baseline,image,stdlib,scalar-audit}.log`, plus source hashes and
baseline/development/installed consumer traces.

## Next boundary

Migrate Word's identical make_error/numeric_error copies onto Error Value next.
Their sole consumer is asInteger's existing overflow path; Word preparation
does not call either helper. Mount Error Value before Word activation and attach
the dependency before any publication or method-registration callback can reach
that path. Preserve Word's earlier storage preparation, original execution table,
live String construction and lazy Error behavior. No new module or ABI version
is needed for this consumer migration.

The same source audit confirms matching Integer/Float recognition and construction
policy, after removing comments. Their earlier activation boundaries need their
own small migrations. Error's payload constructor remains a different operation.
