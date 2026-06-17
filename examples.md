# Birnam example apps — buildable today

Twenty small programs that exercise what's shipped (collections, strings,
datalog + rules + pull, reference cells with time-travel, File/System I/O, the
Result error model, FFI). Each is a REPL session or a short filed-in unit.

Run the REPL:

```
./zig-out/bin/birnam zig-out/lib/libbirnam_peg.dylib zig-out/lib/libbirnam_lang.dylib
```

Pass program args after `--`; read them with `[System arguments]`.

---

## Strings + files

1. **Word frequency counter.** Read a file, `split:` on spaces, fold word→count
   into a map. Sort the pairs by count, print the top N.
   *Features:* `File read:`, `split:`/`words`, `inject:into:`, maps, `sortBy:`.

2. **CSV column summarizer.** Read a `.csv`, `lines` then `split: ","`, pull one
   column, report `min:`/`max:`/sum/mean.
   *Features:* `File read:`, `lines`/`split:`, `asNumber`, number protocol.

3. **Markdown TOC extractor.** Read a `.md`, `select:` lines that `startsWith: "#"`,
   `format:` each into an indented bullet, write the result back out.
   *Features:* `File read:`/`write:to:`, `lines`, `select:`, `format:`.

4. **Config reader.** Read a `key: value` file into a map (`lines` → `split: ":"`
   → `trim` → `at:put:`). Look values up by key with an `ifAbsent:` default.
   *Features:* `File`, `split:`/`trim`, maps, `at:ifAbsent:`.

5. **Mad-libs / template filler.** Hold a template string with `{}` slots; prompt
   for each word via `System readLine`; `format:` the answers in.
   *Features:* `System readLine`/`println:`, string `format:`.

6. **Caesar cipher.** Walk a string's bytes with `at:`, shift each letter, rebuild
   with `,`. Encode/decode are the same method with a +/- shift.
   *Features:* string `at:`/`size`/`,`, `to:do:`, arithmetic.

7. **Anagram checker.** `asVector` both strings, `sort` the characters, compare for
   `=`. True iff same multiset of letters.
   *Features:* strings, `sort`, vector `=`.

8. **ASCII bar chart.** Given a map of label→count, print each row as a label plus
   a bar built by repeating `"#"` count times (`timesRepeat:` + `,`).
   *Features:* maps `do:`, `timesRepeat:`, string `,`, `format:`.

## Numbers + control flow

9. **FizzBuzz.** `1 to: 100 do:` with `mod:` tests; print `Fizz`/`Buzz`/`FizzBuzz`
   or the number.
   *Features:* `to:do:`, `mod:`, `ifTrue:ifFalse:`.

10. **Temperature converter.** Install `toF` / `toC` methods on the number vtable;
    convert a list of readings with `collect:`.
    *Features:* `method:do:`, number arithmetic, `collect:`.

11. **Prime sieve.** Build a range `2 to: n`, `select:` the values with no smaller
    divisor (`mod: ... = 0`), return the survivors.
    *Features:* ranges, `select:`, `detect:`, `mod:`.

12. **Grade book.** A vector of scores → mean (`inject:into:` / `count`), median
    (`sort` + middle `at:`), `min:`/`max:`, and a letter grade per score.
    *Features:* vectors, `sort`, number protocol, `collect:`.

13. **Shopping cart.** A map of item→price and a vector of chosen items; `|>` pipe
    the items through `collect:` (look up price) then `inject:into:` (sum).
    *Features:* maps, `collect:`, `inject:into:`, `|>` pipe.

14. **Roman numeral converter.** A vector of `value→glyph` pairs; greedily subtract
    while building the output string.
    *Features:* vectors, `whileTrue:`, string `,`.

## Reference cells + time-travel (the standout)

15. **Bank ledger.** A `db cell:` holds the balance; each transaction is `[acct
    set: newBalance]`. `[acct history]` is the statement; `[acct asOf: t]` is the
    balance at a past basis.
    *Features:* `db cell:`, `set:`, `history`, `asOf:`, `[[db now] basisT]`.

