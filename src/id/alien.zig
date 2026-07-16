//! src/id/alien.zig — the FFI: wrap and absorb C libraries.
//!
//! birnam's compiler IS a C compiler (libtcc), so the FFI needs no separate
//! marshaling engine like libffi: it GENERATES a tiny C shim that unpacks birnam
//! arguments, calls the real C function, and packs the result back — then compiles
//! that shim with `Tcc` and installs it as a method. tcc resolves the C function
//! against the running process (libc and anything `dlopen`'d), and `addLibrary:`
//! links a named library when needed.
//!
//! Message api (driven through `api.send`):
//!   [Alien dlopen: pathString]                       load a shared library so its
//!                                                    symbols become resolvable
//!   [Alien libraryDir: pathString]                   resolve bare dlopen names
//!                                                    against a project build dir
//!   [Alien dlclose: pathString]                      release one such handle (§G10)
//!   [Alien wrap: cFnName signature: sig as: #sel on: vt]
//!                                                    wrap a C function as a method
//!   [Alien callback: aBlock signature: sig]          the OTHER direction: a C
//!                                                    function pointer that calls
//!                                                    back into birnam
//!   [Alien releaseCallback: addr]                    reclaim a callback (§G10)
//!
//! A SIGNATURE is a string: the first type is the RETURN, the rest are the
//! ARGUMENT types, one per birnam argument (whitespace ignored):
//!   i  int   (32-bit; an Integer)      s  const char* (a String; freed after call)
//!   l  long  (64-bit; an Integer)      p  void* (an opaque full-width Word)
//!   u  unsigned long (64-bit; an Integer if it fits i64, else a Word — §F F3)
//!   d  double (a Float)               f  float  (a Float; narrowed at the call)
//!   v  void  (return only; → nil)
//!   {…}  a struct BY VALUE — braces around its field types, e.g. "{ll}" =
//!        struct { long; long; }. Fields may be scalars (i/l/u/d/f/p — not s),
//!        nested structs ("{{ll}{ll}}"), or fixed SCALAR ARRAYS written
//!        <count><char> ("{l 3i}" = struct { long; int[3]; }).
//! e.g. "i i" = `int f(int)` (abs); "l s" = `long f(const char*)` (strlen);
//! "{ii} i i" = `div_t div(int, int)`; "{dd} {dd} d" = Vec2 f(Vec2, double).
//!
//! STRUCTS map to positional Vectors (structs are positional in C too): a struct
//! argument is a `#[…]` Vector with one element per FIELD, a struct return comes
//! back as a fresh Vector. The rule is uniform and recursive — every aggregate
//! (struct or array field) is ONE element holding its own Vector, so
//! `struct { long; int[3]; }` ⇄ `#[7 #[1 2 3]]` and a nested struct is a nested
//! Vector. The shim checks each Vector's `size` before untagging and answers nil
//! on a shape mismatch (no crash). Struct signatures need the collection module
//! loaded (the Vector factory is baked into the shim). Because the shim DECLARES
//! the real struct type and lets tcc compile the call, the platform ABI (arm64
//! HFA, sret, …) is entirely tcc's problem — no marshaling engine here.
//!
//! The RECEIVER is the FIRST C argument (the Smalltalk-natural idiom), so a
//! K-argument function becomes a method of arity K-1: `["hi" clen]` calls
//! `strlen("hi")` and `[-42 absVia]` calls `abs(-42)`. Wrap on the type whose
//! instances are that first argument (String for strlen, Integer for abs).
//!
//! CALLBACKS (`[Alien callback: aBlock signature: sig]`) close the loop: the same
//! signature grammar, read from C's side — the first type is what the C caller
//! expects BACK, the rest are what it passes IN. Alien generates a real C function
//! that marshals each C argument to an id value (structs → Vectors via the same
//! `__mk` helpers), sends `value`/`value:`/… to the block, and untags the answer;
//! it answers the function's address as a SmallInt (pass it wherever a `p` goes).
//! `aBlock` is ANYTHING answering the arity-matched `value…` selector — a compiled
//! birnam block or any object with the method. Because the callback's address is
//! baked into generated code the GC cannot see, Alien roots every (block, unit)
//! pair in a module registry: a callback lives until the process exits OR the caller
//! explicitly `[Alien releaseCallback: addr]`s it (§G10 — the opt-in release path).
//! An `s` RETURN is rejected (who would free the C string?); a mis-shaped struct
//! answer from the block comes back zeroed.
//!
//! The wrapped function's address is resolved by the process dynamic linker
//! (`dlsym`) and patched into the unit before relocation — the same mechanism that
//! binds the `birnam_*` shims — so resolution is reliable across platforms. For a
//! library not yet in the process, `[Alien dlopen: path]` loads it first.
//!
//! RAW MEMORY (§F F4): a Word is a machine address, so it doubles as a C buffer.
//! `[Alien alloc: n]` mints one over n zeroed bytes; `peek{Int,Byte,Ptr,Double,
//! String}At:` and `poke{Int,Byte,Ptr,Double}:at:` read/write typed values at a
//! byte offset; `[buf free]` reclaims it. This wraps a C function with a `T*`
//! out-param — allocate a buffer, pass the Word as its `p` argument, peek the
//! result — without writing any C.
//!
//! Depends on the IdApi + Tcc. The shim calls the standard `birnam_*` runtime shims
//! (send / intern / mkSmallInt / smallIntValue / mkFloat / floatValue / mkString /
//! nil / cstr / free_cstr) that `bindRuntime` provides, so no id struct layout is
//! ever spelled out.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;
const RTLD_LAZY: c_int = 1;

var g_api: *idapi.IdApi = undefined;
var tcc: Oop = undefined;

pub var alien_vt: Oop = null;
pub var alien_obj: Oop = null;

fn s0(rcv: Oop, sel: [*:0]const u8) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{}, 0);
}
fn s1(rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{a}, 1);
}

fn strBytes(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}
fn dupZ(o: Oop) [:0]u8 {
    const b = strBytes(o);
    return dupZBytes(b);
}
fn dupZBytes(b: []const u8) [:0]u8 {
    const z = ca.allocSentinel(u8, b.len, 0) catch @panic("alien: oom");
    @memcpy(z[0..b.len], b);
    return z;
}

// -- the signature model ------------------------------------------------------
//
// A parsed signature is a small type TREE, not a flat char scan: a type is a
// scalar, a struct (an "agg" of fields), or — inside a struct only — a fixed
// scalar array. Aggs live in a side list in POST-ORDER (an inner struct is
// registered before the struct containing it), so emitting typedefs/helpers in
// list order defines every C name before its first use.

const Ty = union(enum) {
    scalar: u8, // i l u d f s p (plus 'v', return position only)
    array: struct { count: usize, elem: u8 },
    agg: usize, // index into Parsed.aggs
};

const Agg = struct { fields: []Ty };

