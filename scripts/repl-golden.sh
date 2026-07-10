#!/usr/bin/env bash
# repl-golden.sh — end-to-end golden tests for the console REPL.
#
# Pipes scripted stdin into the built binary and diffs the full transcript, so
# the REPL's user-visible behavior is LOCKED before (and across) implementation
# changes — prompts, continuation, value/error printing, commands, the
# stdout/stderr split, and exit codes. Run from anywhere:
#
#   ./scripts/repl-golden.sh          # 0 all-green, 1 any mismatch
#
# The transcripts are the spec. If a case fails after an intentional behavior
# change, update the golden HERE in the same commit and say why.
set -u
cd "$(dirname "$0")/.."
BIN=${BIRNAM_BIN:-./zig-out/bin/birnam}
BANNER='birnam — an interactive image. :help for commands, :quit (or Ctrl-D) to leave.'

pass=0 fail=0

# check <name> <expected> <actual> — count + show a diff on mismatch.
check() {
  if [ "$2" = "$3" ]; then
    pass=$((pass + 1)); echo "ok   $1"
  else
    fail=$((fail + 1)); echo "FAIL $1"
    diff <(printf '%s\n' "$2") <(printf '%s\n' "$3") | sed 's/^/       /'
  fi
}

# ---- value printing: expression, let-persistence, nil --------------------------
actual=$(printf 'let x := 40\n[x + 2]\nnil\n:quit\n' | "$BIN" --repl 2>&1)
expected="$BANNER
birnam> 40
birnam> 42
birnam> nil
birnam> "
check "print: values, let persists across lines, nil special-case" "$expected" "$actual"

# ---- multi-line: a form accumulates until brackets balance ---------------------
actual=$(printf '[1 +\n2]\n:quit\n' | "$BIN" --repl 2>&1)
expected="$BANNER
birnam>    ... 3
birnam> "
check "multiline: [ continues with '   ... ' then evaluates" "$expected" "$actual"

actual=$(printf '[{"a" -> 1} size]\n:quit\n' | "$BIN" --repl 2>&1 | sed -n 2p)
check "scanner: { and } balance inline" "birnam> 1" "$actual"

# ---- scanner: brackets inside strings don't count ------------------------------
actual=$(printf '["a ] b" size]\n:quit\n' | "$BIN" --repl 2>&1)
expected="$BANNER
birnam> 5
birnam> "
check "scanner: brackets inside a string literal are ignored" "$expected" "$actual"

# ---- scanner: // comments are skipped (no continuation despite the stray [) ----
# stdout only: the immediate parse failure goes to stderr; the ABSENCE of a
# '   ... ' prompt is what proves the comment's bracket was not counted.
actual=$(printf '[7] // [\n:quit\n' | "$BIN" --repl 2>/dev/null)
expected="$BANNER
birnam> birnam> "
check "scanner: brackets inside a // comment are ignored" "$expected" "$actual"

# ---- scanner: over-closed counts as done (evaluator reports the error) ---------
actual=$(printf 'a]\n:quit\n' | "$BIN" --repl 2>/dev/null)
expected="$BANNER
birnam> birnam> "
check "scanner: over-closed form evaluates immediately" "$expected" "$actual"

# ---- commands: help, unknowns are exact-match, quit aliases --------------------
actual=$(printf ':h\n:frotz\n:quit extra\n:q\n' | "$BIN" --repl 2>&1)
expected="$BANNER
birnam> Commands:
  :help        this message
  :globals     list the bindings you've defined
  :quit        leave the REPL  (or Ctrl-D)
