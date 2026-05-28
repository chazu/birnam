# Birnam

A prototypal Smalltalk-80/Objective-C-like language built on a COLA-style object system, implemented in Zig.

Birnam is a modular system consisting of a shell, an `id` object model (inspired by Ian Piumarta's [idst](https://www.piumarta.com/software/cola/)), a PEG parsing engine, and a high-level language with Smalltalk-80 syntax and prototypal delegation semantics.

## Architecture

```
birnam (shell)
  | loads dylibs
  +-> libbirnam_id.dylib     object model: vtable dispatch, method cache, bootstrap
  +-> libbirnam_peg.dylib    PEG parsing engine
  +-> libbirnam_lang.dylib   language interpreter + REPL
```

The shell dynamically loads modules as shared libraries. Each module exports a `birnam_module_init` entry point and communicates through a shared export/import table. Modules can register REPL handlers, and the shell supports hot-reloading via kqueue file watching or SIGHUP.

## Building

Requires **Zig 0.15.1**.

```
zig build
```

This produces:

- `zig-out/bin/birnam` -- the shell executable
- `zig-out/lib/libbirnam_id.dylib` -- id object model
- `zig-out/lib/libbirnam_peg.dylib` -- PEG parser
- `zig-out/lib/libbirnam_lang.dylib` -- language module

## Running Tests

```
zig build test
```

130 tests across all modules (5 id, 14 peg, 111 lang).

## Running the REPL

```
# Load id first, then lang (lang depends on id)
./zig-out/bin/birnam ./zig-out/lib/libbirnam_id.dylib
```

The id module bootstraps the object system and exports `id_api`. The lang module imports it and registers its REPL handler. In the default configuration, the shell loads the id module; the lang module must be loaded as well for the language REPL to activate.

Once the REPL is running:

```
birnam> 3 + 4
7
birnam> 'Hello, ' , 'world'
'Hello, world'
birnam> [:x | x * x] value: 5
25
birnam> true ifTrue: ['yes'] ifFalse: ['no']
'yes'
```

Type `quit` or `exit` (or send EOF with Ctrl-D) to leave.

## Exploring the System in the REPL

The REPL is a live environment. State persists between lines -- variables, type definitions, and method definitions all accumulate across the session. This makes it a good tool for incrementally building up objects and exploring how the system works.

### Arithmetic and Strings

Start with simple expressions. Every line you enter is parsed, evaluated, and the result is printed:

```
birnam> 3 + 4
7
birnam> (10 - 2) * 3
24
birnam> 16rFF
255
birnam> 'Hello, ' , 'world!'
'Hello, world!'
birnam> 'hello' size
5
```

### Inspecting Objects

Use `printString` to get a string representation of any value. Use `class` to see what vtable an object has. Use `isNil` to test for nil:

```
birnam> 42 printString
'42'
birnam> nil printString
'nil'
birnam> #(1 2 3) printString
'#(1 2 3)'
birnam> nil isNil
true
birnam> 42 isNil
false
birnam> 'hello' class == 'world' class
true
```

### Testing Identity vs Equality

`=` tests structural equality (for strings and integers). `==` tests object identity (same pointer). This distinction matters:

```
birnam> 'abc' = 'abc'
1
birnam> 'abc' == 'abc'
false
birnam> x := 'hello'. x == x
true
birnam> x ~~ x
false
```

### Working with Blocks

Blocks are closures. You can store them in variables, pass them as arguments, and call them later:

```
birnam> double := [:x | x * 2]. double value: 21
42
birnam> apply := [:f :x | f value: x]. apply value: [:n | n + 1] value: 9
10
```

Blocks close over their environment:

```
birnam> base := 100. [:x | base + x] value: 7
107
```

### Building Types Interactively

You can define types and methods one line at a time. Each definition takes effect immediately:

```
birnam> Point : Object ( x y )
Point
birnam> Point x [^x]
nil
birnam> Point x: v [x := v]
nil
birnam> Point y [^y]
nil
birnam> Point y: v [y := v]
nil
birnam> p := Point new
a Point
birnam> p x: 10. p y: 20. p x
10
birnam> (p x) + (p y)
30
```

Method definitions return nil (they're side effects -- they install methods on the vtable). The interesting part is what happens when you use the methods afterward.

### Adding Methods After the Fact

Since types are prototypal (vtable-based, not class-based), you can add methods to a type at any time, even after instances exist:

```
birnam> Point dist [^((x * x) + (y * y))]
nil
birnam> p dist
500
```

The new method is immediately available on all existing instances because they share the same vtable.

### Exploring Control Flow

All control flow is message-based. Booleans respond to `ifTrue:`, `ifFalse:`, etc. Blocks respond to `whileTrue:`, `whileFalse:`:

```
birnam> true ifTrue: ['yes'] ifFalse: ['no']
'yes'
birnam> false not
true
birnam> true and: [false]
false
birnam> i := 1. [i < 6] whileTrue: [i := i * 2]. i
8
birnam> sum := 0. 5 timesRepeat: [:i | sum := sum + i]. sum
15
```

### Working with Arrays

Arrays are created with `#( )` and support collection operations:

```
birnam> a := #(10 20 30 40 50)
#(10 20 30 40 50)
birnam> a size
5
birnam> a at: 2
30
birnam> a collect: [:x | x * 2]
#(20 40 60 80 100)
birnam> a inject: 0 into: [:sum :x | sum + x]
150
```

### Using Cascades

Cascades (`;`) send multiple messages to the same receiver. This is useful for configuring an object without repeating its name:

```
birnam> Counter : Object ( count )
Counter
birnam> Counter count [^count]
nil
birnam> Counter count: v [count := v]
nil
birnam> Counter inc [count := count + 1]
nil
birnam> c := Counter new. c count: 0
0
birnam> c inc; inc; inc; inc; inc. c count
5
```

### Catching Unknown Messages

Define `doesNotUnderstand:` on a type to intercept messages the object doesn't have methods for:

```
birnam> Logger : Object ( lastMsg )
Logger
birnam> Logger lastMsg [^lastMsg]
nil
birnam> Logger doesNotUnderstand: sel [lastMsg := sel. ^sel]
nil
birnam> l := Logger new. l anythingAtAll
'anythingAtAll'
birnam> l lastMsg
'anythingAtAll'
```

### Error Messages

If you make a syntax error, the REPL reports the position:

```
birnam> (3 +
parse error: UnexpectedEof at line 1, col 5
birnam> 3 + + 4
parse error: UnexpectedToken at line 1, col 5
```

Evaluation errors report the kind of failure:

```
birnam> x
eval error: UndefinedVariable
birnam> nil + 1
eval error: DoesNotUnderstand
```

### Tips

- **State accumulates.** Variables and types defined on earlier lines are available on later lines. There is no way to undefine something short of restarting.
- **Dot separates statements.** Use `.` to put multiple statements on one line: `x := 1. y := 2. x + y`
- **Parentheses control precedence.** Unary binds tightest, then binary, then keyword. When in doubt, parenthesize: `(a size) + (b size)`
- **Comments are double-quoted.** `"this is ignored"` -- useful for annotating REPL lines.
- **Hot reload.** If you rebuild `libbirnam_lang.dylib` while the shell is running, it will detect the file change (via kqueue) and reload automatically. State is lost on reload.
- **SIGHUP.** You can also send `kill -HUP <pid>` to trigger a manual reload of all modules.

## The Language

Birnam's language layer is a prototypal Smalltalk-80 dialect. It has Smalltalk syntax, Smalltalk message-passing semantics, and a prototypal (delegation-based) object model rather than a class-based one.

### Literals

```
42                     "integer"
-7                     "negative integer"
16rFF                  "radix integer (hex 255)"
'hello'                "string ('' for embedded quote)"
#foo                   "symbol (evaluates to a string)"
#at:put:               "keyword symbol"
#'hello world'         "quoted symbol"
#+                     "binary operator symbol"
$a                     "character literal (ASCII value, here 97)"
$\n                    "escape character (newline = 10)"
#(1 2 3)               "array literal"
#()                    "empty array"
true                   "boolean true singleton"
false                  "boolean false singleton"
nil                    "nil (null pointer)"
```

### Comments

Double-quoted strings are comments, as in Smalltalk:

```
"This is a comment"
42 "inline comment" + 1
```

### Messages

Birnam follows standard Smalltalk-80 message-passing rules. There are three kinds of messages:

**Unary** (no arguments):

```
5 yourself             "=> 5"
'hello' size           "=> 5"
true not               "=> false"
nil isNil              "=> true"
42 class               "=> SmallInteger vtable"
42 printString         "=> '42'"
```

**Binary** (one argument, operator selector):

```
3 + 4                  "=> 7"
10 - 3                 "=> 7"
6 * 7                  "=> 42"
10 // 3                "=> 3 (integer division)"
10 \\ 3                "=> 1 (modulo)"
3 < 5                  "=> 1 (truthy)"
'abc' , 'def'          "=> 'abcdef'"
'hello' = 'hello'      "=> 1"
x == x                 "=> true (identity)"
x ~~ y                 "=> true (identity inequality)"
```

**Keyword** (one or more colon-terminated keywords, each with an argument):

```
'hello' at: 1               "=> 101 (ASCII 'e')"
#(10 20 30) at: 0            "=> 10"
obj at: 1 put: 99            "keyword message with two parts"
true ifTrue: ['yes']         "=> 'yes'"
```

### Precedence

Unary > Binary > Keyword, left-to-right within each level. Use parentheses to override:

```
2 + 3 * 4              "=> 20  ((2+3)*4, left-to-right binary)"
2 + (3 * 4)            "=> 14"
(obj size) + 1          "unary 'size' before binary '+'"
```

### Variables and Assignment

```
x := 42                "local variable assignment"
x := x + 1             "reassignment"
Foo := 'bar'           "global assignment (uppercase name)"
```

### Blocks (Closures)

Blocks are first-class closures delimited by `[ ]`:

```
[42] value                           "=> 42"
[:x | x + 1] value: 5               "=> 6"
[:x :y | x + y] value: 3 value: 4   "=> 7"
```

Blocks capture their lexical environment:

```
x := 10. [:y | x + y] value: 5      "=> 15"
```

Blocks support temporary variables:

```
[:x | | tmp | tmp := x * 2. tmp + 1] value: 5   "=> 11"
```

### Statements and Sequences

Statements are separated by `.` (dot):

```
x := 3. y := 4. x + y   "=> 7"
```

### Control Flow

Control flow is done entirely through message sends, as in Smalltalk.

**Booleans:**

```
true ifTrue: ['yes']                    "=> 'yes'"
false ifTrue: ['yes']                   "=> nil"
false ifFalse: ['no']                   "=> 'no'"
true ifTrue: ['yes'] ifFalse: ['no']    "=> 'yes'"
false ifTrue: ['yes'] ifFalse: ['no']   "=> 'no'"
true not                                "=> false"
true and: [false]                       "=> false (short-circuit)"
false or: [true]                        "=> true (short-circuit)"
```

**Loops:**

```
i := 0.
[i < 5] whileTrue: [i := i + 1].
i   "=> 5"

[i > 0] whileFalse: [i := i + 1]

5 timesRepeat: [:i | i println]   "prints 1 through 5"
```

### Type Definitions

Types are defined with prototypal delegation. A type definition creates a new vtable that delegates to a parent:

```
Point : Object ( x y )
```

This creates a `Point` type with two slots, `x` and `y`, delegating to `Object`.

### Method Definitions

Methods are installed on types. The body is a `[ ]` block:

**Unary method:**

```
Point x [^x]
```

**Keyword method:**

```
Point x: v [x := v]
```

**Binary method:**

```
Point + other [^(x + (other x))]
```

**Multi-keyword method:**

```
Pair first: a second: b [first := a. second := b]
```

### Slot Access

Inside a method, slot names are directly accessible as variables. Reading a slot name returns the slot value; assigning to a slot name sets it:

```
Point : Object ( x y ).
Point x [^x].           "reads slot x"
Point x: v [x := v].    "writes slot x"
Point y [^y].
Point y: v [y := v]
```

### Creating Instances

```
p := Point new.
p x: 10.
p y: 20.
p x   "=> 10"
```

### Self

Inside a method, `self` refers to the receiver:

```
Box : Object ( value ).
Box value [^value].
Box getSelf [^self].
b := Box new. b value: 5.
(b getSelf) value   "=> 5"
```

### Non-Local Return

`^value` returns from the enclosing method, not just the block:

```
Box earlyReturn [^42. ^99].
Box new earlyReturn   "=> 42 (second return never reached)"
```

### Cascades

`;` sends additional messages to the same receiver:

```
Box : Object ( value ).
Box value: v [value := v].
Box inc [value := value + 1].

b := Box new. b value: 0.
b inc; inc; inc.
b value   "=> 3"
```

### Arrays

```
a := #(10 20 30).
a size              "=> 3"
a at: 0             "=> 10"
a at: 1 put: 99.
a at: 1             "=> 99"
```

**Collection methods:**

```
#(1 2 3) do: [:x | x println]          "prints each element"

#(1 2 3) collect: [:x | x * 2]         "=> #(2 4 6)"

#(1 2 3 4) inject: 0 into: [:acc :x | acc + x]   "=> 10"
```

### doesNotUnderstand:

When an object receives a message it doesn't understand, the system tries sending `doesNotUnderstand:` with the selector name as a string:

```
Proxy : Object ( log ).
Proxy log [^log].
Proxy doesNotUnderstand: sel [log := sel. ^42].

p := Proxy new.
p bogusMessage.    "=> 42, and p log is now 'bogusMessage'"
```

### Universal Methods

These methods work on all objects, including nil:

| Method | Description |
|---|---|
| `yourself` | Returns the receiver |
| `isNil` | Returns `true` if nil, `false` otherwise |
| `class` | Returns the receiver's vtable |
| `printString` | Returns a string representation |
| `==` | Identity (pointer) equality, returns `true`/`false` |
| `~~` | Identity inequality, returns `true`/`false` |

### Complete Example

```
"A counter with increment and reset"
Counter : Object ( count ).
Counter count [^count].
Counter count: v [count := v].
Counter inc [count := count + 1].
Counter reset [count := 0].
Counter timesN: n [^count * n].

c := Counter new.
c count: 0.
10 timesRepeat: [:i | c inc].
c count                        "=> 10"
c timesN: 3                    "=> 30"
c reset.
c count                        "=> 0"
```

## The id Object Model

The foundation of Birnam is `id`, a minimal prototypal object system based on Ian Piumarta's design from the VPRI STEPS project. Key characteristics:

- **Vtable-at-minus-one**: Every object's vtable pointer is stored at offset -1 (one pointer width before the object pointer). This allows uniform dispatch without per-object overhead.
- **Prototypal delegation**: Vtables delegate to parent vtables. Method lookup walks the delegation chain.
- **Closure-based methods**: Methods are `(function_pointer, data)` pairs (closures). The function pointer follows C calling conventions.
- **Send dispatch**: `send0`-`send3` look up the method in the receiver's vtable (with a 1024-entry method cache) and call it.
- **Bootstrap**: The system bootstraps itself from nothing -- `vtable_vt` points to itself (the fixed point), then `object_vt`, `vector_vt`, selectors, and closures are built up incrementally.

The id module exports an `IdApi` struct of C-callable function pointers that other modules use:

| Function | Description |
|---|---|
| `intern(name)` | Intern a selector string |
| `alloc(vt, size)` | Allocate an object with the given vtable |
| `send0`-`send3` | Message dispatch (0-3 arguments) |
| `vtable_delegated(parent)` | Create a new vtable delegating to parent |
| `vtable_method_at_put_with(vt, sel, fn, data)` | Install a method |

### Tagged Integers

SmallIntegers are encoded as tagged pointers: `(value << 1) | 1`. They are not id objects (no vtable at offset -1). The evaluator checks the tag bit and handles SmallInt operations directly, bypassing vtable dispatch for arithmetic.

## The PEG Module

`libbirnam_peg.dylib` provides a PEG (Parsing Expression Grammar) engine. It includes:

- 15 AST node types for PEG grammars (sequence, ordered choice, repetition, character classes, etc.)
- A matching engine that operates on id objects
- A hand-written recursive descent parser that can parse PEG grammar definitions

The PEG module imports `id_api` from the id module and creates its grammar nodes as id objects with vtables.

## Project Structure

```
birnam/
  build.zig                 build configuration
  src/
    shell.zig               module loader, kqueue watcher, REPL loop
    id/
      id.zig                module entry point, re-exports, tests
      object.zig            Oop, Object, Vtable, Closure, alloc, vtable-at-minus-one
      vtable.zig            vtable lookup, method installation
      selector.zig          sorted-array selector interning
      mcache.zig            1024-entry method cache
      send.zig              Send struct, bind, send0-send3
      bootstrap.zig         bootstrap sequence (vtable_vt fixed point, etc.)
      api.zig               IdApi extern struct, C-callable wrappers
    peg/
      peg.zig               module entry point, tests
      api.zig               local IdApi struct definition
      nodes.zig             15 PEG AST node types
      engine.zig            PEG matching engine
      bootstrap.zig         hand-written PEG grammar parser
    lang/
      lang.zig              module entry point, REPL handler, 111 tests
      api.zig               local IdApi struct definition
      ast.zig               17-tag AST node types (Zig structs, arena-allocated)
      parser.zig            recursive descent parser with source position tracking
      eval.zig              tree-walking evaluator, trampolines, universal methods
      env.zig               environment/scope chain, non-local return
      builtins.zig          SmallInt, String, Block, Array, Boolean vtables and primitives
```

## Design Notes

- **AST is Zig data, not id objects.** The parser produces Zig structs allocated from an arena. The evaluator bridges AST nodes to the id object system. This keeps parsing fast and decoupled from the object model.
- **Method trampolines.** Language-defined methods need C-callable entry points for id's send dispatch. Four trampoline functions (`trampoline0`-`trampoline3`) extract method data from the closure, create a scope, bind parameters, and evaluate the AST body.
- **No GC yet.** Memory is allocated from `page_allocator`. A garbage collector is a future module.
- **Cross-dylib independence.** Each module has its own copy of the `IdApi` struct definition. Zig 0.15 does not support cross-dylib imports, so modules communicate through C-ABI function pointers.

## References

- Ian Piumarta, [Open, Extensible Object Models](https://www.piumarta.com/software/cola/) (the `id` object model)
- VPRI [STEPS Toward the Reinvention of Programming](http://www.vpri.org/pdf/tr2012001_steps.pdf)
- Adele Goldberg, David Robson, *Smalltalk-80: The Language and its Implementation*