const Parsed = struct {
    ret: Ty,
    args: []Ty,
    aggs: []Agg,
};

/// The C type a scalar signature char denotes.
fn cType(sc: u8) ?[]const u8 {
    return switch (sc) {
        'i' => "int",
        'l' => "long",
        'u' => "unsigned long", // §F F3 — an unsigned 64-bit scalar (size_t/uint64_t)
        'd' => "double",
        'f' => "float",
        's' => "const char*",
        'p' => "void*",
        'v' => "void",
        else => null,
    };
}

/// Scalars legal as a struct FIELD or array element: no 's' (freeing char*
/// temporaries inside aggregates is deferred) and no 'v'.
fn fieldScalarOk(c: u8) bool {
    return switch (c) {
        'i', 'l', 'u', 'd', 'f', 'p' => true,
        else => false,
    };
}

const SigParser = struct {
    s: []const u8,
    i: usize = 0,
    arena: std.mem.Allocator,
    aggs: std.ArrayList(Agg) = .{},

    fn peek(p: *SigParser) ?u8 {
        return if (p.i < p.s.len) p.s[p.i] else null;
    }

    /// A struct field: nested struct, scalar array (`<count><char>`), or scalar.
    fn parseField(p: *SigParser) ?Ty {
        const c = p.peek() orelse return null;
        if (c == '{') return p.parseAgg();
        if (c >= '0' and c <= '9') {
            var n: usize = 0;
            while (p.peek()) |dgt| {
                if (dgt < '0' or dgt > '9') break;
                n = n * 10 + (dgt - '0');
                p.i += 1;
            }
            const e = p.peek() orelse return null;
            if (n == 0 or !fieldScalarOk(e)) return null; // 0-count, struct array, 's'/'v' array: rejected
            p.i += 1;
            return .{ .array = .{ .count = n, .elem = e } };
        }
        if (!fieldScalarOk(c)) return null;
        p.i += 1;
        return .{ .scalar = c };
    }

    /// `{field+}` — registered in `aggs` AFTER its fields (post-order).
    fn parseAgg(p: *SigParser) ?Ty {
        p.i += 1; // '{'
        var fields: std.ArrayList(Ty) = .{};
        while (true) {
            const c = p.peek() orelse return null; // unclosed brace
            if (c == '}') {
                p.i += 1;
                break;
            }
            const f = p.parseField() orelse return null;
            fields.append(p.arena, f) catch @panic("alien: oom");
        }
        if (fields.items.len == 0) return null; // empty struct
        const idx = p.aggs.items.len;
        p.aggs.append(p.arena, .{ .fields = fields.toOwnedSlice(p.arena) catch @panic("alien: oom") }) catch @panic("alien: oom");
        return .{ .agg = idx };
    }

    /// A top-level (return/argument) type: struct or scalar. Arrays cannot be
    /// passed or returned by value in C, so they are struct-field-only.
    fn parseTop(p: *SigParser, is_ret: bool) ?Ty {
        const c = p.peek() orelse return null;
        if (c == '{') return p.parseAgg();
        if (cType(c) == null) return null;
        if (c == 'v' and !is_ret) return null;
        p.i += 1;
        return .{ .scalar = c };
    }
};

/// Parse a despaced signature into its type tree, or null if invalid.
fn parseSig(arena: std.mem.Allocator, sig: []const u8) ?Parsed {
    var p = SigParser{ .s = sig, .arena = arena };
    const ret = p.parseTop(true) orelse return null;
    var args: std.ArrayList(Ty) = .{};
    while (p.i < p.s.len) {
        const t = p.parseTop(false) orelse return null;
        args.append(arena, t) catch @panic("alien: oom");
    }
    return .{
        .ret = ret,
        .args = args.toOwnedSlice(arena) catch @panic("alien: oom"),
        .aggs = p.aggs.toOwnedSlice(arena) catch @panic("alien: oom"),
    };
}

/// A despaced copy of `sig` (signatures may be written with spaces for
/// readability, e.g. "l s" or "{dd} {dd} d"). Caller owns it.
fn despace(sig: []const u8) []u8 {
    var b: std.ArrayList(u8) = .{};
    for (sig) |c| {
        if (c != ' ' and c != '\t') b.append(ca, c) catch @panic("alien: oom");
    }
    return b.toOwnedSlice(ca) catch @panic("alien: oom");
}

// -- the C emitter -------------------------------------------------------------

fn put(b: *std.ArrayList(u8), s: []const u8) void {
    b.appendSlice(ca, s) catch @panic("alien: oom");
}
fn putFmt(b: *std.ArrayList(u8), comptime fmt: []const u8, args: anytype) void {
    const s = std.fmt.allocPrint(ca, fmt, args) catch @panic("alien: oom");
    defer ca.free(s);
    b.appendSlice(ca, s) catch @panic("alien: oom");
}

/// The C spelling of a top-level type (scalar name or the `__S<n>` typedef).
fn putCTy(b: *std.ArrayList(u8), t: Ty) void {
    switch (t) {
        .scalar => |c| put(b, cType(c).?),
        .agg => |i| putFmt(b, "__S{d}", .{i}),
        .array => unreachable, // struct-field-only; parser rejects elsewhere
    }
}

/// Emit `<lhs> = <untag of e per scalar char>;`
fn putScalarUntag(b: *std.ArrayList(u8), lhs: []const u8, c: u8, e: []const u8) void {
    switch (c) {
        'i' => putFmt(b, "{s} = (int)birnam_smallIntValue({s});\n", .{ lhs, e }),
        'l' => putFmt(b, "{s} = birnam_smallIntValue({s});\n", .{ lhs, e }),
        // An unsigned reads its full 64 bits from either an Integer or a Word (§F F3).
        'u' => putFmt(b, "{s} = birnam_uintValue({s});\n", .{ lhs, e }),
        'd' => putFmt(b, "{s} = birnam_floatValue({s});\n", .{ lhs, e }),
        'f' => putFmt(b, "{s} = (float)birnam_floatValue({s});\n", .{ lhs, e }),
        // A pointer is opaque and full-width: it rides a boxed Word, not a 63-bit
        // SmallInt, so a high-bit address survives round-trip (§F F1/F4).
        'p' => putFmt(b, "{s} = (void*)birnam_wordValue({s});\n", .{ lhs, e }),
        else => unreachable,
    }
}

/// The C expression boxing scalar expression `x` (of scalar char `c`) as an Oop.
fn putScalarBox(b: *std.ArrayList(u8), c: u8, x: []const u8) void {
    switch (c) {
        'i', 'l' => putFmt(b, "birnam_mkSmallInt((long)({s}))", .{x}),
        // §F F3 large-int policy: an unsigned that fits signed i64 becomes a
        // first-class arithmetic Integer (the common case — every length/count/
        // size_t); one with the top bit set (≥ 2^63) rides a lossless Word.
        'u' => putFmt(b, "(((unsigned long)({s}) >> 63) ? birnam_mkWord((unsigned long)({s})) : birnam_mkSmallInt((long)({s})))", .{ x, x, x }),
        'p' => putFmt(b, "birnam_mkWord((unsigned long)({s}))", .{x}),
        'd', 'f' => putFmt(b, "birnam_mkFloat((double)({s}))", .{x}),
        else => unreachable,
    }
}

