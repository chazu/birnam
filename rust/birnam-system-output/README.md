# System construction and output over Id

Bootstrap loads `prelude`, `printing` and `system_output` with the completed
Namespace Object after String and the Boolean/Nil printers, before collections.
Each module depends only on the common Id ABI. Prelude publishes `System` and
`SystemVt`; Printing installs Object/Symbol defaults and publishes Id's existing
`SymbolVt`. Repeated loading preserves installed replacements. Prelude returns
the original System; the two method installers return Unit.

| Method | Preserved behavior |
|---|---|
| Object `printString` | Construct a fresh String containing `anObject`. |
| Object `asString` | Send the receiver's current `printString`. |
| Symbol `printString` | Copy its name through Id `symbolBytesOf:` and construct a String prefixed with `#`. |
| System `print:` / `println:` | Write exact String bytes, or send `printString` once and require an exact String result. Append a newline only for `println:`, flush, and return the receiver. |

An unavailable/non-String rendering and wrong language arity return nil. Output
keeps its existing policy of ignoring OS write/flush errors. Malformed native
operands fail admission; native send failures propagate unchanged. Printing does
not escape bytes or impose UTF-8 decoding, and exact Strings bypass `printString`.

Construction, retention, Namespace access, method installation, String construction
and byte access all use current Id sends. Receivers, inputs and rendered Strings
remain rooted across collecting methods. Borrowed names/bytes are copied before
another send can invalidate them. Rendering finishes before stdout is locked, so
`printString` can itself print. The native stdout write/flush is private I/O owned
by this module; there is no cross-module native callback or text-view service table.

Run `bash scripts/native-message-test.sh`. The `system_output` integration binary
captures a subprocess's actual stdout and compares exact bytes, including NUL and
non-UTF-8 data. It tests source output, aliases, fresh Object text, Symbol text,
conversion rejection, current lookup, collecting factories/renderers/byte methods,
reentrant output, exact native failures and reload preservation. The existing
unit test checks flushing of a newline-free prompt:

```
cargo test --offline --locked -p birnam-system-output --lib
```

The install manifest uses `libbirnam_prelude.dylib`, matching Bootstrap's path.
Input and environment now use the same prefix; see their [contract](../birnam-system-input/README.md).
[File I/O](../birnam-file-io/README.md) is also connected. Full Host/CLI startup,
image round trips, System GC controls and loaders remain separate migration work. Historical image smoke tests
are not evidence for those unfinished paths.
