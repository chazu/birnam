# Presentation REPL experiment

An opt-in REPL over the installed Birnam, with the presentation model, editor,
history, paging and viewport policy written in Birnam. A small C alien reuses
the existing Termbox2 adapter for terminal mechanics. The ordinary Host still
owns evaluation turns, roots, checkpoints and cleanup. No core CLI or default
REPL changes are needed.

## Try it

From the repository root, in a real terminal on macOS:

```sh
./examples/presentation-repl/play.sh
```

The launcher uses the standard `birnam build` and `birnam run` workflow. On first
use it builds the project. Subsequent launches run the last successful publication;
use `./examples/presentation-repl/play.sh --build` after source changes.
Set `BIRNAM=/absolute/path/to/birnam` to choose a different installation.
This needs a runtime with prepared Alien bindings and embedded packaging support.
The manifest selects `packaging = "embedded"`; change it to `"adjacent"` and
rebuild to publish a native sibling. The publication lives under `.birnam/build`.
The working directory is `examples/presentation-repl`; the system C compiler is
needed for explicit builds. The former Python preparation/cache driver is removed.

Enter these two forms:

```birnam
let cell := [Cell with: 41]
#[cell 7]
```

Press **F2** to expand the vector, **Down** to select its first child, then **F4**
to insert a reference. The prompt now contains `[Presentations valueAt: 3]`.
Edit it to `[[Presentations valueAt: 3] value]` and press Enter: the result is `41`.
`[cell == [Presentations valueAt: 3]]` answers `true`: the child is the actual
object, including across collection, not a reparsed display string. IDs depend
on what you evaluated and expanded earlier; use F4 to insert the current one.

Try `[0 to: 999999]` and F2. Only twenty children are requested. Repeated F2 on
the parent advances the page and wraps after the final page. F3 collapses it.
Cells and nested vectors/ranges can be expanded in the same way.

| Key | Action |
| --- | --- |
| Enter | Evaluate; continue on a new line if brackets are incomplete |
| Ctrl-J | Insert a newline, shown as `↵` in the compact input row |
| Left/Right, Home/End, Ctrl-A/E | Move the input cursor |
| Backspace/Delete | Delete one Unicode codepoint |
| Up/Down | Select transcript rows and scroll |
| F2 / F3 | Expand or advance a page / collapse |
| F4 | Insert the selected object's session reference |
| F5 | Forget transcript and retained values |
| Ctrl-P/N | Recall submitted source history |
| Ctrl-C | Clear the current draft while idle |
| Ctrl-L | Redraw |
| Ctrl-D on empty input, or `:quit` | Exit and restore the terminal |

Some keyboards require Fn with the function keys. Bind a selected value to a
normal Birnam name to keep it beyond transcript eviction, for example
`let saved := [Presentations valueAt: 3]`.

## Contract and scope

- The transcript retains at most 256 rows and 256 value handles. IDs increase
  monotonically within the session; expiration answers `#expiredPresentation`
  and never aliases a newer value. F5 does not clear ordinary globals or source
  history. Retention bounds count references, not the size of their object graphs.
- Collapsing removes visible children but keeps their handles until eviction.
  Expanding reads the current live children; already displayed summaries remain
  snapshots. Nesting stops at depth eight, including cycles.
- Presenters implement `presentationSummary`, `presentationSize`, and
  `presentationChildAt:` through ordinary methods. Unknown objects have a cheap
  fallback. Vector, Range and Cell support expansion; Map currently has only a
  summary. Rendering and selection never invoke presenters or evaluate source.
- Source history keeps 64 forms, input is limited to 4096 UTF-8 bytes, and result
  summaries are clipped. stdout and stderr merge into the transcript after
  evaluation, retaining the first 16 KiB. Capture uses a temporary file to avoid
  a full pipe deadlocking synchronous evaluation; temporary disk use during the
  evaluation is not bounded by that retention limit.
