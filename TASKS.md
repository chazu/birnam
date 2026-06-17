# Birnam Tasks

Living task tracker — parallel to [ROADMAP.md](ROADMAP.md). ROADMAP = phase narrative + reasoning; TASKS = concrete, checkable work items (gaps, fixes, follow-ups) discovered during implementation/verification. An item graduates to a commit; closed items keep a one-line provenance.

Convention: `[ ]` open, `[~]` in progress, `[x]` done (with commit/date). Severity tags: **BLOCKER** / **gap** / **nice-to-have** / **doc**. Each item names the file:line when known.

---

## Networking / FFI surface

Discovered 2026-06-17 verifying that a **pure-birnam module wrapping libc** (no zig, no C shim) can build a `sockaddr_in` and read `errno` — the hard prerequisites for path (A): networking as a birnam module over libc via libffi. Both verified working. See [memory: networking_ffi]. Tenet: authoring a C-wrapping module must not require a zig install.

### Verified working (no fix needed)
- [x] Build `sockaddr_in` bytes in pure birnam — `[Module alloc: 16]` + `[sa pokeByte: v at: off]`, round-trips via `peekByte:`. (2026-06-17)
- [x] FFI call into libc — `[libc close: [0 - 1]]` → -1. (2026-06-17)
- [x] Read `errno` via `__error()` — `wrap: #errnoPtr as: "__error" signature: ")p"`, errno=9 (EBADF) read byte-wise. (2026-06-17)

### Fixes / gaps — ALL SHIPPED 2026-06-17 (tests green: ffi_module_test + lang_module_test)
- [x] **BLOCKER** — `peekInt:` reads 8-byte `i64`; C `int` is 4 bytes → 4-byte `int*` reads (errno, most `int*` out-params) overflowed the 61-bit SmallInt → `nil`. **FIXED:** added width-complete peek/poke family `peekInt32:`/`peekUInt32:`/`peekShort:`/`peekUShort:`/`pokeInt32:at:`/`pokeShort:at:` (SELs 240-245, below `USER_EID_START`=256). Methods + installs in `src/kernel/ffi_module.zig` (`methodPeekInt32`…), interner seeds in `src/kernel/bootstrap.zig`, eids in `src/kernel/kernel_eids.zig`. Verified: errno via `__error()` + `peekInt32: 0` → 9. Signed peeks sign-extend, `U` peeks zero-extend, pokes truncate low N bytes. Test: "width-complete peek/poke …".
- [x] **gap** — 2-byte short peek/poke. **FIXED** as part of the width family above (`peekShort:`/`peekUShort:`/`pokeShort:at:`). `in_port_t`/`sa_family_t` now read without byte-fiddling.
- [x] **gap** — negative integer literal as keyword arg parse error. **FIXED:** added `neg-integer`/`neg-float` grammar rules (`"-" integer` / `"-" float-literal`) recognized ONLY at atom (primary) position, so binary minus is unaffected (verified `[3 - 1]`→2 and `[10 - -5]`→15 both still work). `src/modules/lang/lang.zig` (grammar rules + atom_alts + `rn_neg_integer`/`rn_neg_float` desugar in both literal-value and main desugar). Test: "negative numeric literals at primary position".
- [x] **gap** — no integer bitwise / shift operators. **FIXED:** added `bitAnd:`/`bitOr:`/`bitXor:`/`bitNot`/`shiftLeft:`/`shiftRight:` + `<<`/`>>` infix sugar on `SMALLINT_VT` (`src/modules/lang/lang.zig`, `smallintBitAndImpl`… + install block). Integer-only (Float arg → `#typeMismatch`); range-checked via `mkSmallIntChecked` (`#overflow`); shift count guarded 0..60 (`#shiftAmount`). **`&`/`|` kept boolean-only** per the agreed recommendation (distinct bitwise names avoid int-vs-bool ambiguity). Verified htons(8080)=36895 byteswap. Test: "SmallInt bitwise + shift".
- [x] **non-goal** — bit-level `peekBit:`/`pokeBit:` confirmed NOT needed (C ABI is byte-addressed). Sub-byte = peek containing width + mask/shift with the bitwise ops above. Documented in `docs/2d-ffi.md` peek/poke table.
- [x] **nice-to-have** — wraps now resolve at each REPL line boundary too (not just `:in`/`:load`/AOT). **FIXED:** `ffi_module.resolveAllPending` called after each handler line in the REPL loop, `src/shell.zig` (idempotent — `resolveModule` skips bound wraps/open libs). Verified: `wrap:` then call in one session (no `:in`) → errno=9.
- [x] **doc** — `wrap:as:signature:` arg order (`#selector` first, `"cname"` second). **FIXED two ways:** (1) passing a non-selector first arg now returns an `ffi-wrap-bad-selector` ERROR_VT with a message stating the order (was silent `nil`), `src/kernel/ffi_module.zig` `methodWrapAsSig`; (2) documented loudly in `docs/2d-ffi.md` "Declaring a wrap". Test: "wrap: with a non-selector first arg …".
- [x] **doc** — `docs/2d-ffi.md` rewritten dyncall→libffi: marshaling section, `ffi_type` aggregates, arm64-Apple aggregates-now-work, new peek/poke width table, wrap arg-order + REPL-resolve note.

### Networking design decisions (open — see ROADMAP when committed)
- [ ] Decide path (A) pure-birnam-over-libc vs (B) zig `std.net` in core stdlib. Tenet leans (A).
- [ ] Scope v1: client-only vs client+server (server forces a concurrency model early).
- [ ] TLS: defer (plaintext HTTP/1.1 first) vs https-in-v1. TLS is the real hard fork. Candidates: BearSSL (MIT), mbedTLS (Apache-2), zig `std.crypto.tls`.
- [ ] HTTP layer: model request/response as fact-store entities (status, headers-as-attrs, body) — fits the relational grain.
- [ ] License: avoid mongoose (GPLv2-or-commercial). civetweb/libcurl/BearSSL/picohttpparser/llhttp are MIT-ish.