/// The `extern` preamble every generated unit needs: the `birnam_*` runtime shims
/// (bound by `bindRuntime`), strlen for string returns, and — for struct sigs —
/// the Vector factory, whose address is patched in via the pre-relocate
/// `addSymbol:value:` window, the same way a wrapped fn's is.
fn putPreamble(b: *std.ArrayList(u8), has_aggs: bool) void {
    put(b,
        \\extern void* birnam_send(void* rcv, void* sel, void** args, long n);
        \\extern void* birnam_intern(const char* name);
        \\extern void* birnam_mkSmallInt(long);
        \\extern long birnam_smallIntValue(void*);
        \\extern void* birnam_mkFloat(double);
        \\extern double birnam_floatValue(void*);
        \\extern void* birnam_mkWord(unsigned long);
        \\extern unsigned long birnam_wordValue(void*);
        \\extern unsigned long birnam_uintValue(void*);
        \\extern const char* birnam_cstr(void*);
        \\extern void birnam_free_cstr(const char*);
        \\extern void* birnam_mkString(const char*, long);
        \\extern void* birnam_nil(void);
        \\extern long strlen(const char*);
        \\
    );
    if (has_aggs) put(b, "extern char __vecFactory[];\n");
}

/// Emit the struct support a signature needs: typedefs plus the per-agg
/// `__unm<i>` (Vector→struct, shape-checked) and `__mk<i>` (struct→fresh Vector)
/// helpers. Shared by wrap shims (unm args / mk return) and callbacks (the same
/// helpers, used in the OPPOSITE directions).
fn putAggSupport(b: *std.ArrayList(u8), sig: Parsed) void {
    // Struct typedefs, inner-first (aggs is post-order).
    for (sig.aggs, 0..) |agg, i| {
        putFmt(b, "typedef struct {{ ", .{});
        for (agg.fields, 0..) |f, k| {
            switch (f) {
                .scalar => |c| putFmt(b, "{s} f{d}; ", .{ cType(c).?, k }),
                .array => |a| putFmt(b, "{s} f{d}[{d}]; ", .{ cType(a.elem).?, k, a.count }),
                .agg => |j| putFmt(b, "__S{d} f{d}; ", .{ j, k }),
            }
        }
        putFmt(b, "}} __S{d};\n", .{i});
    }

    // Per-agg unmarshal helpers: birnam Vector → C struct, shape-checked.
    // `__unm<i>` returns 0 on a shape mismatch. An inner agg's helper is defined
    // before any caller (post-order), so no forward declarations are needed.
    for (sig.aggs, 0..) |agg, i| {
        putFmt(b, "static int __unm{d}(void* v, __S{d}* out){{\n", .{ i, i });
        putFmt(b, "if (birnam_smallIntValue(birnam_send(v, birnam_intern(\"size\"), 0, 0L)) != {d}) return 0;\n", .{agg.fields.len});
        put(b, "void* e;\n");
        var lhs: [48]u8 = undefined;
        for (agg.fields, 0..) |f, k| {
            putFmt(b, "e = birnam_send(v, birnam_intern(\"at:\"), (void*[]){{birnam_mkSmallInt({d})}}, 1L);\n", .{k});
            switch (f) {
                .scalar => |c| putScalarUntag(b, std.fmt.bufPrint(&lhs, "out->f{d}", .{k}) catch unreachable, c, "e"),
                .agg => |j| putFmt(b, "if (!__unm{d}(e, &out->f{d})) return 0;\n", .{ j, k }),
                .array => |a| {
                    putFmt(b, "if (birnam_smallIntValue(birnam_send(e, birnam_intern(\"size\"), 0, 0L)) != {d}) return 0;\n", .{a.count});
                    putFmt(b, "{{ long __k; for (__k = 0; __k < {d}; __k++) {{\n", .{a.count});
                    put(b, "void* __ae = birnam_send(e, birnam_intern(\"at:\"), (void*[]){birnam_mkSmallInt(__k)}, 1L);\n");
                    putScalarUntag(b, std.fmt.bufPrint(&lhs, "out->f{d}[__k]", .{k}) catch unreachable, a.elem, "__ae");
                    put(b, "} }\n");
                },
            }
        }
        put(b, "return 1;\n}\n");
    }

    // Per-agg marshal helpers: C struct → a fresh birnam Vector (nested aggs and
    // array fields become nested Vectors — the uniform aggregate⇄Vector rule).
    for (sig.aggs, 0..) |agg, i| {
        putFmt(b, "static void* __mk{d}(__S{d}* v){{\n", .{ i, i });
        put(b, "void* r = birnam_send((void*)__vecFactory, birnam_intern(\"new\"), 0, 0L);\nvoid* e;\n");
        var x: [48]u8 = undefined;
        for (agg.fields, 0..) |f, k| {
            switch (f) {
                .scalar => |c| {
                    put(b, "e = ");
                    putScalarBox(b, c, std.fmt.bufPrint(&x, "v->f{d}", .{k}) catch unreachable);
                    put(b, ";\n");
                },
                .agg => |j| putFmt(b, "e = __mk{d}(&v->f{d});\n", .{ j, k }),
                .array => |a| {
                    putFmt(b, "{{ void* s = birnam_send((void*)__vecFactory, birnam_intern(\"new\"), 0, 0L); long __k;\nfor (__k = 0; __k < {d}; __k++) {{\n", .{a.count});
                    put(b, "void* __ae = ");
                    putScalarBox(b, a.elem, std.fmt.bufPrint(&x, "v->f{d}[__k]", .{k}) catch unreachable);
                    put(b, ";\ns = birnam_send(s, birnam_intern(\"add:\"), (void*[]){__ae}, 1L);\n} e = s; }\n");
                },
            }
            put(b, "r = birnam_send(r, birnam_intern(\"add:\"), (void*[]){e}, 1L);\n");
        }
        put(b, "return r;\n}\n");
    }
}

