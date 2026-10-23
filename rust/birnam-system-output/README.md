# System output over Oop messages

The v2 initializer installs `print:`, `println:` and `printErrorText:` on System.
It captures the completed Namespace, exact String vtable and nil object. One
ordinary root Sequence retains those dependencies and its private writing Method.
Repeated loading preserves method replacement and removal.

`print:` and `println:` use exact String bytes directly; other values receive
`printString` once and must return an exact String. Wrong arity or an unavailable
rendering returns language nil. A missing argument reference is invalid. Current
`bytes` and byte-provider application remain observable. A private Method receives
byte Oops plus an output mode, then performs the OS write without language sends
while the output lock is held. Incoming objects use Id call roots.

Stdout preserves binary bytes. Only `println:` adds LF; both methods flush and
return their receiver. `printErrorText:` accepts an exact String, writes UTF-8-lossy
text to stderr, adds no newline, and flushes. The existing policy of ignoring OS
write/flush failures is preserved. Id failures still propagate.

`bash scripts/oop-id-slice-test.sh` checks actual subprocess stdout/stderr bytes,
NUL/non-UTF-8 output, numeric rendering, reentrant printing with GC, malformed
results, private String rejection, live byte methods, captured Methods and reload.
The unit check verifies a newline-free prompt is flushed. Full source, Host/CLI,
image and worker acceptance remain pending.
