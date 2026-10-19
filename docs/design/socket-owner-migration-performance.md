# Socket ownership migration: regression and qualified remediation

2026-10-19. Baseline `857b57e8`. The initial draft was held under the user's performance-regression rule.
The user approved the Cell remediation, which is now qualified. The failed
measurements below remain diagnostic history.

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
regression. This regression prevented committing the initial runtime draft.

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

## Approved remediation

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

The user approved this remediation. It targets shared access overhead without
duplicating Cell as a socket-specific native or authored storage type. The
implementation captures the receiver/index and retains live dispatch; its
qualification is recorded below. The regression measurements above describe the
initial draft, not the approved remediation.

Raw timing, census, source hashes and acceptance references are in the
[receipt](../measurements/socket-owner-regression-20261019.json).


## Qualified remediation

Cell now captures ObjectModel and its immutable value-slot index. `with:` still
sends current `clone` and `set:`; accessors still send to current methods on the
captured receiver. A warmed getter observed a collecting replacement of that
receiver's `slot:in:` method, and constructor probes observed replaced clone/set
methods. No primitive layout is exposed to Socket.

The Cell isolation fixture now loads actual Error/Globals/Cell initializers,
including real lexical captures. Its base and combined reflection/record/registry/
harness modes pass, retaining their former behavioral and reclamation checks.

Final paired measurements against the pre-socket baseline:

| Batch | Before | Qualified | Change |
| --- | ---: | ---: | ---: |
| 500 fd reads | 138.44 ms | 130.27 ms | -5.9% |
| 100 wraps/closes | 56.63 ms | 60.31 ms | +6.5% |
| Facts added by wraps/closes | 200 | 0 | Removed |
| Startup wall median | 2.908 s | 2.510 s | -13.7% |
| Logue quiet-turn CPU median | 26.285 ms | 26.032 ms | -1.0% |

The initial 25% regression is reduced to a small remaining ownership cost.
Creating the extra shared owner still has a cost: the untimed wrap census is
2,956 sends versus 2,350 before. Close is now 1,620 versus 1,638; Cell value falls
from 620 to 165 and Cell with: from 872 to 417. These counts are explanatory,
not CPU timings. No general speedup or changed GC/scheduling policy is claimed.

All 847 language checks, 177 task checks, interaction/TCP checks, socket resource
and image tests, actual-initializer absence modes, and local Cell retention/image
checks pass. The resource/slot/rooting formal suite remains 405 proofs, 128 rejected
mutations and 277 expected TLC outcomes. See the
[qualified receipt](../measurements/socket-owned-resources-20261019.json).
