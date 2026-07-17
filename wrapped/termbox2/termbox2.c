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
int lg_set_cursor(void *self, int x, int y) {
    (void)self;
    return tb_set_cursor(x, y);
}

/* --- drawing ------------------------------------------------------------ */
/* These two take arguments, so under birnam's Alien FFI the method receiver is
 * marshalled as the FIRST C argument. They are module-level calls (the receiver
 * is the `tb` singleton, carrying no data), so the leading `self` is a dummy the
 * wrapper passes and we ignore — it just shifts the real args into place. The
 * no-argument entry points above need no such slot (their receiver is unused). */
/* One glyph: ch is a Unicode codepoint, fg/bg are TB_* colour codes. */
int lg_set_cell(void *self, int x, int y, int ch, int fg, int bg) {
    (void)self;
    return tb_set_cell(x, y, (uint32_t)ch, (uintattr_t)fg, (uintattr_t)bg);
}
/* A whole NUL-terminated string in one call (termbox handles advancing x). */
int lg_print(void *self, int x, int y, int fg, int bg, const char *s) {
    (void)self;
    return tb_print(x, y, (uintattr_t)fg, (uintattr_t)bg, s);
}

/* --- input -------------------------------------------------------------- */
/* Blocking poll. Returns the event type (>0) or a negative tb error code.
 * The filled event is stashed in lg_ev; read it with the accessors below. */
int lg_poll(void)    { return tb_poll_event(&lg_ev); }
int lg_ev_type(void) { return (int)lg_ev.type; } /* TB_EVENT_* */
int lg_ev_key(void)  { return (int)lg_ev.key; }  /* TB_KEY_*  (0 if a char)  */
int lg_ev_ch(void)   { return (int)lg_ev.ch; }   /* codepoint (0 if special) */

/* Return the last character as a short UTF-8 string. Alien copies cstring
 * returns into an image String, so the static buffer is safe between polls. */
const char *lg_ev_char(void) {
    static char out[5];
    uint32_t ch = lg_ev.ch;
    int n = 0;
    if (ch <= 0x7f) {
        out[n++] = (char)ch;
    } else if (ch <= 0x7ff) {
        out[n++] = (char)(0xc0 | (ch >> 6));
        out[n++] = (char)(0x80 | (ch & 0x3f));
    } else if (ch <= 0xffff) {
        out[n++] = (char)(0xe0 | (ch >> 12));
        out[n++] = (char)(0x80 | ((ch >> 6) & 0x3f));
        out[n++] = (char)(0x80 | (ch & 0x3f));
    } else if (ch <= 0x10ffff) {
        out[n++] = (char)(0xf0 | (ch >> 18));
        out[n++] = (char)(0x80 | ((ch >> 12) & 0x3f));
        out[n++] = (char)(0x80 | ((ch >> 6) & 0x3f));
        out[n++] = (char)(0x80 | (ch & 0x3f));
    }
    out[n] = '\0';
    return out;
}