/// Generate the Method-ABI C shim wrapping `fn_name` per the parsed signature.
/// Caller owns the returned source.
fn genShim(fn_name: []const u8, sig: Parsed) []u8 {
    var b: std.ArrayList(u8) = .{};
    putPreamble(&b, sig.aggs.len > 0);
    putAggSupport(&b, sig);

    // The wrapped function's prototype.
    putCTy(&b, sig.ret);
    putFmt(&b, " {s}(", .{fn_name});
    if (sig.args.len == 0) {
        put(&b, "void");
    } else {
        for (sig.args, 0..) |t, i| {
            if (i != 0) put(&b, ", ");
            putCTy(&b, t);
        }
    }
    put(&b, ");\n");

    // The shim: unmarshal args, call, free temporaries, marshal the result.
    // The RECEIVER is the first C argument (the Smalltalk-natural idiom, e.g.
    // `["hi" clen]` = strlen("hi")); any further C args come from `args[]`. So a
    // K-argument C function is a method of arity K-1 (K=0 → unary, receiver unused).
    put(&b, "void* __shim(void* self, void** args, long n){ (void)self; (void)args; (void)n;\n");
    var srcbuf: [24]u8 = undefined;
    var lhsbuf: [24]u8 = undefined;
    for (sig.args, 0..) |t, i| {
        const src = if (i == 0) "self" else (std.fmt.bufPrint(&srcbuf, "args[{d}]", .{i - 1}) catch unreachable);
        switch (t) {
            .scalar => |c| {
                putFmt(&b, "{s} a{d};\n", .{ cType(c).?, i });
                if (c == 's') {
                    putFmt(&b, "a{d} = birnam_cstr({s});\n", .{ i, src });
                } else {
                    putScalarUntag(&b, std.fmt.bufPrint(&lhsbuf, "a{d}", .{i}) catch unreachable, c, src);
                }
            },
            .agg => |j| putFmt(&b, "__S{d} a{d};\nif (!__unm{d}({s}, &a{d})) return birnam_nil();\n", .{ j, i, j, src, i }),
            .array => unreachable,
        }
    }

    // The call.
    switch (sig.ret) {
        .scalar => |c| if (c == 'v') {
            putFmt(&b, "{s}(", .{fn_name});
        } else {
            putFmt(&b, "{s} __r = {s}(", .{ cType(c).?, fn_name });
        },
        .agg => |j| putFmt(&b, "__S{d} __r = {s}(", .{ j, fn_name }),
        .array => unreachable,
    }
    for (sig.args, 0..) |_, i| {
        if (i != 0) put(&b, ", ");
        putFmt(&b, "a{d}", .{i});
    }
    put(&b, ");\n");

    // Free any string temporaries (after the call has used them).
    for (sig.args, 0..) |t, i| {
        if (t == .scalar and t.scalar == 's') putFmt(&b, "birnam_free_cstr(a{d});\n", .{i});
    }

    // Marshal the result. Numeric/pointer scalars box via the shared emitter (so
    // 'u' gets the F3 Integer-or-Word policy for free); 's'/'v' are special.
    switch (sig.ret) {
        .scalar => |c| switch (c) {
            's' => put(&b, "return __r ? birnam_mkString(__r, strlen(__r)) : birnam_nil();\n"),
            'v' => put(&b, "return birnam_nil();\n"),
            else => {
                put(&b, "return ");
                putScalarBox(&b, c, "__r");
                put(&b, ";\n");
            },
        },
        .agg => |j| putFmt(&b, "return __mk{d}(&__r);\n", .{j}),
        .array => unreachable,
    }
    put(&b, "}\n");
    return b.toOwnedSlice(ca) catch @panic("alien: oom");
}

// -- callbacks: C → birnam ------------------------------------------------------

/// Everything a live callback keeps alive: the block it forwards to, the Tcc unit
/// holding its generated code, and the `__cb` entry address (so `releaseCallback:`
/// can find it). The GC cannot see the block/unit (the block's address is baked into
/// compiled C), so `cbRoots` marks the registry at collection time. A callback lives
/// until the process exits OR the caller explicitly `releaseCallback:`s it (§G10).
const CbEntry = struct { blk: Oop, unit: Oop, addr: u64 };
var g_callbacks: std.ArrayList(CbEntry) = .{};

fn cbRoots(mark: idapi.MarkFn) callconv(.c) void {
    for (g_callbacks.items) |e| {
        mark(e.blk);
        mark(e.unit);
    }
}

/// Generate the C callback function `__cb` for a signature READ FROM C'S SIDE:
/// the return is what the C caller expects back, the args are what it passes in.
/// The body boxes each C argument as an id value (structs → Vectors via `__mk`),
/// sends the arity-matched `value…` to the block baked in as `__cbBlock`, and
/// untags the answer (a struct answer via `__unm`; mis-shaped → zeroed).
fn genCallback(sig: Parsed) []u8 {
    var b: std.ArrayList(u8) = .{};
    putPreamble(&b, sig.aggs.len > 0);
    put(&b, "extern char __cbBlock[];\n");
    putAggSupport(&b, sig);

    putCTy(&b, sig.ret);
    put(&b, " __cb(");
    if (sig.args.len == 0) {
        put(&b, "void");
    } else {
        for (sig.args, 0..) |t, i| {
            if (i != 0) put(&b, ", ");
            putCTy(&b, t);
            putFmt(&b, " a{d}", .{i});
        }
    }
    put(&b, "){\n");

    // Box each C argument as an id value.
    var x: [24]u8 = undefined;
    for (sig.args, 0..) |t, i| {
        putFmt(&b, "void* b{d} = ", .{i});
        switch (t) {
            .scalar => |c| switch (c) {
                's' => putFmt(&b, "a{d} ? birnam_mkString(a{d}, strlen(a{d})) : birnam_nil()", .{ i, i, i }),
                else => putScalarBox(&b, c, std.fmt.bufPrint(&x, "a{d}", .{i}) catch unreachable),
            },
            .agg => |j| putFmt(&b, "__mk{d}(&a{d})", .{ j, i }),
            .array => unreachable,
        }
        put(&b, ";\n");
    }

    // The send: `value` / `value:` / `value:value:` / … per the C arity.
    put(&b, "void* r = birnam_send((void*)__cbBlock, birnam_intern(\"");
    if (sig.args.len == 0) {
        put(&b, "value");
    } else {
        for (sig.args) |_| put(&b, "value:");
    }
    put(&b, "\"), ");
    if (sig.args.len == 0) {
        put(&b, "0, 0L);\n");
    } else {
        put(&b, "(void*[]){");
        for (sig.args, 0..) |_, i| {
            if (i != 0) put(&b, ", ");
            putFmt(&b, "b{d}", .{i});
        }
        putFmt(&b, "}}, {d}L);\n", .{sig.args.len});
    }

    // Untag the answer for the C caller.
    switch (sig.ret) {
        .scalar => |c| switch (c) {
            'v' => put(&b, "(void)r;\n"),
            'i' => put(&b, "return (int)birnam_smallIntValue(r);\n"),
            'l' => put(&b, "return birnam_smallIntValue(r);\n"),
            'u' => put(&b, "return birnam_uintValue(r);\n"), // Integer or Word → u64 (§F F3)
            'p' => put(&b, "return (void*)birnam_wordValue(r);\n"),
            'd' => put(&b, "return birnam_floatValue(r);\n"),
            'f' => put(&b, "return (float)birnam_floatValue(r);\n"),
            else => unreachable, // 's' rejected in buildCallback
        },
        .agg => |j| putFmt(&b, "__S{d} __out = {{0}};\n__unm{d}(r, &__out);\nreturn __out;\n", .{ j, j }),
        .array => unreachable,
    }
    put(&b, "}\n");
    return b.toOwnedSlice(ca) catch @panic("alien: oom");
}

