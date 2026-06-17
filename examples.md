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

---

### Stretch (one extra, needs FFI)

- **libm calculator.** `Module load:` libm, `wrap: #sqrt: as: "sqrt" signature:
  "d)d"`, and expose `sqrt`/`pow` from the REPL — a hand calculator backed by C.
  *Features:* FFI `wrap:as:signature:`, floats.
