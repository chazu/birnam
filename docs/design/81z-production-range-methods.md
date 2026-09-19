# 81Z — Range methods above Collection

> **Status:** Implemented, following StringBuilder in 81Y.

Move Range's six non-Vector methods (`size`, `includes:`, `at:`, `do:`,
`inject:into:` and `printString`) into `birnam-range`. Keep the existing two-i64
RAW payload, canonical objects, constructor/Integer `to:`, Collection parent,
Vector-producing `reversed` and `,`, and publication predicate in Collection
for this checkpoint. Copy the six bodies and private bounds reader unchanged;
the independent C oracle pins the temporary shared 16-byte representation.

Loading a method module after Collection would publish a temporarily incomplete
Range. Instead load Range immediately before Collection and mount a narrow,
immutable construction dependency. At its existing Range-registration position,
Collection passes its selected IdApi, retained Range behavior, three cached
selectors, and the two direct Vector-backed Method pointers to Range's installer.
The installer registers all eight entries in their original order, using the
same current installation callbacks. Six entries point directly into Range;
two still point directly into Collection. No language send is forwarded through
Collection, and no Vector helper is replaced by an overridable send. IdApi is
unchanged; the new descriptor contains one construction operation, not ordinary
Range operations or a compatibility version of Collection's interface.

Range retains the first selected IdApi and three selectors and gives those
selectors its own root provider. Collection keeps its selector roots for its
remaining methods. Reinitializing Collection invokes the installer again, just
as it previously reinstalled Range methods; cached identities/selectors and
namespace rebinding retain their behavior. Initializing the Range library only
prepares this construction service. This avoids a new export lookup, reordered
publication, or delayed Range method availability during export callbacks.
The host and native embedders pin both modules and attach Range before invoking
Collection. Existing custom IdApi instrumentation still selects the table passed
to Collection, rather than the table used to prepare the Range library.

Preserve i128 intermediate arithmetic at i64 extremes, nil on unrepresentable
size/out-of-range index, empty descending intervals, exact Integer arguments,
inherited receivers, and current return/arity/null conventions. Iteration reads
bounds once and sends through the current Id callback for every element. `do:`
ignores callback results; `inject:into:` stops only on a null result, not nil.
Printing sends each freshly constructed Integer to the cached printing selector,
reads its current String bytes, and constructs a new String. Keep reentrant
method replacement, allocation and GC schedules. Later Birnam stackless Range
methods must still replace these native loops during normal bootstrap.

Compare bodies/registration order to the committed Collection source and run an
independent native oracle against both builds, including export-time reentry,
custom API tables, partial roots, malformed arguments, numeric extremes,
callback/method replacement, bounds mutation during iteration, GC and reloads.
Verify ordinary installed startup, source-free compiled module, native-Code image
and baked executable with captured Range aliases and live replacement. Image's
generic two-RAW-word encoding and IdApi v7 remain unchanged. Use debug only.

## Validation

- The six Method bodies and two bounds helpers match the committed Collection
  source modulo formatting. The remaining constructor, Integer `to:`, bounds
  helpers and two Vector-producing bodies are unchanged. Id, its v7 manifest,
  and Image source are unchanged.
- The independent C oracle passes 648 boundary/index cases against the previous
  installed Collection, the development modules and the new installed prefix.
  It also checks signed extremes/size overflow, invalid arguments, inherited
  receivers, empty intervals, native iteration's exact null/nil conventions,
  fresh callback lookup, bounds mutation during iteration, binary printing,
  current Integer/String/Id callbacks and GC.
- The oracle records all eight bindings in their original order, collects during
  registration and publication, invokes Range before its export callback returns,
  verifies selector roots, tests malformed/retargeted dependency admission, and
  checks repeated Collection initialization with rebound namespace aliases.
  Preparing Range again preserves replacements until Collection reinstalls them.
- Native address inspection confirms the six extracted functions reside in
  `libbirnam_range`; `reversed` and `,` remain direct Collection functions. Both
  Range artifacts match the five-symbol inventory, relocatable install name and
  system-only linkage. Collection adds exactly its Range dependency symbol.
- ABI tests (four), Collection tests (four), host tests (86, one ignored), and
  installer tests (four) pass; Range and ABI Clippy pass with warnings denied.
- A fresh external debug prefix contains 27 native libraries and 28 compiled
  modules. All 1,518 source hashes remained unchanged during installation.
  Subsequent changes only extend test assertions and document results.
- Installed StringBuilder passes all four adoption/publication modes; String
  passes 410 byte/index/slice cases and 18 numeric boundaries; nil passes 30
  receiver/method cases. The installed stdlib suite passes all 145 assertions.
- Both previous and new builds pass ordinary startup, a compiled module after
  its source is removed, saved v8 native-Code image boot and baked execution.
  These preserve captured Range aliases, bounds, membership/indexing, printing,
  Vector results, later Birnam stackless iteration, live method replacement and
  GC. Range loading is observed in the new build. Captured bounds remain exactly
  one two-RAW-word Range record shared by both aliases.

Focused receipts are `.cargo-target/81z-{check,build,baseline,native,unit,host,
clippy,prefix,prefix-native,range-origin,baseline-image,image,stdlib}.log`.
No release validation was run.

## Next boundary

Audit Range's canonical objects, two-word representation, constructor and Integer
`to:` for the next extraction. Keep their existing positions in Collection's
creation/publication sequence and retain direct Vector-backed methods until
Vector storage has an explicit owner. This should remove the temporary duplicate
payload description without relocating all Vector/Map construction at once.
The shared Collection parent and Vector-guarded adoption/publication must be
accounted for explicitly; replacing private storage calls with language sends
would change live-replacement behavior and is outside this refactor.
