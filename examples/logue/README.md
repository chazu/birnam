# logue — a roguelike in birnam

A self-contained birnam sub-project: a small but complete roguelike played in the
terminal over a **vendored, wrapped [termbox2](https://github.com/termbox/termbox2)**
(MIT). Descend five procedurally-generated dungeon levels, fight monsters, manage
hunger and equipment, and retrieve the **Star of Birnam** from the bottom — then
climb back out alive to win.

```
            L O G U E  —  the Dungeon of Birnam

    #############       ........        Depth 3   HP 24/30   Hunger Peckish
    #...........#########.......#        Gold 41   Fighter L2
    #....@......+.......g.......#        Weapon sword (4)  Armor chainmail (3)
    #....%......#########.!.....#        Rat 1  Pot 1
    #####.######       #........#        You hit the goblin (7).
        #.#                #....>#        The goblin hits you (3).
```

## Features

- **Five dungeon levels**, procedurally generated (rooms joined by L-corridors),
  with stairs `>`/`<` between them.
- **Three character classes** — Fighter (tough), Rogue (crits, richer), Wizard
  (frail, casts `z` zap with mana).
- **Equipment** — weapons and armor found in the dungeon auto-equip when better;
  stats feed combat.
- **Hunger** — a food clock; eat rations (`e`) or starve and lose HP.
- **Combat** — bump-to-attack, monster AI that chases and hits back, XP and
  level-ups, monsters scaling by depth (rat → goblin → orc → ogre → wraith).
- **The macguffin** — the Star of Birnam waits on level 5. Grab it and escape via
  the level-1 up-stairs to win.

## Layout

```
logue/
  birnam.toml                  project notes for the example
  logue-core.birnam            ALL game logic — no termbox; headless-testable
  logue.birnam                 thin render + input layer (the only file using `tb`)
  logue-test.birnam            headless test: deterministic seed + assertions
../../wrapped/termbox2/
  termbox2.h                   vendored upstream single header (MIT)
  termbox2.c                   TB_IMPL + int-clean `lg_*` FFI adapter
  termbox2.birnam              the reusable Alien-backed `Termbox` module
  build.sh                     cc the adapter into libtermbox2.dylib
  libtermbox2.dylib            built by build.sh (git-ignored)
```

The split is deliberate: **`logue-core.birnam` contains no terminal code at all**,
so the whole game (generation, combat, hunger, inventory, level changes, the win
condition) is exercised by `logue-test.birnam` with a fixed RNG seed and
assertions — no TTY required. `logue.birnam` is only the render/input skin.

## Build & play

From this directory, on a real terminal:

```sh
./play.sh          # builds libtermbox2.dylib if needed, then plays
```

`play.sh` builds the termbox2 dylib on first run (`../../wrapped/termbox2/build.sh`)
and launches `logue.birnam`, which files in the wrapper and `logue-core.birnam`
via `[System load: …]`. From this repo without an installed birnam:

```sh
BIRNAM=../../zig-out/bin/birnam ./play.sh
```

> **A real terminal is required** — termbox2 opens `/dev/tty`, so the renderer
> cannot run under a pipe or without a controlling tty (there, `tb_init` fails
> and there is nothing to play). The full game *logic* has no such dependency:
> `logue-test.birnam` exercises it headlessly (see below).

### A note on rendering speed

birnam is interpreted and each wrapped FFI call costs ~0.25 ms, so repainting all
60×18 cells every turn is ~1.3 s — unplayable. `logue.birnam` therefore uses
**dirty rendering**: the static map is drawn in full only on a level change, and
each turn repaints just the cells entities cover (player + monsters + items).
That makes a turn ~20 ms (snappy). The trade-off is a brief (~1 s) pause when you
take the stairs and the whole new level is painted — expected, not a hang.

### Controls

```
move    h j k l   (left/down/up/right)      y u b n  (diagonals)      or arrow keys
stairs  >  descend      <  ascend / escape
items   e  eat a ration       q  quaff a potion
magic   z  zap an adjacent foe (Wizard only)
wait    .          quit  Q / Esc
```

## Run the headless tests

```sh
birnam logue-core.birnam logue-test.birnam
```

Expected tail: `RESULT: 26 checks, 0 failures` — covering generation, hunger,
eating, quaffing, combat, descent, every class, and the macguffin/win path.

## How termbox2 is wrapped

termbox2 is a single-header library, so `../../wrapped/termbox2/termbox2.c` is a 2-line
instantiation shim (`#define TB_IMPL` + `#include "termbox2.h"`) **plus** a thin
int-clean adapter (`lg_*`). The adapter exists so birnam's `Alien` FFI
(`wrap:signature:as:on:`) never has to deal with two awkward edges of the real API:

- `tb_set_cell`/`tb_print` take `uintattr_t` colors which **default to 64-bit**
  (`TB_OPT_ATTR_W=64`) — a width a birnam `SmallInt` can't be marshalled to via
  the `i` sigchar. The adapter takes `int` and casts up.
- `tb_poll_event` fills a `struct tb_event` out-param. Rather than peek struct
  fields at byte offsets from birnam, the adapter polls into a static struct and
  exposes scalar accessors (`lg_ev_type` / `lg_ev_key` / `lg_ev_ch`).

There is a third, FFI-shaped detail: `Alien` marshals a method's **receiver as
the first C argument**. The no-argument calls (`init`, `clear`, `poll`, …) don't
care — their receiver is unused. But the two drawing calls take arguments, so
`lg_set_cell`/`lg_print` carry a leading `void *self` the wrapper fills with the
`Termbox` module object and the adapter ignores; it just shifts the real arguments
into place (hence the `p` after the return type in those signatures).

The result is that every wrapped call is `int -> int` or `(ints…, cstring) -> int`
— see `termbox2.birnam`. `Termbox` itself is a named singleton (`[Object clone]`
with its own copy-on-write vtable), so wrapping these methods `on: Termbox` keeps
them off `Object`. This "thin C adapter over a single-header lib" is the clean
pattern for wrapping header-only C from birnam without any zig.
