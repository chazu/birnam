/* termbox2.c — int-clean FFI adapter over termbox2 for Birnam.
 *
 * termbox2 is a single-header library. This translation unit instantiates its
 * implementation and exposes a small adapter whose entry points use only ints
 * and C strings. Birnam's Alien module therefore does not need to marshal
 * uintattr_t or inspect struct tb_event layouts.
 */
#define TB_IMPL
#include "termbox2.h"

static struct tb_event lg_ev;

int lg_init(void)        { return tb_init(); }
int lg_shutdown(void)    { return tb_shutdown(); }
int lg_width(void)       { return tb_width(); }
int lg_height(void)      { return tb_height(); }
int lg_clear(void)       { return tb_clear(); }
int lg_present(void)     { return tb_present(); }
int lg_hide_cursor(void) { return tb_hide_cursor(); }

/* Alien passes the method receiver as the first C argument for calls with
 * arguments. It is a stateless module singleton, so these adapters ignore it. */
int lg_set_cell(void *self, int x, int y, int ch, int fg, int bg) {
    (void)self;
    return tb_set_cell(x, y, (uint32_t)ch, (uintattr_t)fg, (uintattr_t)bg);
}

int lg_print(void *self, int x, int y, int fg, int bg, const char *s) {
    (void)self;
    return tb_print(x, y, (uintattr_t)fg, (uintattr_t)bg, s);
}

int lg_poll(void)    { return tb_poll_event(&lg_ev); }
int lg_ev_type(void) { return (int)lg_ev.type; }
int lg_ev_key(void)  { return (int)lg_ev.key; }
int lg_ev_ch(void)   { return (int)lg_ev.ch; }
