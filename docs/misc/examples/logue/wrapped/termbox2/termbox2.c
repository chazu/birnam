/* termbox2.c — int-clean FFI adapter over termbox2 for birnam.
 *
 * termbox2 is a single-header library. This translation unit instantiates its
 * implementation (TB_IMPL) AND exposes a thin `lg_*` adapter whose every entry
 * point is int/cstring-clean, so birnam's `wrap:as:signature:` surface never has
 * to deal with:
 *
 *   - `uintattr_t` fg/bg, which default to uint64_t (TB_OPT_ATTR_W=64) — a width
 *     a birnam SmallInt can't be marshalled to via the `i` (sint32) sigchar;
 *   - the `struct tb_event` out-param of tb_poll_event — read here into a static
 *     struct and surfaced as scalar accessors (lg_ev_*), so birnam never peeks
 *     struct fields at byte offsets.
 *
 * The adapter casts birnam ints up to the library's real types. This keeps the
 * .birnam wrapper trivial (every call is int->int or (...,cstring)->int) and is
 * standard FFI adapter practice — still "no zig", just a thicker C shim.
 */
#define TB_IMPL
#include "termbox2.h"

/* Last event polled by lg_poll, surfaced field-by-field below. */
static struct tb_event lg_ev;

/* --- lifecycle / screen ------------------------------------------------- */
int lg_init(void)        { return tb_init(); }
int lg_shutdown(void)    { return tb_shutdown(); }
int lg_width(void)       { return tb_width(); }
int lg_height(void)      { return tb_height(); }
int lg_clear(void)       { return tb_clear(); }
int lg_present(void)     { return tb_present(); }
int lg_hide_cursor(void) { return tb_hide_cursor(); }

/* --- drawing ------------------------------------------------------------ */
/* One glyph: ch is a Unicode codepoint, fg/bg are TB_* colour codes. */
int lg_set_cell(int x, int y, int ch, int fg, int bg) {
    return tb_set_cell(x, y, (uint32_t)ch, (uintattr_t)fg, (uintattr_t)bg);
}
/* A whole NUL-terminated string in one call (termbox handles advancing x). */
int lg_print(int x, int y, int fg, int bg, const char *s) {
    return tb_print(x, y, (uintattr_t)fg, (uintattr_t)bg, s);
}

/* --- input -------------------------------------------------------------- */
/* Blocking poll. Returns the event type (>0) or a negative tb error code.
 * The filled event is stashed in lg_ev; read it with the accessors below. */
int lg_poll(void)    { return tb_poll_event(&lg_ev); }
int lg_ev_type(void) { return (int)lg_ev.type; } /* TB_EVENT_* */
int lg_ev_key(void)  { return (int)lg_ev.key; }  /* TB_KEY_*  (0 if a char)  */
int lg_ev_ch(void)   { return (int)lg_ev.ch; }   /* codepoint (0 if special) */
