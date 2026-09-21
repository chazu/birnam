# Namespace factory v8

The 176-byte descriptor describes Namespace's storage and service operations.
All twenty operations are required. Its layout is fixed by the
[independent C mirror](fixtures/namespace-v8/namespace_v8.h).
Production constructs the [Namespace owner](fixtures/namespace-owner-v1/README.md)
through Id after roots, Lifetime and base values. Runtime later adopts its
completed storage and operations. Id has no Namespace prerequisite.
The old provider and dependency-setter exports are removed; historical factory
versions are not fallbacks. The Rust factory helper remains for local unit support.

The first eighteen operations retain [v7](namespace-v7.md)'s offsets and
contracts. `create_export` at offset 160 and `destroy_export` at 168 create and
destroy the runtime-owned [native export service](namespace-export-v1.md):

```c
NamespaceExportV1 *create_export(void *storage, BindV1 bind,
                               PublishV1 publish, const NamespaceExportFloorV1 *floor);
void destroy_export(NamespaceExportV1 *owned_service);
```

Creation copies the immutable 48-byte FloorV1: version, extent, opaque context,
access, interner and mirror callbacks. It retains the selected storage, bind
and publish operations. Null storage, missing operations/floor, wrong floor
version/extent or missing floor callbacks return null without invoking callbacks.
The floor context may be null when its callbacks permit it. All selected code,
contexts and storage remain pinned until destruction.

Null-name checking and C-string scanning precede floor access. Canonical export
then admits Id access, interns, binds and mirrors. The returned bind result is
independent of the original value passed to mirror; either may be null.
The service retains no transient RuntimeState context or shared state borrow.

Each runtime owns a separate allocation. Replacing its public function/context
changes invocation without transferring ownership. Destruction takes the
original owned address even if the public fields have changed. A copied public
descriptor borrows the service and does not extend its lifetime. Services must
be destroyed before their underlying namespace storage.

Runtime creates the upper export service using the completed Namespace storage.
Bootstrap selects it through Id's Runtime native entry and passes it explicitly
to consumers. Id does not expose a namespace provider or export slot. Language
behavior and image encoding are unchanged.