16. **Undo-able counter.** A cell counter incremented on each `System readLine`;
    "undo" reads `[c asOf: previousBasis]`. Time-travel gives undo for free.
    *Features:* cells, `value`/`set:`, `asOf:`, `System readLine`, loop.

17. **Change feed / timeline.** Take any cell, walk `[c history]`, and `format:`
    each entry as a `basis → value` line — a built-in audit log.
    *Features:* cells, `history`, `do:`, `format:`.

## Datalog (facts, rules, pull)

18. **Address book as facts.** Assert `[person :name …][person :city …]`; query
    `[db findAll: ?p where: #[ #[?p #city "Boston"] ]]`. Add a `pull:` spec to
    materialize each match into a record map.
    *Features:* `db findAll:where:`, `pull:spec:`.

19. **Family tree.** `defineRule: #ancestor` (recursive over `#parent`); ask for all
    ancestors of a person. Reverse-pull `#_parent` to list someone's children.
    *Features:* `defineRule:params:body:`, recursive rules, reverse-ref pull.

20. **Saved-query report.** `defineQuery:` a couple of named queries (e.g.
    `#adults`, `#localCustomers`) over your facts, then `[db run: #adults]` on
    demand — a tiny reusable report deck. Persist the whole DB with `:save`.
    *Features:* `db defineQuery:find:where:`, `db run:`, image `:save`/`:load`.

## Games (hotseat / deterministic — no RNG needed)

Birnam has no random primitive yet, so these take their "randomness" from
`[System arguments]` or are two-player hotseat (deterministic input).

21. **Number guessing.** Target comes from `[System arguments]`; loop on
    `System readLine`, compare with `<=>`, print `higher` / `lower` / `got it`,
    count tries.
    *Features:* `System readLine`/`arguments`, `<=>`, `whileTrue:`.

22. **Hangman.** Secret word from an arg; track guessed letters in a cell-held
    vector; render the masked word (`includes:` per letter); decrement a lives
    counter; win/lose on full reveal or zero lives.
    *Features:* strings `at:`/`includes:`, cells, `System readLine`, loop.

23. **Tic-tac-toe (2-player).** Board = a 9-slot vector; players alternate
    entering a cell index via `readLine`; check the eight win-lines after each
    move; declare winner or draw.
    *Features:* vectors `at:`/`with:`, `System`, `detect:`, control flow.

24. **Nim / 21.** A counter cell starts at 21; two players each remove 1–3;
    whoever is forced to take the last loses. `[c history]` replays the match.
    *Features:* cells `value`/`set:`/`history`, `System readLine`, `between:and:`.

25. **Rock-paper-scissors (2-player).** Each player types a move; a tiny map
    `{move → beats}` decides the round; first to 3 wins. Hide the second input by
    reading both before judging.
    *Features:* maps, string `=`, `System readLine`, loop, counters.

26. **Word ladder validator.** Given a start word, an end word, and a vector of
    steps, verify each step differs from the previous by exactly one letter and
    the chain reaches the end.
    *Features:* strings `at:`/`size`, vectors `inject:into:`, comparisons.

---

### Stretch (one extra, needs FFI)

- **libm calculator.** `Module load:` libm, `wrap: #sqrt: as: "sqrt" signature:
  "d)d"`, and expose `sqrt`/`pow` from the REPL — a hand calculator backed by C.
  *Features:* FFI `wrap:as:signature:`, floats.

---

## Useful Birnam modules

Reusable units — a filed-in `.brn` that installs methods/prototypes, or a new
grammar built on the `peg` engine. Each is small but earns its keep.

### M1. YAML grammar (PEG) — `Yaml parse:`

A `peg` grammar for a **flat subset** of YAML (block mappings + block sequences +
scalars; flow `[a, b]` / `{k: v}` as an easy add-on). Real YAML is
indentation-sensitive and not cleanly PEG-expressible; scope to one level of
block structure (the 80% that configs actually use) and note the limit.

