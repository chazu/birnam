# Review of the draft Birnam style guide

> Review of `style-guide-DRAFT.md` at `468f2e0`, 2026-07-17.

## Overall view

This is a strong draft and worth turning into the canonical style guide after a
revision pass. Its best idea is genuinely specific to Birnam: visual density is
unavoidable, but rightward drift is not. That gives the guide a coherent reason
for its rules instead of presenting a bag of formatting preferences.

I would not adopt it unchanged. The readability advice is mostly sound, but a
few useful heuristics are stated as universal laws, and the performance section
makes stronger claims than its current proof supports. The guide will be more
credible if it separates rules required by syntax or runtime semantics from
heuristics that should yield to local clarity and measurement.

## What works well

### It has a Birnam-specific theory of readability

“Fix the drift” is memorable and explains the common failure mode of a language
where sends and blocks share brackets. The before/after examples make that
theory immediately actionable. This is much better than importing a generic
Smalltalk or functional-language guide and changing the examples.

The distinction between examples and modules is similarly good. Teaching code
should carry narrative scaffolding; library code should carry contracts and
only the non-obvious reasons. Keeping both standards in one guide makes their
difference explicit rather than letting the tree develop two undocumented
styles.

### The guide is grounded in the actual repository

The Digla and Minesweeper examples expose real problems rather than invented
toy smells. The stale `whileDo` comment is an especially effective example of
why style includes knowing the current language floor, not merely arranging
whitespace.

The rules around `doc:`, early `docModule:is:` registration, persistent
collections, and native loop primitives encode runtime facts that readers can
otherwise learn only through failure. Those deserve strong wording.

### It offers transformations, not just criticism

The cheat sheet, keyword-message layout, conditional-arm layout, and worked
rewrite all tell an author what to do next. “This is ugly” is not actionable;
“bind the two predicates and name the combined intent” is.

The hierarchy is also useful: a small set of principles first, concrete layout
conventions second, then a compact lookup table. With editing, this can serve
both as an introduction and a reference.

### Shipping an executable proof is exactly the right instinct

`style-proof.birnam` is a major positive. Style guides frequently smuggle in
performance folklore; this one at least makes the claim reproducible and checks
semantic agreement before timing. Keeping the proof beside the prose also
makes future revisions possible when dispatch, maps, or the compiler change.

## What needs revision

### The map rule is too absolute

“Three or more is a table, full stop” and “map over cascade always” are stronger
than the semantic rule actually established. A table is a very good fit when:

- every arm compares the same value to constants;
- the arms are independent values or nullary actions;
- lookup order is irrelevant; and
- constructing or capturing the table does not change lifetime or error
  behavior.

Conditionals remain reasonable for ordered predicates, ranges, dependent tests,
deliberately likely fast paths, or arms whose closures would capture changing
state. A table of blocks can also hide control flow rather than clarify it.

I would phrase this as a strong preference with a semantic checklist. Keep the
three-arm threshold as a review smell, not a law.

### The benchmark does not support “never slower”

The proof supports the important conclusion that a hoisted table is much faster
for this method. It does not establish that an inline map is never slower.

As a diagnostic, I ran three shortened trials with 4,000 repetitions, or 36,000
method calls per variant. The rounded results were:

| Variant | Trial range |
|---|---:|
| nested conditional | 54.7–55.8 us/call |
| inline map | 58.2–59.2 us/call |
| let-hoisted map | 28.9–29.4 us/call |
| Globals-hoisted map | 38.7–39.5 us/call |

The inline map was consistently about 6–7% slower in those runs. That is close
enough to support “roughly comparable on this workload,” but it directly
contradicts “never slower.” One machine, one method, one key distribution, and
one build profile cannot justify a universal performance claim either way.

A full-size recheck on 2026-07-19 passed the semantic agreement check and
reported 64.0 us/call for the nested conditional, 57.9 us/call for the inline
map, 29.8 us/call for the `let`-hoisted map, and 41.8 us/call for the
Globals-hoisted map. The inline result reversed the shortened trials above.
That does not invalidate either run; it demonstrates why a single ordered
sample cannot support “never slower” and strengthens the case for repeated,
rotated measurements.

