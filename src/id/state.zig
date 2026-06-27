//! The minimal shared state the siphon hands each module's `entrypoint` — the
//! libid analog of the C `void *state`, given a concrete (but tiny) shape.
//!
//! Pure id: opaque object pointers + a couple of fn-pointers. **No eids, no fact
//! store, no IdApi.** libid's `entrypoint` fills this in; the siphon and every
//! later module (peg, …) drive the object system through it. It is an `extern
//! struct` of pointers, so one definition compiled into each binary has an
//! identical layout across the dylib boundary (same trick as `module_abi.zig`).

/// Ordinary object pointer (an object's vtable lives at `[-1]`).
pub const Oop = ?*anyopaque;

pub const State = extern struct {
    /// send(receiver, selector, args, n_args) -> result.
    /// `selector` is an interned symbol (as an `Oop`); `args` is a flat buffer.
    send: ?*const fn (Oop, Oop, [*]const Oop, usize) callconv(.c) Oop = null,
    /// intern(cstr) -> the unique symbol object for that string (as an `Oop`).
    intern: ?*const fn ([*:0]const u8) callconv(.c) Oop = null,
    /// The well-known vtables, so later modules can root their own classes.
    vtable_vt: Oop = null,
    object_vt: Oop = null,
    symbol_vt: Oop = null,
};

/// A module's entry symbol: `entrypoint(state)`, matching the C model.
pub const EntrypointFn = *const fn (*State) callconv(.c) void;