Rule sketch (PEG productions — build with `peg_seq`/`peg_choice`/`peg_star`/
`peg_charset`/`peg_literal`/`peg_rule_ref`, the same combinators `lang.zig`
uses for the Birnam grammar):

```
document   <- ws (mapping / sequence) ws
mapping    <- pair (nl pair)*
pair       <- key ws ":" ws value
sequence   <- item (nl item)*
item       <- "-" ws value
key        <- bareword
value      <- flowMap / flowSeq / quoted / number / bareword
flowMap    <- "{" ws (pair (ws "," ws pair)*)? ws "}"
flowSeq    <- "[" ws (value (ws "," ws value)*)? ws "]"
quoted     <- '"' (('\' .) / (!'"' .))* '"'
number     <- "-"? digit+ ("." digit+)?
bareword   <- (!nl !":" !"," !"]" !"}" .)+        (trimmed)
nl         <- ws? "\n" ws?
```

Lowering: a `mapping` → an `{ }` Birnam map, a `sequence`/`flowSeq` → a `#[ ]`
vector, `quoted`/`bareword` → strings, `number` → SmallInt/Float. Returns native
Birnam data, so `[Yaml parse: text]` drops straight into `at:` / `do:` /
`collect:`. Pairs naturally with example **#4** (config reader) — replace the
hand-rolled `split: ":"` with a real parser.
*Features:* `peg` combinators, maps/vectors/strings, `File read:`.

### M2. JSON parser — `Json parse:` / `Json stringify:`

The sibling of M1 and a cleaner PEG fit (JSON isn't whitespace-sensitive).
`object`/`array`/`string`/`number`/`true`/`false`/`null` → Birnam
maps/vectors/strings/numbers/bool/nil. `stringify:` is the reverse, reusing the
`appendRef` formatter shape. Round-trips config and FFI payloads.
*Features:* `peg`, maps/vectors, `format:`.

### M3. CSV module — `Csv readFile:` / `Csv rows:`

Header-aware: first line becomes keys, each later line a row → a map keyed by
header. Handles quoted fields with embedded commas. `writeRows:to:` is the
inverse (`join:` with quoting). Turns examples **#1/#2** into one-liners.
*Features:* `File`, `split:`/`join:`, maps, vectors.

### M4. String-utils — `pad:`, `center:`, `repeat:`, `titleCase`, `slugify`

A filed-in batch over the byte primitives that the core string protocol doesn't
ship: `repeat:` (build via `timesRepeat:` + `,`), left/right `pad:` to a width,
`center:`, `capitalize`/`titleCase` (split on space, upper the first byte),
`slugify` (lowercase + replace non-alphanumeric runs with `-`).
*Features:* strings, `timesRepeat:`, `split:`/`join:`, case folding.

### M5. Stats — `Stats mean:`/`median:`/`mode:`/`stddev:`

Numeric summaries on a vector: `mean`/`median` (`sort` + middle), `mode` (count
map → max), variance/`stddev` (two-pass over `inject:into:`), `percentile:`.
Backs grade-book (**#12**) and CSV summaries (**#2**).
*Features:* vectors, `sort`, `inject:into:`, `sqrt`, maps.

### M6. Tiny test framework — `describe:` / `it:` / `assert:equals:`

Self-hosted xUnit-lite: `it:` runs a block, catches errors with `on:do:`, tallies
pass/fail; `assert:equals:` raises a kinded error on mismatch; a final summary
line. Lets the example apps above carry their own tests.
*Features:* blocks, `on:do:`/`error:`, counters, `format:`.

### M7. Logging — `Log info:` / `warn:` / `error:`

Leveled logging to stderr (`System error:`) and/or a file (`File append:to:`),
each line `format:`-stamped with a level tag (and a basis-T from `[[db now]
basisT]` as a monotonic clock until a real time source lands). A level threshold
gates output.
*Features:* `System`/`File`, `format:`, `db now`.

