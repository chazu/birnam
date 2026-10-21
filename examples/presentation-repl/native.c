/* Terminal mechanics only. The editor, transcript and values live in Birnam.
 * Build with the existing first-party Termbox adapter, not a second terminal
 * implementation. No Birnam pointers or callbacks cross this interface. */
#include "../../wrapped/termbox2/termbox2.c"
#include <locale.h>
#include <wchar.h>
#include <stdio.h>

static FILE *capture;
static int saved_out = -1, saved_err = -1;
static char output[16385];

int pr_init(void) {
    const char *locale = setlocale(LC_CTYPE, "");
    if (!locale || !strcmp(locale, "C") || !strcmp(locale, "POSIX")) {
        // macOS does not provide C.UTF-8, often inherited from dev shells.
        setlocale(LC_CTYPE, "en_US.UTF-8");
    }
    return lg_init();
}

int pr_poll(void) {
    int result;
    do {
        result = tb_poll_event(&lg_ev);
    } while (result == TB_ERR_POLL &&
             (tb_last_errno() == EINTR || tb_last_errno() == EAGAIN));
    return result;
}

const char *pr_text(void) {
    static char text[5];
    memset(text, 0, sizeof(text));
    if (lg_ev.ch) tb_utf8_unicode_to_char(text, lg_ev.ch);
    return text;
}

int pr_previous(void *self, const char *s, int pos) {
    (void)self;
    int n = (int)strlen(s);
    if (pos > n) pos = n;
    if (pos > 0) --pos;
    while (pos > 0 && ((unsigned char)s[pos] & 0xc0) == 0x80) --pos;
    return pos < 0 ? 0 : pos;
}

int pr_next(void *self, const char *s, int pos) {
    (void)self;
    int n = (int)strlen(s);
    if (pos < 0) pos = 0;
    if (pos < n) ++pos;
    while (pos < n && ((unsigned char)s[pos] & 0xc0) == 0x80) ++pos;
    return pos > n ? n : pos;
}

/* Decode safely; controls become visible glyphs, never terminal escapes.
 * Cursor movement is by Unicode codepoint, not grapheme cluster, in this slice. */
static int decode(const char *s, size_t n, uint32_t *ch, int *width) {
    wchar_t wc;
    mbstate_t state = {0};
    size_t used = mbrtowc(&wc, s, n, &state);
    if (used == (size_t)-1 || used == (size_t)-2 || used == 0) {
        *ch = 0xfffd; *width = 1; return 1;
    }
    *ch = (uint32_t)wc;
    *width = wcwidth(wc);
    if (wc == '\n') { *ch = 0x21b5; *width = 1; }
    else if (*width < 0) { *ch = 0xfffd; *width = 1; }
    // This adapter uses the basic cell API, so show combining marks explicitly.
    else if (*width == 0) { *ch = 0x25cc; *width = 1; }
    return (int)used;
}

/* One whole row per alien call, with native clipping and horizontal scrolling.
 * cursor < 0 draws a transcript row; otherwise it is a UTF-8 byte offset. */
static int draw_line(int y, int fg, int bg, const char *s, size_t n, int cursor) {
    int w = tb_width();
    if (y < 0 || y >= tb_height() || w < 1) return 0;
    for (int x = 0; x < w; ++x) tb_set_cell(x, y, ' ', fg, bg);
    size_t p = 0;
    int column = 0, caret = 0;
    if (cursor >= 0) {
        while (p < n && p < (size_t)cursor) {
            uint32_t ch; int width;
            p += decode(s + p, n - p, &ch, &width); caret += width;
        }
    }
    int left = caret >= w ? caret - w + 1 : 0;
    p = 0;
    while (p < n && column - left < w) {
        uint32_t ch; int width;
        p += decode(s + p, n - p, &ch, &width);
        int x = column - left;
        if (x >= 0 && x + width <= w) tb_set_cell(x, y, ch, fg, bg);
        column += width;
    }
    if (cursor >= 0) tb_set_cursor(caret - left, y);
    return 0;
}

int pr_line(void *self, int y, int fg, int bg, const char *s, int cursor) {
    (void)self;
    return draw_line(y, fg, bg, s, strlen(s), cursor);
}

/* A cached plain-text row packet, prepared by Birnam only when content changes.
 * Birnam chooses the viewport and selection. Native code only clips/blits it. */
int pr_rows(void *self, const char *s, int top, int selected) {
    (void)self;
    const char *end = s + strlen(s);
    for (int i = 0; i < top && s < end; ++i) {
        const char *next = memchr(s, '\n', (size_t)(end - s));
        s = next ? next + 1 : end;
    }
    for (int row = 0; row < tb_height() - 4; ++row) {
        const char *next = memchr(s, '\n', (size_t)(end - s));
        const char *stop = next ? next : end;
        draw_line(row + 1, 8, row == selected ? 5 : 0, s, (size_t)(stop - s), -1);
        s = next ? next + 1 : end;
    }
    return 0;
}

/* A file avoids a pipe filling while synchronous evaluation owns the thread.
 * stdout/stderr share one stream in the transcript; retain only the first 16KiB.
 * Restore descriptors before Birnam receives any result, including an Error. */
int pr_capture_begin(void) {
    if (capture) return -1;
    fflush(NULL);
    capture = tmpfile();
    if (!capture) return -1;
    saved_out = dup(STDOUT_FILENO); saved_err = dup(STDERR_FILENO);
    if (saved_out < 0 || saved_err < 0) goto failed;
    if (dup2(fileno(capture), STDOUT_FILENO) < 0) goto failed;
    if (dup2(fileno(capture), STDERR_FILENO) < 0) goto failed;
    return 0;
failed:
    if (saved_out >= 0) { dup2(saved_out, STDOUT_FILENO); close(saved_out); }
    if (saved_err >= 0) { dup2(saved_err, STDERR_FILENO); close(saved_err); }
    fclose(capture); capture = NULL; saved_out = saved_err = -1;
    return -1;
}

const char *pr_capture_end(void) {
    output[0] = 0;
    if (!capture) return output;
    fflush(NULL);
    dup2(saved_out, STDOUT_FILENO); dup2(saved_err, STDERR_FILENO);
    close(saved_out); close(saved_err); saved_out = saved_err = -1;
    rewind(capture);
    size_t n = fread(output, 1, sizeof(output) - 1, capture);
    int extra = fgetc(capture);
    fclose(capture); capture = NULL;
    // Foreign String returns are NUL terminated; make embedded NULs visible.
    for (size_t i = 0; i < n; ++i) if (!output[i]) output[i] = '?';
    if (extra != EOF && n >= 20) memcpy(output + n - 20, " [output truncated] ", 20);
    output[n] = 0;
    return output;
}

int pr_shutdown(void) {
    pr_capture_end();
    return lg_shutdown();
}
