# String and text construction

`libbirnam_string` takes one Namespace Object and returns String. Bootstrap loads
it after Object Model. It requires completed Roots, GC, ObjectVt, true, false and
nil bindings. It publishes String, StringVt, StringBuilder and StringBuilderVt;
there is no separate builder library. Repeat initialization returns the published
String without reinstalling methods.

Every cross-module operation is a fresh Id message. The instance owns selectors,
family objects and a weak address map of native byte buffers. String's two raw
payload words and Builder's three words mirror pointer/length/capacity for native
diagnostics; consumers must use messages, never these layouts. Native access
uses the owner's map, so payload mutation and private behavior changes cannot
redirect borrowing or cleanup. No mutable process/TLS state or service tables
are introduced. Calls are confined to the owning Id thread.

## Messages

| Receiver / selector | Arguments | Result |
|---|---|---|
| String `fromBytes:` | borrowed native Bytes | New String with copied bytes, including NUL/non-UTF-8. |
| String instance `bytes` | none | Borrowed native Bytes. Copy before any subsequent send that can collect or invalidate the owner; retaining the object does not authorize retaining this borrow across arbitrary callbacks. |
| String instance `length` | none | Native Word byte count (existing native protocol). |
| String instance `new`, `clone` | none | New canonical String with an independent copy of the receiver's bytes. |
| String instance `printString` | none | Receiver. |
| String instance `,` | Object | New String; non-String input receives `printString`, then `bytes`. |
| String instance `=` | Object | true/false byte equality with a String owned by this instance. |
| String instance `trimmed` | none | New String with ASCII whitespace trimmed. |
| String instance `replace:with:` | two Strings | New String, left-to-right nonoverlapping substitution; empty needle leaves bytes unchanged; non-String inputs return nil. |
| String instance `asSymbol` | none | Id Symbol; preserves the existing first-NUL boundary. |
| StringBuilder instance `new`, `clone` | none | New empty builder (existing semantics). |
| StringBuilder instance `add:` | Object | Append text, using `printString` for non-String input; return receiver. |
| StringBuilder instance `asString`, `printString` | none | Independent String snapshot through the current String `fromBytes:` method. |

Operations producing text send the current `fromBytes:` method. Resource-aware
String cloning constructs its own buffer and installs its own GC finalizer.
Printing callbacks and allocating sends hold no byte-map borrow. Native Bytes
inputs and callback results are copied before further sends; receiver/argument
objects are retained through Roots for the call. Builder mutation observes GC's
construction phase before changing its buffer.

## Lifetime

String implements the existing owner messages `rootsDo:`, `referencesOf:do:` and
`finalize:`. Its root inventory retains family identities and selectors, not all
buffer keys. Text contains no object references. GC attaches this owner to String
and Builder behaviors, and attaches a finalizer to every constructed buffer.
Finalization removes the owned buffer after Id detaches the object, without
consulting detached Id metadata or calling another dependency. Private behaviors
inherit the reference owner. Changing/removing a finalizer carries responsibility
for the replaced resource cleanup, as with other native finalizers.

Number-independent operations are installed immediately. String `size`, `at:`,
`byteAt:`, `copyFrom:to:`, `indexOf:`, `bracketDepth`, `asInteger`, `asNumber`, and
Builder `size`/`addByte:` await Integer/Float conversion; they are not retyped to
Words. The bracket-depth algorithm is retained privately for that integration.
Splitting awaits collection objects. Neither dependency blocks basic text or
Boolean/Nil printing. Run `mise run native-messages` for production coverage.
