# 81AW — Namespace owns raw export state

> **Status:** Implemented, following Source Loader in 81AV.

Move Id's existing Environment ordered bindings and address index into the loaded
Namespace module. This is the raw Symbol/value namespace, distinct from Globals
and System getEnv:. Preserve insertion order, same-name replacement, first
reverse alias, null bindings, enumeration and the strong roots of both halves.
Keep the existing address hash and independent state per Runtime.

The module supplies an immutable factory descriptor before Id initialization.
Preparing that descriptor constructs no Id objects or namespace state. The host
loads it explicitly beside the selected floor and attaches it before id_init;
Id constructs one opaque namespace after constructing its primordial heap,
vtables and Symbols. Normal module_init runs with the other native rungs and
must not reset state. There is no hidden loader or fallback inside Id, no normal
Rust dependency from Id to Namespace, and no shared process namespace between
independent test Runtimes. Destruction returns state to its owning module.

The 80-byte descriptor has create/destroy, bind/find/reverse, count/name_at and a
borrowed binding view. Operations are serialized native storage operations: no
Id allocation, sends, GC, callbacks or reentry. Read operations do not mutate.
The view is valid until mutation/destruction; Id copies roots synchronously.
This contract is needed by both GC observation and callback-free Block admission.
It does not change the existing replaceability of ordinary IdApi callbacks.

Id retains Symbol interning, owner-thread checks and its weak internal-dictionary
mirror. Existing imports still intern absent names. Binding still occurs before
the mirror update. Existing root collection, reverse-name Symbol borrowing,
Codegen enumeration and Block admission use the selected namespace state. No
methods, canonical identities, module policy, object/image formats or IdApi v7
fields change in this state extraction. The existing public entries remain
actual consumers; migrating those entries and their callers is the next distinct
boundary, not a new compatibility interface.

Before committing, compare native behavior with installed 81AV, verify independent
state, ordered roots, rebinding, aliases, nulls, early attachment, reload, GC,
reflection and namespace-vs-Globals distinctions. Test startup, source-free
compiled caller, saved v8 native Code image and baked executable in a fresh
external debug prefix. No release validation.

## Review constraints

- Do not delay canonical publication until after id_init or change its order.
- Do not call foreign/user code while Id holds a RuntimeState borrow; the selected
  storage descriptor must satisfy the restricted operations contract above.
- Do not copy Rust Vec/HashMap values across the ABI or free state in the consumer.
- Preserve null reverse lookup, null-valued enumeration slots and first-current
  alias selection after rebinding. Do not merge the raw namespace with Globals.
- The prepared library and descriptor remain pinned as long as any context exists.
  Descriptor validation and provider selection precede all floor initialization.
- Current native clients must prepare the dependency explicitly. Historical
  fixtures remain historical; only test baseline adapters may omit attachment
  when the old floor has no dependency export.

## Validation

- All seven existing namespace operation bodies and the address hasher are
  copied modulo whitespace. Runtime, public API and dispatch sources are
  unchanged. Id's Environment now owns only the selected descriptor and opaque
  context; Namespace owns its ordered bindings, index and destruction.
- The independent C oracle passes 2,048 ordered-binding cases through the actual
  floor. Complete output matches installed 81AV, development and the fresh
  installed prefix. Cases cover empty and non-UTF-8 names, absent-name interning,
  repeated bindings, aliases, null values, enumeration and out-of-range queries.
- Another 4,096 cases exercise two independent native contexts against a linear
  oracle, including null identities, borrowed binding views, destruction and
  continued use of the other context. All descriptor operations are required;
  malformed descriptors and retargeting fail, repeated attachment succeeds.
- A selected descriptor with counting wrappers proves one floor context is
  created and used for bootstrap bindings, lookup, reverse lookup and root views.
  Repeated module_init preserves that context and its bindings without invoking
  watched allocation, interning, method or root-registration callbacks. Dropping
  an extra library acquisition leaves the deliberately pinned descriptor usable.
- With Symbol compaction enabled and an actual bounded native-stack collection,
  the current exported object and both bound name Symbols survive. Rebinding the
  other export to null allows its old value to be reclaimed; its name remains
  rooted and enumerable. A zero stack-bottom disables collection in the existing
  runtime, so this fixture uses a real stack anchor and asserts reclamation.
- Host probes reject a missing library, missing provider, null descriptor, wrong
  version/extent, missing operation, missing floor dependency and rejected
  attachment before id_init. A successful probe observes canonical publication
  and subsequent binding through the new provider. There is no fallback in Id.
- Id tests pass (245, 15 ignored), as do Namespace tests (three, one ignored),
  ABI tests (four), host tests (86, one ignored), builder tests (four) and Namespace
  Clippy with warnings denied. The moved namespace tests retain independent
  Runtime and large rebinding coverage. Current native test clients build with
  their explicit pre-initialization attachment.
- Source Loader, Native Loader, Block Loop, numeric Error policies, Boolean,
  storage observation, storage tracing, live tracing and collection checks pass.
  Installed native comparisons also preserve File IO, Environment, Input,
  Integer/Float/Word operations, Error Value, Output, Text View, System GC and
  Printing. The independent IdApi v7 client, both Block-construction admission
  modes and all nine selected-floor discovery cases pass. All 145 stdlib
  assertions pass.
- Development and installed Namespace libraries match the three-symbol export
  inventory, relocatable install name and system-only linkage. Id gains only
  the Namespace dependency export; checked prior providers retain their exports.
  Id's normal Cargo dependency tree contains no Namespace implementation.
- The fresh external debug prefix has 43 native libraries and 28 compiled
  modules. Namespace is its sole added library; installed source metadata and
  all 71 required libraries match the refreshed inventory. All 1,734 source hashes
  remained unchanged through installation. Only this validation record changed
  afterward. No release validation or full install-smoke run was performed.
- Installed 81AV and the new prefix pass dynamic startup, a source-free compiled
  caller, a saved v8 native Code image and a baked executable. The fixture checks
  binding order, current reverse aliases, raw-null versus language-nil bindings,
  enumeration, retained identity, GC and native reinitialization. Raw publication
  remains separate from Globals. The Source Loader image fixture also passes,
  exercising real compiler staging and source evaluation with the new namespace.

Receipts are `.cargo-target/81aw-{check,host-check,unit,clippy,namespace,
namespace-baseline,namespace-native,namespace-installed,host-validation,
native-clients-build,prefix,prefix-native,image-baseline,image,source-loader-image,
stdlib,idapi-client,block-construction,collection,floor-discovery}.log`, focused
native regression logs, source hashes and the floor-discovery JSON receipt.

## Next boundary

Migrate the raw namespace's public entry points and actual consumers separately.
First review the per-runtime capability handoff and the current direct interner,
Symbol-name borrowing and weak-dictionary mirror primitives. Moving the existing
String-name operations must preserve their owner/phase checks and the absence of
user callbacks inside these primitive operations.

Account for live import/export/isExported/exportedNameOf replacements, numeric and
Block admission contexts, Codegen's selected-floor enumeration, and Image's use
of reverse names before removing any IdApi field or private inventory export.
Keep independent runtime contexts and existing source/image identity rules. Do
not replace the remaining consumers with an aggregate compatibility table, merge
Globals into this namespace, or combine the migration with Machine/control work.
