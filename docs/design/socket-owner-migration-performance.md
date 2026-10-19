# Socket ownership migration: performance hold

2026-10-19. Baseline `857b57e8`. Runtime, fixture and formal-model changes are
uncommitted pending performance approval. The user's migration goal requires
holding after significant performance regressions.

## Implemented draft and correctness evidence

SockConn stores an ordinary Cell containing an immutable descriptor/buffer pair.
Clones share that owner; `Socket wrapFd:` installs a fresh owner for each new
connection. There is no Store state, prefixed field schema or native owner registry.

`close` asks the current `connectionOwner` for its value, conditionally replaces
that exact pair with an empty pair, then frees/closes the captured resources.
Retirement precedes callbacks. Reentry and aliases cannot claim the same pair
again; a replacement installed during cleanup survives. The first close status,
including an error, is returned without retry. Inspection getters `fd`/`buffer`
read the owner's pair; close operates on the owner snapshot itself. All of these
are ordinary authored methods and sends, not compiler assumptions about methods
remaining unchanged.

The draft adds `Cell replace:with:` as an explicit identity-based conditional
write, implemented through ObjectModel inside Cell. Socket uses that message and
does not know Cell's private slot layout. Native bodies, calling conventions,
method-identity guards and deoptimization machinery are unchanged.

The existing socket defect was reproduced before editing: after closing an old
connection, `dup` reused its numeric descriptor; closing the old object again
closed the new descriptor. The draft fixes this, including the cloned-alias case.
It also rejects images of live connections while allowing closed connections and
factory clones, preserving their shared owner identity on image recovery.

Passed acceptance:

- Real descriptor reuse, cloned aliases, collecting/reentrant cleanup, replacement
  during cleanup, stale conditional writes, error status and no retry.
- Mapped and rebuilt closed-owner images; live-owner refusal preserves an existing
  destination file.
- Actual Cell initialization and identity-based conditional replacement with
  Store/Relational physically absent.
- Installed interaction tooling, executable image recovery, persistent TCP
  sessions, bind failure, project tty/socket execution and main-once behavior.
- 405 Lean theorems, 128 rejected mutations and 277 expected TLC outcomes. New
  owner models distinguish record identity from reused numeric descriptors and
  check retirement, reentry, replacement and cleanup-status behavior. They model
  the default protocol, not arbitrary user redefinitions or deliberate resurrection
  of consumed raw capabilities; no runtime guard forbids those changes.

Socket initialization still calls Alien wrappers, whose recipe storage remains
Store-dependent. The draft removes direct socket-state use of Store; it does not
prove full Store-free socket initialization yet.

## Performance regression

Three interleaved observations per variant, ordinary release runtimes, normal GC:

| Batch | Baseline | Shared-owner draft | Change |
| --- | ---: | ---: | ---: |
| 500 `fd` reads | 138.09 ms | 147.04 ms | +6.5% |
| 100 real descriptor wraps/closes | 57.00 ms | 71.22 ms | **+25.0%** |
| Facts added by wrap/close batch | 200 | 0 | Removed |

The ownership and retention improvements do not by themselves qualify the CPU
regression. No runtime change from this slice has been committed.

An untimed census of the actual installed policies shows where work grows:

| One outward operation | Baseline sends | Draft sends |
| --- | ---: | ---: |
| `fd` | 664 | 958 |
| Ordinary `Cell value` | 620 | 620 |
| Ordinary `Cell with:` | 872 | 872 |
| `Socket wrapFd:` | 2,350 | 3,411 |
| `close` | 1,638 | 2,530 |

Cell itself has not become slower: the correct shared-owner path calls it more.
Its existing value/set/conditional-write methods resolve ObjectModel and construct
a slot-index operand repeatedly. Namespace lookup, binding/representation checks
and rooting dominate the extra message counts. The new Cell allocation and
conditional retirement also have real costs; the census is not a CPU profile and
does not prove that accessor changes alone will recover every nanosecond.

## Proposed remediation, awaiting approval

1. Capture the ObjectModel receiver and immutable slot index when the authored
   Cell prototype is initialized, as already done in Globals, Trace and task
   accessors. Use those values inside `value`, `set:` and `replace:with:`.
2. Keep `with:` calling the current `clone` and `set:` methods. Keep every accessor
   sending to the current method on its captured ObjectModel receiver. Do not add
   native Cell policy, cached method entries, replacement guards or suspension
   annotations. Rebinding the global ObjectModel name would no longer redirect an
   existing Cell accessor; editing that receiver's methods or Cell's methods
   remains live. This is an explicit lexical dependency in authored source.
3. Update the old Cell isolation fixture: it currently exports zero-capture method
   bodies and assumes they need no captured values. Prefer loading the actual
   Error/Globals/Cell initializers on its Store-absent floor, as the newer task
   absence fixture already does. Preserve its extra slot/record/registry/harness
   coverage, rather than dropping assertions to fit the new implementation.
4. Repeat Cell/GC/image/alias and socket/interaction acceptance and paired timings.
   Commit the runtime slice only when performance is qualified; report any
   significant remaining cost before proceeding further.

This targets shared access overhead without duplicating Cell as a socket-specific
native or authored storage type. It is a proposal, not a delivered speedup.

Raw timing, census, source hashes and acceptance references are in the
[receipt](../measurements/socket-owner-regression-20261019.json).