Everything else is birnam: an expression prints its value; \`let x := …\`
binds a name that persists across lines; \`[Reflect defineMethod:…]\` extends
the live system. Multi-line forms continue until the brackets balance.
birnam> unknown command (try :help)
birnam> unknown command (try :help)
birnam> "
check "commands: :h help text, unknown + ':quit extra' rejected, :q quits" "$expected" "$actual"

printf ':exit\n' | "$BIN" --repl >/dev/null 2>&1
check "commands: :exit quits with status 0" "0" "$?"

# ---- :globals lists user bindings, filters __-internals ------------------------
actual=$(printf 'let __hidden := 9\nlet vis := 1\n:globals\n:quit\n' | "$BIN" --repl 2>&1)
case "$actual" in
  *"  vis = 1"*) check ":globals: lists a user binding" "y" "y" ;;
  *) check ":globals: lists a user binding" "  vis = 1 present" "absent" ;;
esac
case "$actual" in
  *__hidden*) check ":globals: filters __-prefixed internals" "__hidden absent" "shown" ;;
  *) check ":globals: filters __-prefixed internals" "y" "y" ;;
esac

# ---- errors-as-values print on stdout via printString --------------------------
actual=$(printf '[3 frotz]\n:quit\n' | "$BIN" --repl 2>/dev/null)
expected="$BANNER
birnam> error(#doesNotUnderstand: does not understand #frotz)
birnam> "
check "errors: a fault prints as a value on stdout, loop survives" "$expected" "$actual"

# ---- compile errors: located block on STDERR, nothing on stdout ----------------
actual=$(printf '[1 + ]\n:quit\n' | "$BIN" --repl 2>&1 1>/dev/null)
expected='1:6: syntax error
[1 + ]
     ^'
check "syntax error: located report goes to stderr" "$expected" "$actual"

actual=$(printf '[1 + ]\n:quit\n' | "$BIN" --repl 2>/dev/null)
expected="$BANNER
birnam> birnam> "
check "syntax error: stdout stays clean (prompts only)" "$expected" "$actual"

# ---- EOF (Ctrl-D): clean exit, status 0 ----------------------------------------
actual=$(printf '[1 + 1]\n' | "$BIN" --repl 2>&1); st=$?
expected="$BANNER
birnam> 2
birnam> "
check "eof: mid-session EOF exits cleanly" "$expected" "$actual"
check "eof: exit status 0" "0" "$st"

# ---- invocation aliases: bare, -i, repl all open the same REPL -----------------
for inv in "" "-i" "repl"; do
  actual=$(printf ':quit\n' | "$BIN" $inv 2>&1 | head -1)
  check "invocation: '${inv:-<bare>}' opens the REPL" "$BANNER" "$actual"
done

# ---- BIRNAM_NO_STDLIB --repl -----------------------------------------------------
# The console REPL lives in birnam (modules/repl.birnam) and rides the stdlib's
# string protocol, so the no-stdlib escape hatch has no interactive loop —
# `birnam <file>` remains for bare-floor debugging. (Changed when the REPL moved
# to birnam; before that the native loop happened to work stdlib-free.)
actual=$(printf '[3 + 4]\n:quit\n' | BIRNAM_NO_STDLIB=1 "$BIN" --repl 2>&1); st=$?
expected='birnam: the REPL requires the stdlib (unset BIRNAM_NO_STDLIB)'
check "no-stdlib: --repl reports the REPL unavailable" "$expected" "$actual"
check "no-stdlib: exit status 1" "1" "$st"

# ---- booted-image REPL: `birnam foo.image` drops into the REPL with bindings ---
tdir=$(mktemp -d)
printf 'let imgx := 41\n[System saveImage: "%s/t.image"]\n:quit\n' "$tdir" | "$BIN" --repl >/dev/null 2>&1
actual=$(printf '[imgx + 1]\n:quit\n' | "$BIN" "$tdir/t.image" 2>&1)
expected="$BANNER
birnam> 42
birnam> "
check "image boot: REPL over a mapped image sees its bindings" "$expected" "$actual"

# ---- booted-image stdlib: doc'd stdlib methods survive the save/boot round-trip.
# startsWith:/endsWith:/words are defined with `defineMethod:…doc:` and their
# guards compare against the memoized StringExemplarVt global (a vtable fact) —
# this locks BOTH the doc'd-define source recording and the exportref fact codec.
actual=$(printf '["hello" startsWith: "he"]\n["hello" endsWith: "lo"]\n["hello" startsWith: 42]\n["a b  c" words]\n:quit\n' | "$BIN" "$tdir/t.image" 2>&1)
expected="$BANNER
birnam> true
birnam> true
birnam> false
birnam> #[a b c]
birnam> "
check "image boot: doc'd stdlib string protocol survives (incl. guards)" "$expected" "$actual"
rm -rf "$tdir"

# ---- booted-image modules: a module singleton + its Alien wraps survive the boot.
# Time is the canonical case: `[Globals at: #Time put: [Object clone]]` (a private
# vtable — serialized under its exported parent, re-specialised by the replayed
# installs) + a native gettimeofday trampoline (recorded as an `__aliens__` fact,
# re-wrapped by replayAliens). This locks the whole chain: singleton survival →
# installs land on it → wrap replay. Run from the repo root (module paths).
tdir=$(mktemp -d)
printf '[System load: "modules/time.birnam"]\n[System saveImage: "%s/mod.image"]\n:quit\n' "$tdir" | "$BIN" --repl >/dev/null 2>&1
actual=$(printf '[Globals includes: #Time]\n[[Time nowMillis] > 1700000000000]\n:quit\n' | "$BIN" "$tdir/mod.image" 2>&1)
expected="$BANNER
birnam> true
birnam> true
birnam> "
check "image boot: a module singleton and its Alien wraps survive" "$expected" "$actual"
rm -rf "$tdir"

# ---- store surface: reading an ABSENT fact answers nil, never a VM panic -------
# `[db currentValue: #absent attr: #x]` used to hand birnam the store's internal
# Zig-null absence — the next send to it panicked the whole VM. The surface now
# normalises to nil; the distinction rides hasValue:attr: / currentValue:attr:ifAbsent:
# (§G6, the no-NULL lock). An unset cell's `value` rode the same null.
actual=$(printf '[db currentValue: #absent attr: #x]\n[db hasValue: #absent attr: #x]\n[db currentValue: #absent attr: #x ifAbsent: [| #missing]]\n[[db cellFor: #fresh attr: #slot] value]\n:quit\n' | "$BIN" --repl 2>&1)
expected="$BANNER
birnam> nil
birnam> false
birnam> #missing
birnam> nil
birnam> "
check "store surface: absent fact reads answer nil (no VM panic)" "$expected" "$actual"

echo "----"
echo "repl-golden: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
