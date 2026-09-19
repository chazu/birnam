# 81AJ — System GC control methods

> **Status:** Implemented, following printing defaults in 81AI.

Move System gc/liveObjects/autoGc:/preciseGc: from Prelude into
`birnam-system-gc`. Copy their small borrowed-argument adapter, which Prelude
still uses for other methods. This module owns the existing language-facing
control methods, not the collector implementation or System family. It depends
on Integer and loads immediately before Prelude. Module initialization only
enables a provider; it creates no objects, roots, layouts or methods.

Prelude retains System creation/adoption, all twelve selector interns, and the
first eight method registrations. At the original final four registration slots,
call the new installer with the current registration table, Prelude's retained
original execution table, the selected System behavior and the four already
interned selectors. The immutable descriptor has one operation and is 24 bytes
on arm64. Do not reintern or cache these selectors or behavior. Retain the supplied
original execution pointer before registration can call a method or reenter;
first installation may itself be reached through reentry with a different table.

Copy all four bodies unchanged. The two queries ignore receiver/carrier, reject
nonzero arity, call the live collector callback, cast its usize result to i64,
then use the live Integer constructor. The constructor must be selected after
the collector callback, including when that callback changes the constructor.
Preserve null and alias constructor results.

The two setters require exactly one borrowed argument and compare it with the
retained table's live true/false identities. They accept any receiver, forward
0/1 to the current setter and return the original receiver, including null.
Preserve true precedence if the identities alias, null identity behavior,
malformed-carrier rejection and invalid values returning current nil without
calling the collector. Do not add type checks, error construction, new policy,
collector state, IdApi fields or image-format changes.

Use an independent C callback/value oracle against installed 81AI. Cover arity,
receivers, Boolean identity changes/aliasing, boundary count casts, null/alias
Integer construction, constructor replacement from a query, nested calls and GC.
Compare full installation traces and first-table retention under first-gc-intern
and first-readLine-binding reentry. Check supplied selector identity and target
behavior snapshots, publication/adoption and reinstallation. Verify fresh debug
startup, a source-free compiled module, a saved v8 native Code image and a baked
executable, then commit. No release validation.

## Validation

- The four method bodies and borrowed-argument adapter match 81AI after
  normalizing formatting. Their four registration statements are unchanged;
  all other Prelude functions except the coordinating installer match the prior
  source. Each control Method resolves into System GC, with no Prelude forwarder.
- The independent C oracle passes 496 count/identity/receiver combinations in
  normal startup, first-gc-intern reentry and first-readLine-binding reentry.
  All callback traces match the prior installed 81AI build, development libraries
  and the fresh installed prefix. This includes calls during method registration
  before the outer installation has finished.
- Query counts include zero, ordinary counts, INT64_MAX, its unsigned successor
  and SIZE_MAX; their i64 bit patterns are preserved. Receivers include null,
  nil, ordinary objects and System. Zero-arity queries ignore their carrier;
  invalid arities and setter carriers return current nil before callbacks.
- The Boolean oracle covers canonical, replaced, aliased and null identities,
  including true precedence when both identities match. Valid setters preserve
  the supplied receiver and forward exactly 0/1; invalid values never call either
  setter. A setter callback changes the identities, and the next call observes
  those changes. The selected nil field remains live as well.
- A query replaces both Integer make and its context; construction observes the
  replacements after the query. Null and alias construction results pass through.
  Queries and constructors reenter other control methods and collect. Copied
  Methods remain callable after public replacement, and precise collection after
  return preserves the explicitly rooted result.
- Reinstallation through another table retains Prelude's original execution
  table. A readLine registration callback changes the selector mapping and
  exported SystemVt after selection; the current installation keeps its original
  snapshots, while the next installation adopts the changed behavior/selectors.
  The provider adds no objects, roots, layouts or cached Oops.
- Descriptor admission rejects null, wrong version/size, missing install and
  retargeting, and accepts repeat attachment of the same pointer. Development and
  installed libraries match the four-symbol inventory, relocatable install name
  and system-only linkage. Prelude's sole export change adds System GC attachment.
  The collector, System representation, IdApi v7 and image encoding are unchanged.
- Focused System GC, Printing (1,104 cases in each of two modes), Boolean (34),
  Float (676 arithmetic cases) and Word (81 equality cases) scripts pass, as do
  their installed native checks. All 145 stdlib assertions pass. ABI tests (four),
  Prelude tests (two), host tests (86, one ignored), builder tests (four), and
  ABI/System GC Clippy with warnings denied pass. No release validation was run.
- A fresh external debug prefix contains 33 native libraries and 28 compiled
  modules. All 1,602 source hashes stayed unchanged during installation; this
  validation record was added afterward. Installed package metadata and all 61
  required libraries match the updated install-smoke inventory. The full
  install-smoke script was not run.
- Both the previous build and the fresh prefix pass dynamic startup, a source-free
  compiled module, a saved v8 native Code image and a baked executable. They
  exercise actual collection, live-object queries, automatic/precise policy
  changes, receiver aliases, retained values across precise collection, invalid
  arguments and replacement of all four public methods. System GC is observed
  loaded during new startup, image and baked runs.

Receipts are `.cargo-target/81aj-{check,build,compare,system-gc,printing,boolean,
float,word,unit,clippy,prefix,prefix-native,image-baseline,image,stdlib}.log`,
plus source hashes and per-mode baseline/development/installed control traces.

## Next boundary

Review System print:/println: and their shared text-conversion helpers. Output
uses exact String recognition or fresh printString dispatch, then synchronously
borrows bytes while writing/flushing stdout. Source-load diagnostics also use
printable_string, and several file/environment operations use string_bytes.
Resolve that shared helper ownership before extracting another caller; preserve
byte-borrow lifetimes, callback order, null/empty distinctions, newline/flush
behavior and the existing error-result handling without duplicating policy.