The proof should gain warm-up, multiple samples, median/minimum reporting,
explicit GC treatment, and rotated or randomized variant order. The prose
should say that readability motivates the inline form on cold paths; measured
performance motivates a verified hoisted form on hot paths.

### The preferred hot-path form still has an unresolved correctness question

The draft correctly admits that file-level `let` capture has not been verified
after compile-to-dylib and reload. That caveat is not peripheral: it applies to
the form the guide calls both fastest and cleanest.

Resolve that experiment before making let-hoisting canonical for compiled
modules. Until then, the safe rule is:

- use an inline table where the path is cold and clarity wins;
- use a Globals-backed table when stable runtime ownership matters; and
- use file-level capture only where its compiled-unit lifetime has been tested.

### The dispatch-cost story conflicts with the existing idioms guide

The draft says method dispatch is about 22 us in this benchmark, while
`02-idioms.md` says one send costs roughly 1.5–2 us. Both numbers may describe
different operations—the proof includes block invocation, argument plumbing,
cell writes, and the measured method—but the current wording calls both “method
dispatch” or “one send.” Readers will reasonably treat them as contradictory.

The proof does not currently contain the separate dispatch-only measurement
claimed by the prose. Add that case and name exactly what it measures, or remove
the 22 us claim and refer to the shared fixed overhead more cautiously.

### Some heuristics should not become hard lints

The three-opening-bracket rule and one-primary-decision rule are valuable review
prompts. Enforcing them mechanically would sometimes create names that carry no
meaning, fragment cohesive logic across methods, and add dispatch/navigation
cost solely to satisfy a count.

Likewise, exact alignment and continuation indentation are formatter concerns,
not semantic lint failures. The future tooling boundary should be:

- formatter: indentation, keyword wrapping, arm alignment, terminators;
- linter errors: syntactically invalid bare-literal brackets and missing
  required documentation registration;
- linter warnings: deep drift, repeated predicates, nested primary decisions,
  and inline tables in measured hot loops.

### The performance appendix overwhelms the style guide

Part 5 is valuable, but it is long and likely to age faster than the layout
rules. Move the benchmark protocol, raw numbers, and compiled-capture experiment
to a short companion note or benchmark README. Keep the style guide's durable
conclusion and link to the evidence. The executable proof should remain.

There is also a small editorial error: the cheat sheet assigns hot-table
hoisting to “Rule 5,” although the guide defines four cardinal rules and Part 5
is not introduced as a fifth rule.

## Answers to the draft's open questions

1. **Keep one guide.** The altitude distinction is one of the draft's strengths.
   Split only the volatile performance evidence into a companion note.
2. **Give tooling selective teeth.** Automate objective layout and runtime
   contract checks. Keep decomposition counts as warnings until repository-wide
   experience shows a low false-positive rate.
3. **Retain maps as a strong preference, not a universal performance theorem.**
   Verify compiled closure lifetime and strengthen the benchmark before making
   the hot-path recommendation categorical.
4. **Keep `|>` optional.** It is excellent when named linear stages become
   clearer top-to-bottom. Making it the default would replace one kind of
   punctuation density with another and introduce blocks where a direct send is
   already clear.

## Recommended path to adoption

1. Separate hard runtime/syntax rules from judgment-based readability smells.
2. Soften the universal map and branch-count wording.
3. Reconcile the dispatch-cost claim with `02-idioms.md`.
4. Verify file-level captures through build, dylib load, and a fresh process.
5. Harden `style-proof.birnam` into a repeated benchmark and move detailed
   performance evidence beside it.
6. Fix the Rule 5 reference, then remove `DRAFT` and adopt the guide.

The draft's direction is right. These changes would preserve its clarity and
confidence while making that confidence rest on rules and evidence strong
enough to survive changes in the compiler and runtime.
