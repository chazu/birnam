# Namespace factory v8

The floor requires `birnam_namespace_dependency_v8` before `id_init`. The host
pins `libbirnam_namespace.dylib` and attaches `birnam_namespace_provider_v8`'s
immutable 176-byte descriptor. All twenty operations are required. Invalid
descriptors and retargeting reject; repeating the same attachment succeeds.
The [independent C mirror](fixtures/namespace-v8/namespace_v8.h) fixes the layout.
Historical factories are not fallbacks.

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
descriptor borrows the service and does not extend its lifetime. Id destroys
services before their raw namespace storage.

Id exposes the live descriptor through `birnam_namespace_export_provider_v1`;
modules receive it explicitly with their API through five-argument native
initialization. IdApi v11 removes the old export slot. Internal byte-slice
bootstrap export remains private to Id. Language behavior and image encoding
are unchanged.
