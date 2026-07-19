//! Native Termbox module for Digla's latency-sensitive terminal startup.
//!
//! This is an application wrapper, not a language-floor primitive. It binds the
//! existing int-clean C adapter directly through IdApi so loading Termbox does
//! not compile an Alien trampoline and then install a Birnam forwarding method
//! for every operation.

const std = @import("std");
const idapi = @import("idapi");

const Oop = idapi.Oop;

extern fn lg_init() c_int;
extern fn lg_shutdown() c_int;
extern fn lg_width() c_int;
extern fn lg_height() c_int;
extern fn lg_clear() c_int;
extern fn lg_present() c_int;
extern fn lg_hide_cursor() c_int;
extern fn lg_poll() c_int;
extern fn lg_ev_type() c_int;
extern fn lg_ev_mod() c_int;
extern fn lg_ev_key() c_int;
extern fn lg_ev_ch() c_int;
extern fn lg_set_cursor(self: ?*anyopaque, x: c_int, y: c_int) c_int;
extern fn lg_set_cell(self: ?*anyopaque, x: c_int, y: c_int, ch: c_int, fg: c_int, bg: c_int) c_int;
extern fn lg_print(self: ?*anyopaque, x: c_int, y: c_int, fg: c_int, bg: c_int, text: [*:0]const u8) c_int;
extern fn lg_ev_char() [*:0]const u8;

var g_api: *idapi.IdApi = undefined;
var termbox_vt: Oop = null;
var termbox: Oop = null;

fn int(value: Oop) c_int {
    return @intCast(g_api.smallIntValue(value));
}

fn result(value: c_int) Oop {
    return g_api.mkSmallInt(value);
}

fn mInit(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_init());
}
fn mShutdown(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_shutdown());
}
fn mWidth(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_width());
}
fn mHeight(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_height());
}
fn mClear(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_clear());
}
fn mPresent(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_present());
}
fn mHideCursor(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_hide_cursor());
}
fn mPoll(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_poll());
}
fn mEvType(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_ev_type());
}
fn mEvMod(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_ev_mod());
}
fn mEvKey(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_ev_key());
}
fn mEvCh(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_ev_ch());
}
fn mSetCursor(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_set_cursor(null, int(args[0]), int(args[1])));
}
fn mSetCell(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return result(lg_set_cell(null, int(args[0]), int(args[1]), int(args[2]), int(args[3]), int(args[4])));
}
fn mPrint(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    var len: usize = 0;
    const bytes = g_api.stringBytes(args[4], &len);
    const text = std.heap.c_allocator.allocSentinel(u8, len, 0) catch return g_api.nilObj;
    defer std.heap.c_allocator.free(text);
    @memcpy(text[0..len], bytes[0..len]);
    return result(lg_print(null, int(args[0]), int(args[1]), int(args[2]), int(args[3]), text.ptr));
}
fn mEvChar(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    const text = std.mem.span(lg_ev_char());
    return g_api.mkString(text.ptr, text.len);
}

fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const reloading = api.isExported("Termbox") != 0;
    if (reloading) {
        termbox = api.import("Termbox");
        termbox_vt = api.import("TermboxVt");
    } else {
        termbox_vt = api.proto(api.object_vt);
        termbox = api.alloc(termbox_vt, 0);
        api.setOwner(termbox_vt, termbox);
        api.setLayout(termbox_vt, null, 0, idapi.slot_raw);
        _ = api.exportObj("TermboxVt", termbox_vt);
        _ = api.exportObj("Termbox", termbox);
    }

    api.method(termbox_vt, api.intern("init"), &mInit);
    api.method(termbox_vt, api.intern("shutdown"), &mShutdown);
    api.method(termbox_vt, api.intern("width"), &mWidth);
    api.method(termbox_vt, api.intern("height"), &mHeight);
    api.method(termbox_vt, api.intern("clear"), &mClear);
    api.method(termbox_vt, api.intern("present"), &mPresent);
    api.method(termbox_vt, api.intern("hideCursor"), &mHideCursor);
    api.method(termbox_vt, api.intern("poll"), &mPoll);
    api.method(termbox_vt, api.intern("evType"), &mEvType);
    api.method(termbox_vt, api.intern("evMod"), &mEvMod);
    api.method(termbox_vt, api.intern("evKey"), &mEvKey);
    api.method(termbox_vt, api.intern("evCh"), &mEvCh);
    api.method(termbox_vt, api.intern("setCursor:y:"), &mSetCursor);
    api.method(termbox_vt, api.intern("setX:y:ch:fg:bg:"), &mSetCell);
    api.method(termbox_vt, api.intern("printX:y:fg:bg:str:"), &mPrint);
    api.method(termbox_vt, api.intern("evChar"), &mEvChar);
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}

comptime {
    @export(&moduleInit, .{ .name = "module_init" });
}
