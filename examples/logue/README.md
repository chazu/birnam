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
  birnam.toml                  project config: modules + [[wrapped]] termbox2
  logue-core.birnam            ALL game logic — no termbox; headless-testable
  logue.birnam                 thin render + input layer (the only file using `tb`)
  logue-test.birnam            headless test: deterministic seed + assertions
  wrapped/termbox2/
    termbox2.h                 vendored upstream single header (MIT)
    termbox2.c                 TB_IMPL + int-clean `lg_*` FFI adapter
    termbox2.birnam            the wrapper: Module load: + wrap:as:signature:
    libtermbox2.dylib          built by `birnam wrap` (git-ignored)
```

The split is deliberate: **`logue-core.birnam` contains no terminal code at all**,
so the whole game (generation, combat, hunger, inventory, level changes, the win
condition) is exercised by `logue-test.birnam` with a fixed RNG seed and
assertions — no TTY required. `logue.birnam` is only the render/input skin.

## Build & play

From this directory:

```sh
./play.sh          # runs the headless test suite (logue-test.birnam)
```

which files in `logue-core.birnam` (via `[System load: …]`) and runs 26
assertions against the full game logic — no TTY required. This is the runnable,
green path today.

> **The interactive renderer (`logue.birnam`) does not currently run.** It
> depends on the wrapped-C-library / termbox2 FFI subsystem — the `birnam wrap`
> command, `Module load:` of a raw `.dylib`, and the `wrap:as:signature:` facade
> — which has since been retired from birnam. Raw-C FFI now goes through
> `Alien dlopen:` + `wrap:signature:as:on:`. Once `termbox2.birnam` is
> re-expressed over `Alien`, the renderer (whose game/loop/symbol idioms are
> already up to date) should light up again. It also needs a real terminal
> (termbox opens `/dev/tty`).

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

termbox2 is a single-header library, so `wrapped/termbox2/termbox2.c` is a 2-line
instantiation shim (`#define TB_IMPL` + `#include "termbox2.h"`) **plus** a thin
int-clean adapter (`lg_*`). The adapter exists so birnam's `wrap:as:signature:`
never has to deal with two awkward edges of the real API:

- `tb_set_cell`/`tb_print` take `uintattr_t` colors which **default to 64-bit**
  (`TB_OPT_ATTR_W=64`) — a width a birnam `SmallInt` can't be marshalled to via
  the `i` sigchar. The adapter takes `int` and casts up.
- `tb_poll_event` fills a `struct tb_event` out-param. Rather than peek struct
  fields at byte offsets from birnam, the adapter polls into a static struct and
  exposes scalar accessors (`lg_ev_type` / `lg_ev_key` / `lg_ev_ch`).

The result is that every wrapped call is `int -> int` or `(ints…, cstring) -> int`
— see `termbox2.birnam`. This "thin C adapter over a single-header lib" is the
clean pattern for wrapping header-only C from birnam without any zig.
