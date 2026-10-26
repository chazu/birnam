# Bytes

Bytes owns its buffers and decoded views. Id sees Oop receivers, selectors,
arguments and results. `fromBytes:` and `appendBytes:` read the supplied object
through ordinary `sizeWord` and `atWord:` messages; no application or forwarding
protocol is involved. `fromByteValues` and `appendByteValues` construct or append
explicit immediate byte values. `sizeWord` and `atWord:` expose count and indexed
byte values; `size` and `at:` remain the language Integer operations.

Native buffer/view ownership, finalization and allocation accounting remain in
this owner. `bash scripts/oop-id-slice-test.sh` qualifies the native slice.
