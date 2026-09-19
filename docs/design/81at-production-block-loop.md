# 81AT — Block Loop owns the native while methods

> **Status:** Implemented, following File IO in 81AS.

Move whileTrue:, whileFalse: and their shared loop body unchanged from Prelude
into Block Loop. Preserve fresh value sends, exact Boolean identity, condition
and body Error recognition, short-circuit order and current nil. Do not introduce
a scheduler, stackless lowering, cached selectors or new loop semantics.

Use the existing 24-byte installer with registration/execution tables, selected
Block behavior and both already interned selectors. Prelude retains Block
identity, capture layout and creation/adoption, and supplies its first execution
pointer before either registration callback. Keep selector/behavior snapshots
synchronous and unretained. Block Loop depends only on Error Value; initialization
creates no Id objects, roots or methods. No IdApi or image encoding changes.

Full startup subsequently installs the authored stackless-control loops. Preserve
that ordering and replacement. Compare direct native callback traces against
installed 81AS, including registration reentry, live Boolean/Error tables, fresh
lookup, condition/body evaluation counts, nested loops, GC and reload. In the
startup/compiled-module/saved-image/baked fixture, reload Prelude to exercise the
native methods explicitly and verify their owning library. Also run the normal
Block suite with the authored stackless definitions. Use a fresh external debug
prefix; no release validation. Commit this extraction separately.

## Validation

- Both methods, the shared loop body and argument/intern/send helpers are copied
  verbatim. Every other Prelude function body is unchanged except replacing the
  two Block method bindings with installation. Block creation/adoption, identity,
  capture layout, selector ordering and original execution selection remain.
- An independent C oracle passes 980 evaluation/recognition cases in each of
  three modes: normal, first whileTrue:-intern reentry and first whileTrue:-binding
  reentry. Complete traces match installed 81AS, development libraries and the
  fresh installed prefix. The matrix checks both loop directions, zero through
  eight initial iterations, exact and inherited Errors, null/non-Boolean values,
  arbitrary body results, export presence and null/live Error behaviors.
- Counts and short-circuit order match an independent sequence evaluator. Wrong
  arity and null carriers skip all callbacks; a terminating condition skips a
  null body. Non-Block receivers/operands still dispatch value. The body argument
  is copied before the first callback, including mutation of its original carrier.
- Boolean identity remains live through Error recognition. Shared true/false
  identity retains the native loop's true-comparison precedence. Current nil
  remains live after body recognition, and exact Error recognition compares the
  imported behavior snapshot even when its export changes during the callback.
- Each condition/body send interns its current selector. Replacing either value
  method changes the next relevant send; custom condition lookup is observed on
  every iteration. Nested loops and nested error construction survive callback
  GC. Missing/null/replaced Error factories, null/alias messages and results,
  current constructor/context, publication during message creation, imported
  factory snapshots and custom factory lookup retain the existing policy.
- Methods execute during registration. Reentry uses Prelude's first execution
  table, including before System exists; assigning System's vtable owner still
  uses the current registration table. Copied Methods survive replacement, reload
  and direct installation. First-binding mutation preserves the second selector
  and Block behavior; the next installation adopts the new selections. Precise
  GC preserves retained values.
- Admission rejects null, wrong version/extent, missing install and retargeting;
  repeat attachment succeeds. Initialization adds no Id allocations, methods or
  roots. Development and installed libraries match the four-symbol inventory,
  relocatable install name and system-only linkage. Prelude adds only Block Loop
  attachment; the checked prior provider export inventories are unchanged.
- Focused File IO, Environment, Input, Integer arithmetic/error/admission, Float
  and Word numeric/error, Error Value, Output, Text View, System GC, Printing and
  Boolean checks pass, including installed comparisons against 81AS. The normal
  authored Block suite passes all 20 assertions in both prefixes; all 145 stdlib
  assertions pass. ABI tests (four), Prelude identity (one), host tests (86, one
  ignored), builder tests (four) and Block Loop Clippy with warnings denied pass.
- The fresh external debug prefix contains 40 native libraries and 28 compiled
  modules; Block Loop is its sole added library. Installed metadata and all 68
  required libraries match the refreshed inventory. All 1,694 source hashes were
  unchanged during installation; this validation record was added afterward.
  No release validation or full install-smoke run was performed.
- Installed 81AS and the new prefix pass native startup, source-free compiled
  module, saved v8 native Code image and baked executable checks. The fixture
  explicitly reloads Prelude because normal startup replaces its native loops
  with authored stackless control. An independent C guard verifies both Method
  addresses belong to the expected native library in every execution path.
  Native 10,000-iteration loops, both directions, short-circuit behavior, captured
  Blocks/Cells and aliases, retained Error identity, precise GC and fresh value,
  loop and Error factory replacements preserve the baseline results.

Receipts are `.cargo-target/81at-{check,compare,block-loop,unit,clippy,file-io,
environment,system-input,integer-error-value,float-error-value,word-error-value,
float,word,error-value,system-output,text-view,system-gc,printing,boolean,prefix,
prefix-native,image-baseline,image,block-suite-baseline,block-suite,stdlib}.log`,
source hashes and baseline/development/installed traces for each reentry mode.

## Next boundary

Extract System loadModule: into Native Loader. Preserve exact borrowed String
access followed by its owned CString, RTLD_LAZY, live dlopen/dlsym callbacks,
existing errors, original execution-table pointer passed to module_init, null
state, ignored initializer result and receiver return. Keep handle lifetime and
callback reentry unchanged. This existing direct loader path must not gain the
host's dependency admission behavior. Leave source loading/evaluation separate.
