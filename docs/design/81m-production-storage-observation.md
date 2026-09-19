# 81M — Production storage observation

> **Status:** Implemented; focused checks passed. Follows Integer checkpoint `fb452df8`.
> This is the storage-observation prerequisite from 81H; tracing and collection
> remain separate extractions. Validation is focused, as requested by the user.

## Boundary decision

Three designs considered a compact callback-scoped snapshot, a streaming visitor
over current GC policy, and allocation-lifetime representation registration.
The snapshot plus explicit prepared description providers is selected. Streaming
legacy trace hooks would introduce calls on objects the collector might never
reach. Permanent representation registration would also require constructor,
clone, recovery, Image and retirement changes. Neither is necessary to expose
storage facts and validate owner-supplied descriptions now.

Id supplies an ordinary replaceable `withStorageView:` method. A narrow native
host entry verifies quiescence and permits one invocation through ordinary
lookup. The method copies facts, ends all runtime borrows, excludes Id activity
while the callback runs, and restores access on success or failure. Native
callbacks may use their own memory and the supplied view; ordinary Id calls and
captured mutators are rejected. This is a trusted native contract, not protection
against arbitrary machine-code writes.

The view distinguishes physical allocations from charged deferred allocations,
which have no Oop. Observation neither settles those journals nor marks or frees
storage. Each physical allocation has a never-reused runtime-local identity,
requested and padded extents, current behavior, permanence and finalizer/hook
facts. The existing four-word heap-registry bound is preserved: allocation
identity, padding, permanence and mark bits share one word. Object payload/header
bytes do not change. Existing `objectPayloadSize` continues to report padded
extent. Generic clone continues to request the original object's padded extent.

Core-root enumeration is shared with production GC. Compatibility-only Symbol
roots, strong owner edges, weak opaque dictionary keys and registration roots
keep their existing treatment. Core edges describe headers and registered
default-vtable storage; native method addresses are not object references.
No native stack scan or legacy root/trace callback is executed by observation.
Their presence is reported explicitly. Thus successful observation does not
claim a complete production GC root set or a complete object graph.

`birnam-storage` owns validation of prepared module descriptions, independently
of Id's implementation. Descriptions refer to the view's runtime and allocation
identities, exact object address and requested extent, with aligned strong-slot
offsets. Reassociation does not change those identities. Invalid targets,
duplicate owners, malformed extents and ignored sink errors reject the result.
Missing descriptions produce an explicit incomplete result when completeness is
requested. This layer validates facts; it does not compute reachability.

Providers establish ownership from their own construction knowledge. Looking up
a fresh allocation identity for a stale address does not establish ownership.
No current/birth vtable or mutable GC layout is inferred to be representation
authority. Production-wide provenance/provider migration is still required
before this interface can describe every upper payload. Native/code/Frame roots,
trace hooks and deferred identities remain explicit coverage gaps.

The observer, provider code and callback data stay pinned for their synchronous
calls under existing native-module lifetime rules. Borrowed arrays and sinks
expire at callback return. No IdApi field, image format, collector policy or
module-unloading mechanism changes.

## Focused evidence

The independent C fixture exercises the actual production Id and storage dylibs:
size/padding and clone facts, replaceable lookup, ordered error cleanup, module
descriptions after reassociation, malformed and missing descriptions, core roots
and edges, and captured-mutator rejection. Rust tests cover deferred accounting,
weak Symbol roots, runtime/allocation identity and preservation of GC marks.
No release-validation plan is run.

Final focused results: 245 Id tests, four ABI tests and one storage-library test
pass. The independent C probe passes, including eight malformed-provider cases,
foreign payload targets and 13 captured-mutator rejection subprocesses. The host
compiles; ABI/storage Clippy passes with warnings denied. The storage dylib has
only its three declared exports and no Birnam implementation dependency.