/// A kinded FFI error value (F7 error model, imported at call time), or nil if the
/// error module isn't loaded (docs/42 §C1). Every FFI failure used to collapse to an
/// indistinguishable silent nil; now the reason rides a `#kind` + message, so a
/// caller can tell "symbol not found" from "malformed signature" from "compile
/// failed", and the error PROPAGATES (DNU → self) instead of untagging into garbage.
fn alienError(comptime kind: [:0]const u8, message: []const u8) Oop {
    const ef = g_api.import("Error") orelse return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:message:"), &[_]Oop{
        g_api.intern(kind.ptr), g_api.mkString(message.ptr, message.len),
    }, 2);
}

/// Compile a callback for `blk` per `sig_raw` and answer its C function address as a
/// SmallInt (usable wherever a `p` argument goes), or a kinded error (§C1) if the
/// signature is invalid, returns a string, needs structs without the collection
/// module, or compilation fails. Roots (blk, unit) for the life of the process.
fn buildCallback(blk: Oop, sig_raw: []const u8) Oop {
    var arena_state = std.heap.ArenaAllocator.init(ca);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sigz = despace(sig_raw);
    defer ca.free(sigz);
    if (sigz.len == 0) return alienError("badSignature", "empty signature");
    const sig = parseSig(arena, sigz) orelse return alienError("badSignature", "malformed signature");
    if (sig.ret == .scalar and sig.ret.scalar == 's')
        return alienError("badSignature", "a callback cannot return a string (who would free it?)");

    var vec_factory: Oop = null;
    if (sig.aggs.len > 0) {
        vec_factory = g_api.import("Vector") orelse
            return alienError("moduleNotLoaded", "a struct signature needs the collection module");
    }

    const src = genCallback(sig);
    defer ca.free(src);
    if (std.posix.getenv("ALIEN_DUMP") != null) std.debug.print("[alien] callback:\n{s}\n", .{src});

    const unit = s0(tcc, "new");
    if (unit == g_api.nilObj) return alienError("compileFailed", "could not create a compile unit");
    if (s1(unit, "compile:", g_api.mkString(src.ptr, src.len)) == g_api.nilObj)
        return alienError("compileFailed", "the generated callback C did not compile");
    _ = s0(unit, "bindRuntime");
    addSym(unit, "__cbBlock", @intFromPtr(blk));
    if (vec_factory != null) addSym(unit, "__vecFactory", @intFromPtr(vec_factory));
    if (s0(unit, "relocate") == g_api.nilObj)
        return alienError("relocateFailed", "could not relocate the callback");

    const addr = s1(unit, "symbol:", g_api.mkString("__cb", 4)); // already a SmallInt
    if (addr == g_api.nilObj) return alienError("symbolNotFound", "the callback entry symbol was not found");
    g_callbacks.append(ca, .{ .blk = blk, .unit = unit, .addr = @bitCast(g_api.smallIntValue(addr)) }) catch @panic("alien: oom");
    return addr;
}

/// `[Alien releaseCallback: addr]` — the opt-in counterpart to `callback:signature:`,
/// closing the "callbacks live forever" leak (§G10). Drops the callback registered at
/// `addr` (a Word or Integer, as returned by `callback:signature:`): it leaves the
/// live set — so `cbRoots` stops pinning its block and the GC may reclaim it — and its
/// Tcc unit is marked `disposable`, so the collector `tcc_delete`s the generated code's
/// executable pages once the unit becomes unreachable. Answers true, or a kinded error
/// if nothing is registered there. UNSAFE like `[buf free]`: the caller ASSERTS the C
/// side will never invoke this function pointer again — a later call through a released
/// address is a use-after-free once its code has been reclaimed.
fn mReleaseCallback(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const want = argAddr(a[0]) orelse
        return alienError("typeMismatch", "releaseCallback: needs the callback address (a Word or Integer)");
    var i: usize = 0;
    while (i < g_callbacks.items.len) : (i += 1) {
        if (g_callbacks.items[i].addr == want) {
            const e = g_callbacks.swapRemove(i); // order in the registry is irrelevant
            _ = s0(e.unit, "disposable"); // GC finalizer → tcc_delete when unreachable
            return g_api.trueObj;
        }
    }
    return alienError("notFound", "releaseCallback: no live callback at that address");
}

/// `[Alien callback: aBlock signature: sig]`
fn mCallback(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return buildCallback(a[0], strBytes(a[1]));
}

// The process-wide symbol search handle: find a symbol in the main program or any
// loaded library. Its value is PLATFORM-SPECIFIC — the Darwin dyld ABI uses the
// sentinel `(void*)-2`, while glibc/musl define `RTLD_DEFAULT` as `(void*)0` (NULL).
// Getting this wrong makes every `wrap:` fail to resolve its symbol (audit §F2 —
// the old unconditional `-2` returned #symbolNotFound for everything on Linux).
const RTLD_DEFAULT: ?*anyopaque = switch (@import("builtin").os.tag) {
    .macos, .ios, .tvos, .watchos, .visionos => @ptrFromInt(@as(usize, @bitCast(@as(isize, -2)))),
    else => null, // glibc/musl: RTLD_DEFAULT == (void*)0
};

/// Patch a unit's external symbol `name` to a raw address before relocation.
fn addSym(unit: Oop, name: []const u8, addr: usize) void {
    _ = g_api.send(unit, g_api.intern("addSymbol:value:"), &[_]Oop{ g_api.mkString(name.ptr, name.len), g_api.mkSmallInt(@bitCast(addr)) }, 2);
}

