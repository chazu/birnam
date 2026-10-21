# Prepared Alien modules and native packaging

Implemented and installed locally, 2026-10-21. The standard project builder now
supports `[[wrapped]].packaging = "adjacent"` (default) and `"embedded"`.
See [the authoring contract](../guide/09-shipping-artifacts.md#native-wrappers).

Prepared declarations use `Alien bind:signature:as:on:`. The compiler prepares
literal, unshadowed declarations through the existing authored AlienCompiler,
then embeds their object code. Source modules are never evaluated by this step.
At activation, Alien resolves process-local addresses and publishes/records
Methods through the same owners as ordinary runtime wrapping. Dynamic `wrap:`
sends retain their existing semantics. New wrapper scaffolds use `bind:`.

An adjacent wrapper declares and resolves native siblings relative to its own
artifact. This works for direct-file invocation as well as project execution and
does not alter the caller's fallback library directory. An embedded wrapper
carries native payloads in its initializer; the loader supplies all embedded
siblings before executing its body. Alien owns private materialization for the
OS loader, with normal-exit/DSO cleanup. Duplicate, conflicting logical library
identities fail visibly. External adjacent files remain caller-owned.

Embedded native payloads use an inert hex carrier, preventing a nested native
binary's own descriptor bytes from being mistaken for the containing artifact's
descriptor. The initializer's optional fifth field carries those payloads.
`embedded-native-hex-v1` marks the extension; old four-field artifacts remain
readable. `prepared-alien-v1` marks the compiler's declaration support and keeps
older compiled project artifacts out of incremental reuse. Embedded native
consumers conservatively rebuild on explicit build, including native-only edits
and packaging changes. Failed builds retain the previous publication.

The presentation REPL now uses `birnam build` / `birnam run`, with an embedded
terminal library selected in its manifest. Its custom Python preparation/cache
driver was removed. `play.sh --build` rebuilds after edits; ordinary `play.sh`
runs the published project. This follows the normal explicit-build contract.

## Validation

- `scripts/alien-packaging-test.py PREFIX`: both modes pass inert check/build/
  inspection, direct artifact entry, source-free relocation, native-only edits,
  packaging switches, failed-build preservation, GC, registration and cleanup.
  The embedded wrapper is relocated as exactly one file. Loading succeeds with
  language parsing, foreign-wrapper generation and C compilation disabled.
  Lexical shadowing and invalid-signature rejection are covered too.
- Workspace tests passed. The authored gate passed **854 assertions** plus its
  scratch-state, reflection, module, cell, LSP, globals and harness-owner checks.
- Native Artifact, artifact-activation and embedding tests passed; five native
  libraries retained their declared export ABI. Project-policy values passed
  the collecting-message integration test.
- Existing project checks passed build/run/install/test, relocation, path and
  pinned Git dependencies. The presentation REPL passed its **24 model checks**
  and real-PTY interaction/cleanup sequence using the normal installed command.
- The local runtime at `~/.local/opt/birnam` was rebuilt and installed. Packaging
  and presentation tests passed again against that installation. This is a
  local development delivery, not a release/tag/publication qualification.

Commands:

```sh
python3 scripts/alien-packaging-test.py ~/.local/opt/birnam
uv run scripts/presentation-repl-test.py --samples 1
./examples/presentation-repl/play.sh --build
```

Raw logs are retained under `.cargo-target/alien-packaging-*.log`; the
[compact receipt](../measurements/alien-packaging-20261021.json) records their
digests and the relevant source/installed identities.
