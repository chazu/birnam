# Bootstrap Block loops through Id

Bootstrap loads `libbirnam_block_loop` immediately after Execution. Its initializer
accepts one Namespace Object, installs `whileTrue:` and `whileFalse:` on Block's
shared behavior, and returns Unit. Repeat initialization preserves replacements;
a different Namespace/Id or thread is rejected. The only dependency is the common
Id ABI. No service tables, foreign payloads or saved dependency entries are used.

Both methods take one body Object. Wrong arity or a null argument carrier returns
nil. Receiver/body types are not prechecked: each iteration freshly sends `value`
to the condition, then to the body only when the canonical Boolean permits it.
`whileTrue:` continues on true; `whileFalse:` on false. Normal termination returns
nil. Non-Boolean conditions produce `mustBeBoolean` with the existing message.
Exact-family Error results from either callback return unchanged; ordinary body
results are ignored. This preserves the native loop's exact ErrorVt recognition,
using current Namespace and Id `behaviorOf:` messages, rather than introducing
an `isError` override policy. Error construction remains lazy and replaceable.
Native dependency failures propagate unchanged.

The receiver and body remain rooted for the call. Each callback result stays
rooted across error recognition and is released before the next iteration.
Native iteration has constant stack depth; roots do not accumulate with iterations.
Every cross-owner operation, including GC roots and error construction, is a send.
The private Rust loop itself owns no language execution state.

## Existing control boundary

These are the synchronous bootstrap methods. Execution rejects suspension across
their native callback boundary. The existing authored Boolean/loop methods in
`modules/stackless-control.birnam` replace them for resumable execution; this slice
adds no native loop engine or scheduler. Execution now registers Block as owner of
its shared behavior through `ownerOf:put:`, so Reflect changes on Block reach both
existing and newly created compiled Blocks. This repairs a missing prototype
ownership registration; ordinary Store MOP publication is unchanged.

`bash scripts/native-message-test.sh` builds/audits the dylib and runs `block_loop`.
Coverage includes 10,000 iterations, compiled Store-backed counters, both loop
directions, early termination, exact errors, invalid conditions, live replacements,
native failures and collection of transient inputs/results. It verifies the native
suspension barrier, then installs the unchanged Boolean/while prefix from the
actual authored control file and verifies suspension/resumption and reload safety.
This is not acceptance of the rest of that file's scheduler/collection policy,
full Host startup, Image restoration or the historical service-table probes.