/// Compile + install a wrapper for the C function `fn_name`. Resolves the function
/// in the process (libc + anything dlopen'd) and patches its address into the unit,
/// so the emitted `extern`/call binds to the real symbol. Returns vt, or nil if the
/// signature is invalid, the symbol is not found, a struct signature is used
/// without the collection module loaded, or compilation fails.
fn buildWrapper(fn_name: []const u8, sig_raw: []const u8, sel: Oop, vt: Oop) Oop {
    var arena_state = std.heap.ArenaAllocator.init(ca);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sigz = despace(sig_raw);
    defer ca.free(sigz);
    if (sigz.len == 0) return alienError("badSignature", "empty signature");
    const sig = parseSig(arena, sigz) orelse return alienError("badSignature", "malformed signature");

    // Struct marshal shuttles values through positional Vectors: the shim needs
    // the Vector factory's (stable, permanent) address baked in.
    var vec_factory: Oop = null;
    if (sig.aggs.len > 0) {
        vec_factory = g_api.import("Vector") orelse
            return alienError("moduleNotLoaded", "a struct signature needs the collection module");
    }

    const fz = ca.allocSentinel(u8, fn_name.len, 0) catch @panic("alien: oom");
    defer ca.free(fz);
    @memcpy(fz[0..fn_name.len], fn_name);
    const addr = g_api.dlsym(RTLD_DEFAULT, fz.ptr) orelse {
        var mb: [256]u8 = undefined;
        const msg = std.fmt.bufPrint(&mb, "symbol not found: {s}", .{fn_name}) catch "symbol not found";
        return alienError("symbolNotFound", msg);
    };

    const shim = genShim(fn_name, sig);
    defer ca.free(shim);
    if (std.posix.getenv("ALIEN_DUMP") != null) std.debug.print("[alien] shim for {s}:\n{s}\n", .{ fn_name, shim });

    const unit = s0(tcc, "new");
    if (unit == g_api.nilObj) return alienError("compileFailed", "could not create a compile unit");
    if (s1(unit, "compile:", g_api.mkString(shim.ptr, shim.len)) == g_api.nilObj)
        return alienError("compileFailed", "the generated shim C did not compile");
    _ = s0(unit, "bindRuntime"); // the marshaling shims (send / mkSmallInt / cstr / …)
    addSym(unit, fn_name, @intFromPtr(addr)); // the wrapped function itself
    if (vec_factory != null) addSym(unit, "__vecFactory", @intFromPtr(vec_factory));
    if (s0(unit, "relocate") == g_api.nilObj)
        return alienError("relocateFailed", "could not relocate the shim");

    const fname = "__shim";
    // `on:` names a prototype now (String/Integer = their prototypes) or a raw
    // vtable (VectorVt); `install:as:on:` resolves either to its family.
    return g_api.send(unit, g_api.intern("install:as:on:"), &[_]Oop{ g_api.mkString(fname, fname.len), sel, vt }, 3);
}

// -- image replay records (docs/40) -------------------------------------------
//
// A wrap installs a NATIVE trampoline and a dlopen loads a library into THIS
// process — neither can ride the image. So each successful `wrap:…`/`dlopen:`
// send is recorded as replayable birnam source on the `__aliens__` entity (the
// `__methods__` spine applied to the Alien surface), and `[Codegen replayAliens]`
// re-evaluates them at boot, in assert order — every dlopen precedes the wraps
// that need it. Recording at RUNTIME (not a codegen intercept) catches every call
// path: module load, a method body (the REPL's wrapStdio), the console. Records
// are deduped by value, so boot replay and re-run wrap sites don't churn the
// journal, and keyed `__alien_<n>` with n past the max in use.

/// The `__aliens__` entity and the store, or null when no store is loaded (the
/// bare floor: wraps still work, they just aren't replayable).
fn alienStore() ?struct { store: Oop, entity: Oop } {
    const store = g_api.import("Store") orelse return null;
    return .{ .store = store, .entity = g_api.intern("__aliens__") };
}

/// Find the `__aliens__` attr currently holding exactly `src`, or null.
fn findAlienSrc(store: Oop, entity: Oop, src: []const u8) ?Oop {
    const nAttrs = g_api.smallIntValue(g_api.send(store, g_api.intern("attrCount:"), &[_]Oop{entity}, 1));
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(store, g_api.intern("attrAt:index:"), &[_]Oop{ entity, g_api.mkSmallInt(i) }, 2);
        const v = g_api.send(store, g_api.intern("currentValue:attr:"), &[_]Oop{ entity, attr }, 2);
        if (v == null or v == g_api.nilObj) continue;
        var len: usize = 0;
        const p = g_api.stringBytes(v, &len);
        if (std.mem.eql(u8, p[0..len], src)) return attr;
    }
    return null;
}

/// Record `src` as a new `__aliens__` fact unless an identical record is already
/// current (idempotence: boot replay re-executes these very sends). The key index
/// is one past the max in use — attrCount alone could collide with a live key
/// after a `dlclose:` retraction freed a lower index.
fn recordAlienSrc(src: []const u8) void {
    const as = alienStore() orelse return;
    if (findAlienSrc(as.store, as.entity, src) != null) return;
    const nAttrs = g_api.smallIntValue(g_api.send(as.store, g_api.intern("attrCount:"), &[_]Oop{as.entity}, 1));
    var next: i64 = 0;
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(as.store, g_api.intern("attrAt:index:"), &[_]Oop{ as.entity, g_api.mkSmallInt(i) }, 2);
        const nm = std.mem.span(g_api.symbolName(attr));
        if (std.mem.startsWith(u8, nm, "__alien_")) {
            const idx = std.fmt.parseInt(i64, nm["__alien_".len..], 10) catch continue;
            if (idx >= next) next = idx + 1;
        }
    }
    var kb: [40]u8 = undefined;
    const key = std.fmt.bufPrintZ(&kb, "__alien_{d}", .{next}) catch return;
    _ = g_api.send(as.store, g_api.intern("assert:attr:value:"), &[_]Oop{ as.entity, g_api.intern(key.ptr), g_api.mkString(src.ptr, src.len) }, 3);
}

/// Retract the record holding exactly `src` (a dlclose balancing its dlopen).
fn retractAlienSrc(src: []const u8) void {
    const as = alienStore() orelse return;
    const attr = findAlienSrc(as.store, as.entity, src) orelse return;
    const v = g_api.send(as.store, g_api.intern("currentValue:attr:"), &[_]Oop{ as.entity, attr }, 2);
    _ = g_api.send(as.store, g_api.intern("retract:attr:value:"), &[_]Oop{ as.entity, attr, v }, 3);
}

/// The replayable NAME `o` resolves to at eval time: its exported name, else the
/// Globals binding holding it (a module singleton — its object survives the image
/// under leg 1's serial-vtable walk, and the binding re-resolves at boot). Null →
/// anonymous; a wrap on it cannot replay and is not recorded.
fn globalNameOf(o: Oop) ?[]const u8 {
    if (g_api.exportedNameOf(o)) |nm| return std.mem.span(nm);
    const store = g_api.import("Store") orelse return null;
    const globals = g_api.intern("__globals__");
    const nAttrs = g_api.smallIntValue(g_api.send(store, g_api.intern("attrCount:"), &[_]Oop{globals}, 1));
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(store, g_api.intern("attrAt:index:"), &[_]Oop{ globals, g_api.mkSmallInt(i) }, 2);
        const v = g_api.send(store, g_api.intern("currentValue:attr:"), &[_]Oop{ globals, attr }, 2);
        if (v == o) return std.mem.span(g_api.symbolName(attr));
    }
    return null;
}

/// birnam string literals have no escapes: a value containing `"` can't be
/// synthesized back into source, so such a record is skipped (never occurs for
/// real C function names/signatures; a quoted path just won't replay).
fn hasQuote(s: []const u8) bool {
    return std.mem.indexOfScalar(u8, s, '"') != null;
}

