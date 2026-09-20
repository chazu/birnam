# Vtable enumeration v2

[81BH](../docs/design/81bh-production-selector-enumeration.md) extends
[v1](vtable-enumeration-v1.md) with direct ordered-key observation. Object Model
owns count and checked selection; Id owns its registry and dictionaries. All
views are transient, unrooted borrows. There is no send, allocation, snapshot or
additional root. Selection observes the current dictionary on every call.

The [independent C mirror](fixtures/vtable-enumeration-v2/vtable_enumeration_v2.h)
defines these arm64 macOS layouts:

| Descriptor | Size | Operations after version/extent/context |
| --- | --- | --- |
| Factory v2 | 32 | create at 16, destroy at 24; no context |
| Floor v2 | 40 | registry view at 24, selectors at 32 |
| Capability v2 | 48 | count at 24, at at 32, selector_at at 40 |
| View v1 | 16 | entries at 0, count at 8; no version/extent |

`selectors(context, vtable)` first performs owner/observation admission and
exact registry lookup, then returns the ordered key borrow. Invalid vtables
fail before bounds checking, even for SIZE_MAX. Object Model immediately copies
the selected identity or returns null past the end. Replacement retains key
position; removal shifts successors; re-addition appends. Parent dictionaries,
custom lookup and weak-key filtering are not consulted. Root callbacks may
observe; storage-observation callbacks retain their existing refusal.

The host pins Object Model and attaches `birnam_vtable_enumeration_factory_v2`
through `birnam_vtable_enumeration_dependency_v2` before Id initialization.
Factory validation/creation are inert and reject missing operations or incorrect
version/extent. Same-pointer attachment repeats; retargeting rejects. Each
Runtime owns its service, copies the floor and saves its canonical capability
for pure optional Block-constructor admission. Destroy uses the original owned
address even after public context replacement; descriptor copies borrow its
lifetime. Ordinary Object Model method installation stays at its original point.

Id publishes `birnam_vtable_enumeration_provider_v2`. Native initialization
still receives `(api, prior_state, presence, imports, exports, vtables)`;
[Native Loader v6](native-loader-v6.md) forwards the saved matching capability.
Each operation copies callback and context before invoking either. Consumers
retain it at the same point as their execution API. Replacing a shared context
requires callbacks that accept that context; an active call retains its selected
pair. No API-address registry or aggregate replacement facade is introduced.

IdApi v13 has 27 fields / 216 bytes. Native consumers must rebuild; image
encoding is unchanged. [Block construction v5](native-block-construction-v5.md)
compares the canonical selector function and context before any call, declining
if either differs while ordinary construction retains live dispatch.

Run `mise run vtable-enumeration`, `mise run selector-enumeration`, and
`scripts/selector-enumeration-image-smoke.sh PREFIX` for focused checks.