- Content changes rebuild a cached text packet. Selection redraws two rows;
  scrolling blits the cached packet in one alien call. The native adapter sees
  strings and coordinates, never Birnam objects or their memory layouts.
- This slice uses synchronous evaluation. Ctrl-C does not interrupt a running
  form. The editor moves by codepoint, not grapheme cluster; combining marks are
  shown with a placeholder. It has no completion, undo, mouse interaction,
  general slot browser, remote transport or portable presentation references.
  Image recovery of an active presentation session is not qualified.

## Validation and measurements

**Historical prototype startup:** preparing the code reduced the measured median from **4.63 s
to 3.18 s** (31%); ordinary Birnam startup was 2.93 s in the same series. See the
[startup profile and acceptance](../../docs/archive/2026-10-runtime-refactors/presentation-repl-startup.md).
Initial preparation is paid once when inputs change; these launch times exclude it.

From the repository root:

```sh
birnam examples/presentation-repl/test.birnam
uv run scripts/presentation-repl-test.py --samples 3 \
  --output .cargo-target/measurements/presentation-repl/current.json
uv run scripts/presentation-startup.py --samples 3
```

The PTY harness uses pinned `pyte` to verify actual screen contents. It checks
identity reuse through evaluation and GC, paging, source errors and Error values,
stdout capture, Unicode editing, multiline input, resize, expiration, a full
transcript, normal exit and Host-fault terminal restoration. The native capture
probe also checks output larger than the retained buffer and nested capture
rejection. Headless tests exercise cycles, presenter failures, bounds and history.

Timings measure a key sent to a controlling PTY through the updated screen bytes
observed by its reader. They include the Python reader's overhead, but do not
measure a graphical terminal's painting latency. Fresh processes use a warm
filesystem. Startup is measured separately from editing and expansion, against
the same installed binary's ordinary console startup. The startup harness now
measures the standard project workflow. `scripts/alien-packaging-test.py PREFIX`
checks compiler-free module loading, relocation and both packaging choices.

The final receipt records the machine, binary/bootstrap hashes, source hashes,
sample counts and timings. This is local prototype evidence, not a release or
cross-platform qualification.

The following table preserves the **initial source-launch baseline**, before
startup preparation was added. Measured on macOS arm64 with installed Birnam
0.18.0, three fresh presentation
sessions and three ordinary-console starts. The native library was already built.
The [receipt](../../docs/measurements/presentation-repl-20261021.json) includes
all raw samples; no timing outliers were removed.

| Operation | Samples | Median | p95 |
| --- | ---: | ---: | ---: |
| Typing, empty transcript | 180 | 3.98 ms | 5.71 ms |
| Typing, expanded transcript | 180 | 3.95 ms | 5.14 ms |
| Typing, full 256-row transcript | 180 | 4.24 ms | 4.89 ms |
| Move selection within viewport | 180 | 5.59 ms | 7.89 ms |
| Scroll full transcript | 159 | 5.35 ms | 6.33 ms |
| Expand 20 of 1,000,000 items | 3 | 97.12 ms | 118.76 ms |
| Presentation source startup | 3 | 5.11 s | 7.60 s |
| Ordinary REPL startup | 3 | 3.34 s | 9.57 s |

The editing/navigation target of 16 ms p95 was met in that baseline pass. Expansion
and source startup remain perceptibly slower. Startup varied substantially, and
with three samples its p95 is simply the maximum; this is not a precise startup
regression estimate or a latency guarantee. Earlier development passes also
showed occasional typing spikes around 32 ms. A full viewport redraw on every
selection initially cost about 38 ms p95; repainting two rows and blitting a
cached transcript for scrolling removed that observed bottleneck without moving
presentation policy into C.

Acceptance: **24 headless assertions**, **three full PTY sessions**, native
capture checks, C-locale Unicode fallback and a forced Host-fault cleanup passed.
The existing **26 REPL golden checks** also passed, including their ordinary
image round trips. These image checks do not qualify saving this experimental
presentation session. The wider runtime/release suite was not rerun for this
isolated example.