/// Synthesize and record the replay source for a successful wrap.
fn recordWrap(a: [*]const Oop) void {
    const target = globalNameOf(a[3]) orelse return;
    const fnb = strBytes(a[0]);
    const sigb = strBytes(a[1]);
    if (hasQuote(fnb) or hasQuote(sigb)) return;
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    var w = buf.writer(ca);
    w.print("[Alien wrap: \"{s}\" signature: \"{s}\" as: #{s} on: {s}]", .{ fnb, sigb, std.mem.span(g_api.symbolName(a[2])), target }) catch return;
    recordAlienSrc(buf.items);
}

/// An Error-family object (alienError's kinded errors relink by `ErrorVt`).
fn isErrorObj(o: Oop) bool {
    const evt = g_api.import("ErrorVt") orelse return false;
    return g_api.vtableOf(o) == evt;
}

/// `[Alien wrap: cFnName signature: sig as: #sel on: vt]`
fn mWrap(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const r = buildWrapper(strBytes(a[0]), strBytes(a[1]), a[2], a[3]);
    if (r != g_api.nilObj and !isErrorObj(r)) recordWrap(a);
    return r;
}

/// Every library `dlopen:`'d and not yet `dlclose:`'d, so the handle is TRACKED
/// rather than silently dropped (§G10) — closeable, and inspectable in principle.
/// `dlopen` refcounts, so N opens of a path make N entries needing N closes; the
/// owned `path` bytes key the lookup. These are raw handles + bytes, not Oops, so no
/// GC root provider is needed.
const DlEntry = struct { path: []u8, handle: *anyopaque };
var g_dls: std.ArrayList(DlEntry) = .{};
var g_library_dir: ?[]u8 = null;

/// `[Alien libraryDir: path]` sets the process-local directory used for bare
/// `dlopen:` names. The source path remains the recorded key, so image replay
/// keeps the wrapper's logical library name while the current project chooses
/// where that name is materialized.
fn mLibraryDir(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const path = strBytes(a[0]);
    if (g_library_dir) |old| ca.free(old);
    g_library_dir = ca.dupe(u8, path) catch @panic("alien: oom");
    return self;
}

/// `[Alien dlopen: path]` — load a shared library into the process so its symbols
/// become resolvable by later `wrap:`. Answers true, or nil if it cannot load. The
/// handle is recorded (see `g_dls`) so `dlclose:` can release it — v1 no longer leaks
/// it unconditionally (§G10).
fn mDlopen(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const requested = strBytes(a[0]);
    const resolved = if (g_library_dir) |dir|
        if (!std.fs.path.isAbsolute(requested) and std.mem.indexOfScalar(u8, requested, '/') == null)
            std.fs.path.join(ca, &.{ dir, requested }) catch @panic("alien: oom")
        else
            ca.dupe(u8, requested) catch @panic("alien: oom")
    else
        ca.dupe(u8, requested) catch @panic("alien: oom");
    defer ca.free(resolved);
    const path = dupZBytes(resolved);
    defer ca.free(path);
    const h = g_api.dlopen(path.ptr, RTLD_LAZY) orelse return g_api.nilObj;
    const owned = ca.dupe(u8, requested) catch @panic("alien: oom");
    g_dls.append(ca, .{ .path = owned, .handle = h }) catch @panic("alien: oom");
    recordDlopen(owned);
    return g_api.trueObj;
}

/// Synthesize the replay source for a dlopen — recorded AS GIVEN (a relative path
/// replays only from the same cwd; visible and honest, per the module convention).
fn dlopenSrc(buf: *std.ArrayList(u8), path: []const u8) ?[]const u8 {
    if (hasQuote(path)) return null;
    var w = buf.writer(ca);
    w.print("[Alien dlopen: \"{s}\"]", .{path}) catch return null;
    return buf.items;
}

fn recordDlopen(path: []const u8) void {
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    const src = dlopenSrc(&buf, path) orelse return;
    recordAlienSrc(src);
}

/// `[Alien dlclose: path]` — release ONE handle opened by a matching `dlopen:` (the
/// counterpart that closes the §G10 leak): balances one open's refcount and forgets
/// the entry. Answers true, or a kinded error if no open library matches. UNSAFE like
/// `[buf free]`: the caller asserts nothing still calls a symbol wrapped from this
/// library — closing a library whose address was patched into a live wrapper unit
/// leaves that wrapper dangling.
fn mDlclose(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const want = strBytes(a[0]);
    var i: usize = 0;
    while (i < g_dls.items.len) : (i += 1) {
        if (std.mem.eql(u8, g_dls.items[i].path, want)) {
            const e = g_dls.swapRemove(i);
            _ = std.c.dlclose(e.handle);
            // Retract the replay record only when this was the LAST open of the
            // path (dlopen refcounts; a boot needs the library open once).
            var still_open = false;
            for (g_dls.items) |d| {
                if (std.mem.eql(u8, d.path, e.path)) still_open = true;
            }
            if (!still_open) {
                var buf: std.ArrayList(u8) = .{};
                defer buf.deinit(ca);
                if (dlopenSrc(&buf, e.path)) |src| retractAlienSrc(src);
            }
            ca.free(e.path);
            return g_api.trueObj;
        }
    }
    return alienError("notFound", "dlclose: no open library at that path");
}

// -- F4: raw C-memory access through a Word (alloc/free + peek/poke) ------------
//
// A Word holds a machine address, so it doubles as a C buffer handle: `[Alien
// alloc: n]` mints one over `n` zeroed libc-malloc'd bytes, `peek*At:`/`poke*:at:`
// read and write typed values at a byte offset, and `[buf free]` reclaims it. This
// is the other half of "wrap a C library without writing C": a function taking a
// `T*` out-param is wrapped by allocating a buffer, passing the Word as its `p`
// argument (§F F1 — a Word untags to `void*`), then peeking the result back.
//
// These are inherently unsafe raw-memory ops (like C): the guards catch a null
// receiver (#nullPointer) and a non-Integer offset (#typeMismatch), NOT a wild
// address. Buffers may sit at any offset, so every access is `align(1)`.

/// Resolve `wordValue(receiver) + offset(arg)` to an absolute address. On success
/// writes it to `out` and returns null; on a null base or non-Integer offset,
/// returns the kinded error to propagate (§C1 — no silent nil/garbage deref).
fn resolveAddr(self: Oop, off_arg: Oop, out: *u64) ?Oop {
    const base = g_api.wordValue(self);
    if (base == 0) return alienError("nullPointer", "cannot peek/poke through a null pointer");
    if (g_api.isSmallInt(off_arg) == 0) return alienError("typeMismatch", "the offset must be an Integer");
    out.* = base +% @as(u64, @bitCast(g_api.smallIntValue(off_arg)));
    return null;
}
/// An address-valued argument: a Word (raw address) or an Integer (its bits).
fn argAddr(o: Oop) ?u64 {
    if (g_api.isWord(o) != 0) return g_api.wordValue(o);
    if (g_api.isSmallInt(o) != 0) return @bitCast(g_api.smallIntValue(o));
    return null;
}

