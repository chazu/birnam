# System GC controls through Id messages

Bootstrap loads System GC with Namespace after System and Integer exist. The
module installs the existing `gc`, `liveObjects`, `autoGc:` and `preciseGc:`
methods. Repeat loading preserves the installed methods and current settings.

| System selector | Behavior |
|---|---|
| `gc` | Send GC `collect`, then Integer `fromWord:` with the reclaimed count. |
| `liveObjects` | Send Id `allocationCount`, then Integer `fromWord:` with the current physical allocation count. This does not trigger collection. |
| `autoGc:` | Accept the selected true/false objects, send GC `automaticCollection:` with Word 1/0, return the receiver. |
| `preciseGc:` | Accept the selected true/false objects, send GC `precisePayloads:` with Word 1/0, return the receiver. |

Wrong arity or a non-Boolean language setter argument returns nil. Native failures
propagate unchanged. Count conversion selects the current Integer method after
the query returns and preserves the native count's bits, including negative i64
representations. Receivers and arguments remain rooted across dependency sends.
There is no native GC-settings table, callback, or separate settings service.

GC owns the settings, initially automatic enabled and precise payloads disabled.
Disabling automatic collection preserves accounting and explicit collection.
Re-enabling it does not collect immediately; the next ordinary allocation or
`prepareAllocation` checks the existing budget. The current one-MiB default and
`budget:` tuning remain unchanged.

Payload precision and native roots are independent. `preciseGc: true` makes
representation owners use declared references; false also checks their own raw
payload words for exact allocated addresses. Unknown ordinary slot layouts
remain conservative. Both modes retain required references in native side storage.
String bytes and arbitrary Rust memory are not scanned. Ordinary `gc` still
captures native stack/register roots in both modes. GC `collectPrecisely` remains
the native declared-roots-only operation; it does not select payload precision.

Settings can change during tracing/finalization. Each representation owner sends
`precisePayloads` before scanning an object's payload; a hook can change later
objects' treatment. Object Model permits layout replacement during tracing without
admitting language allocation. All enumeration continues through `referencesOf:do:`
and `visit:` sends; a failure prevents sweep.

Run `bash scripts/native-message-test.sh`. The `system_gc` integration binary
covers System/source sends, counts and constructor replacement, setter replacement
and native failures, both payload modes, numeric private behaviors, Range and
String/Builder payloads, live mode/layout changes, failed-trace accounting,
native-stack survival, automatic suppression and reloads. Other production memory
and execution tests continue to cover finalizers and code/continuation lifetime.
Full Host/CLI and Image integration remain separate work.