/// `[Alien alloc: nBytes]` — `nBytes` zeroed libc-malloc'd bytes as a Word. Raw
/// (not GC heap) so the address is stable for C; reclaim with `[theWord free]`.
fn mAlloc(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (g_api.isSmallInt(a[0]) == 0) return alienError("typeMismatch", "alloc: needs an Integer byte count");
    const nbytes = g_api.smallIntValue(a[0]);
    if (nbytes <= 0) return alienError("badArgument", "alloc: needs a positive byte count");
    const raw = std.c.malloc(@intCast(nbytes)) orelse return alienError("outOfMemory", "alloc: malloc failed");
    @memset(@as([*]u8, @ptrCast(raw))[0..@intCast(nbytes)], 0);
    return g_api.mkWord(@intFromPtr(raw));
}
/// `[aWord free]` — libc-free the address (a no-op on null). Answers nil.
fn mFree(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const base = g_api.wordValue(self);
    if (base != 0) std.c.free(@ptrFromInt(base));
    return g_api.nilObj;
}

/// `[buf peekIntAt: off]` — the C `long` (i64) at `base+off`, as an Integer.
fn mPeekIntAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[0], &addr)) |e| return e;
    const p: *align(1) const i64 = @ptrFromInt(addr);
    return g_api.mkSmallInt(p.*);
}
/// `[buf peekByteAt: off]` — the unsigned byte at `base+off`, as an Integer (0..255).
fn mPeekByteAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[0], &addr)) |e| return e;
    const p: *const u8 = @ptrFromInt(addr);
    return g_api.mkSmallInt(@as(i64, p.*));
}
/// `[buf peekPtrAt: off]` — the C `void*` at `base+off`, as a Word (deref a `T**`).
fn mPeekPtrAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[0], &addr)) |e| return e;
    const p: *align(1) const usize = @ptrFromInt(addr);
    return g_api.mkWord(p.*);
}
/// `[buf peekDoubleAt: off]` — the C `double` at `base+off`, as a Float.
fn mPeekDoubleAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[0], &addr)) |e| return e;
    const p: *align(1) const f64 = @ptrFromInt(addr);
    return g_api.mkFloat(p.*);
}
/// `[buf peekStringAt: off]` — the NUL-terminated C string at `base+off`, COPIED
/// into a fresh birnam String (so the C buffer may be freed after).
fn mPeekStringAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[0], &addr)) |e| return e;
    const cptr: [*:0]const u8 = @ptrFromInt(addr);
    const s = std.mem.span(cptr);
    return g_api.mkString(s.ptr, s.len);
}

/// `[buf pokeInt: v at: off]` — write the Integer `v` as a C `long` at `base+off`.
/// Answers the receiver, so pokes chain. (Poke value in a[0], offset in a[1].)
fn mPokeIntAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[1], &addr)) |e| return e;
    if (g_api.isSmallInt(a[0]) == 0) return alienError("typeMismatch", "pokeInt: needs an Integer value");
    const p: *align(1) i64 = @ptrFromInt(addr);
    p.* = g_api.smallIntValue(a[0]);
    return self;
}
/// `[buf pokeByte: v at: off]` — write the low byte of Integer `v` at `base+off`.
fn mPokeByteAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[1], &addr)) |e| return e;
    if (g_api.isSmallInt(a[0]) == 0) return alienError("typeMismatch", "pokeByte: needs an Integer value");
    const p: *u8 = @ptrFromInt(addr);
    p.* = @truncate(@as(u64, @bitCast(g_api.smallIntValue(a[0]))));
    return self;
}
/// `[buf pokePtr: w at: off]` — write the address `w` (a Word or Integer) as a C
/// `void*` at `base+off`.
fn mPokePtrAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[1], &addr)) |e| return e;
    const w = argAddr(a[0]) orelse return alienError("typeMismatch", "pokePtr: needs a Word or Integer address");
    const p: *align(1) usize = @ptrFromInt(addr);
    p.* = w;
    return self;
}
/// `[buf pokeDouble: v at: off]` — write the Float `v` as a C `double` at `base+off`.
fn mPokeDoubleAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var addr: u64 = undefined;
    if (resolveAddr(self, a[1], &addr)) |e| return e;
    if (g_api.isFloat(a[0]) == 0) return alienError("typeMismatch", "pokeDouble: needs a Float value");
    const p: *align(1) f64 = @ptrFromInt(addr);
    p.* = g_api.floatValue(a[0]);
    return self;
}

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    tcc = api.import("Tcc") orelse @panic("alien: Tcc not loaded");

    const reloading = api.isExported("Alien") != 0;
    if (reloading) {
        alien_vt = api.import("AlienVt");
        alien_obj = api.import("Alien");
    } else {
        alien_vt = api.proto(api.object_vt);
        alien_obj = api.alloc(alien_vt, 0);
    }

    api.method(alien_vt, api.intern("wrap:signature:as:on:"), &mWrap);
    api.method(alien_vt, api.intern("callback:signature:"), &mCallback);
    api.method(alien_vt, api.intern("releaseCallback:"), &mReleaseCallback);
    api.method(alien_vt, api.intern("libraryDir:"), &mLibraryDir);
    api.method(alien_vt, api.intern("dlopen:"), &mDlopen);
    api.method(alien_vt, api.intern("dlclose:"), &mDlclose);
    api.method(alien_vt, api.intern("alloc:"), &mAlloc);

    // F4: the raw C-memory protocol on Word (a pointer IS a Word since §F F1), so a
    // buffer allocated here, a `p`-returned pointer, and a poked-in address all share it.
    const wvt = api.word_vt;
    api.method(wvt, api.intern("free"), &mFree);
    api.method(wvt, api.intern("peekIntAt:"), &mPeekIntAt);
    api.method(wvt, api.intern("peekByteAt:"), &mPeekByteAt);
    api.method(wvt, api.intern("peekPtrAt:"), &mPeekPtrAt);
    api.method(wvt, api.intern("peekDoubleAt:"), &mPeekDoubleAt);
    api.method(wvt, api.intern("peekStringAt:"), &mPeekStringAt);
    api.method(wvt, api.intern("pokeInt:at:"), &mPokeIntAt);
    api.method(wvt, api.intern("pokeByte:at:"), &mPokeByteAt);
    api.method(wvt, api.intern("pokePtr:at:"), &mPokePtrAt);
    api.method(wvt, api.intern("pokeDouble:at:"), &mPokeDoubleAt);

    if (!reloading) {
        api.gcRegisterRootProvider(&cbRoots); // callbacks pin their block + unit
        _ = api.exportObj("AlienVt", alien_vt);
        _ = api.exportObj("Alien", alien_obj);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
