const std = @import("std");
const IdApi = @import("api.zig").IdApi;

/// Opaque pointer type matching id's Oop.
pub const Oop = ?*anyopaque;

// -- Node vtables (set during init) -----------------------------------------

pub var grammar_vt: Oop = null;
pub var rule_vt: Oop = null;
pub var sequence_vt: Oop = null;
pub var alternate_vt: Oop = null;
pub var star_vt: Oop = null;
pub var plus_vt: Oop = null;
pub var query_vt: Oop = null;
pub var peek_for_vt: Oop = null;
pub var peek_not_vt: Oop = null;
pub var name_vt: Oop = null;
pub var string_lit_vt: Oop = null;
pub var char_class_vt: Oop = null;
pub var dot_vt: Oop = null;
pub var action_vt: Oop = null;
pub var capture_vt: Oop = null;

pub var s_match: Oop = null;

// -- Node struct layouts (stored in id object memory) -----------------------

pub const Grammar = extern struct {
    start_rule: Oop,
    rules: Oop, // vector of Rule Oops
    rule_count: usize,
};

pub const Rule = extern struct {
    name: Oop, // selector Oop
    expr: Oop,
};

pub const Sequence = extern struct {
    elements: Oop, // vector Oop
    count: usize,
};

pub const Alternate = extern struct {
    elements: Oop, // vector Oop
    count: usize,
};

pub const Star = extern struct {
    element: Oop,
};

pub const Plus = extern struct {
    element: Oop,
};

pub const Query = extern struct {
    element: Oop,
};

pub const PeekFor = extern struct {
    element: Oop,
};

pub const PeekNot = extern struct {
    element: Oop,
};

pub const Name = extern struct {
    rule_name: Oop, // selector Oop
};

pub const StringLit = extern struct {
    ptr: ?[*]const u8,
    len: usize,
};

pub const CharClass = extern struct {
    table: ?*[256]u8,
};

pub const Dot = extern struct {
    _pad: usize, // empty struct needs at least 1 field for extern
};

pub const Action = extern struct {
    ptr: ?[*]const u8,
    len: usize,
};

pub const Capture = extern struct {
    element: Oop,
};

// -- Casting helpers --------------------------------------------------------

pub fn asGrammar(oop: Oop) ?*Grammar {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asRule(oop: Oop) ?*Rule {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asSequence(oop: Oop) ?*Sequence {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asAlternate(oop: Oop) ?*Alternate {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asStar(oop: Oop) ?*Star {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asPlus(oop: Oop) ?*Plus {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asQuery(oop: Oop) ?*Query {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asPeekFor(oop: Oop) ?*PeekFor {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asPeekNot(oop: Oop) ?*PeekNot {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asName(oop: Oop) ?*Name {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asStringLit(oop: Oop) ?*StringLit {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asCharClass(oop: Oop) ?*CharClass {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asDot(oop: Oop) ?*Dot {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asAction(oop: Oop) ?*Action {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}
pub fn asCapture(oop: Oop) ?*Capture {
    return if (oop) |o| @ptrCast(@alignCast(o)) else null;
}

// -- Vtable identity check (for engine dispatch) ----------------------------

pub fn vtableOf(oop: Oop) Oop {
    const o = oop orelse return null;
    // Read vtable from offset -1 (one pointer before object)
    const raw: [*]align(@alignOf(*anyopaque)) u8 = @ptrCast(@alignCast(o));
    const vt_slot: *Oop = @ptrCast(@alignCast(raw - @sizeOf(Oop)));
    return vt_slot.*;
}

// -- Initialization ---------------------------------------------------------

var id: *const IdApi = undefined;

pub fn init(api: *const IdApi) void {
    id = api;

    // Create all node vtables delegating from object_vt
    grammar_vt = api.vtable_delegated(api.object_vt);
    rule_vt = api.vtable_delegated(api.object_vt);
    sequence_vt = api.vtable_delegated(api.object_vt);
    alternate_vt = api.vtable_delegated(api.object_vt);
    star_vt = api.vtable_delegated(api.object_vt);
    plus_vt = api.vtable_delegated(api.object_vt);
    query_vt = api.vtable_delegated(api.object_vt);
    peek_for_vt = api.vtable_delegated(api.object_vt);
    peek_not_vt = api.vtable_delegated(api.object_vt);
    name_vt = api.vtable_delegated(api.object_vt);
    string_lit_vt = api.vtable_delegated(api.object_vt);
    char_class_vt = api.vtable_delegated(api.object_vt);
    dot_vt = api.vtable_delegated(api.object_vt);
    action_vt = api.vtable_delegated(api.object_vt);
    capture_vt = api.vtable_delegated(api.object_vt);

    s_match = api.intern("match:");
}

// -- Node constructors ------------------------------------------------------

pub fn makeGrammar() Oop {
    return id.alloc(grammar_vt, @sizeOf(Grammar));
}

pub fn makeRule(name_sel: Oop, expr: Oop) Oop {
    const o = id.alloc(rule_vt, @sizeOf(Rule)) orelse return null;
    const r = asRule(o).?;
    r.name = name_sel;
    r.expr = expr;
    return o;
}

pub fn makeSequence() Oop {
    const o = id.alloc(sequence_vt, @sizeOf(Sequence)) orelse return null;
    const s = asSequence(o).?;
    s.elements = null;
    s.count = 0;
    return o;
}

pub fn makeAlternate() Oop {
    const o = id.alloc(alternate_vt, @sizeOf(Alternate)) orelse return null;
    const a = asAlternate(o).?;
    a.elements = null;
    a.count = 0;
    return o;
}

pub fn makeStar(element: Oop) Oop {
    const o = id.alloc(star_vt, @sizeOf(Star)) orelse return null;
    asStar(o).?.element = element;
    return o;
}

pub fn makePlus(element: Oop) Oop {
    const o = id.alloc(plus_vt, @sizeOf(Plus)) orelse return null;
    asPlus(o).?.element = element;
    return o;
}

pub fn makeQuery(element: Oop) Oop {
    const o = id.alloc(query_vt, @sizeOf(Query)) orelse return null;
    asQuery(o).?.element = element;
    return o;
}

pub fn makePeekFor(element: Oop) Oop {
    const o = id.alloc(peek_for_vt, @sizeOf(PeekFor)) orelse return null;
    asPeekFor(o).?.element = element;
    return o;
}

pub fn makePeekNot(element: Oop) Oop {
    const o = id.alloc(peek_not_vt, @sizeOf(PeekNot)) orelse return null;
    asPeekNot(o).?.element = element;
    return o;
}

pub fn makeName(rule_name: Oop) Oop {
    const o = id.alloc(name_vt, @sizeOf(Name)) orelse return null;
    asName(o).?.rule_name = rule_name;
    return o;
}

pub fn makeStringLit(str: []const u8) Oop {
    const o = id.alloc(string_lit_vt, @sizeOf(StringLit)) orelse return null;
    const s = asStringLit(o).?;
    s.ptr = str.ptr;
    s.len = str.len;
    return o;
}

pub fn makeCharClass(table: *[256]u8) Oop {
    const o = id.alloc(char_class_vt, @sizeOf(CharClass)) orelse return null;
    asCharClass(o).?.table = table;
    return o;
}

pub fn makeDot() Oop {
    return id.alloc(dot_vt, @sizeOf(Dot));
}

pub fn makeAction(text: []const u8) Oop {
    const o = id.alloc(action_vt, @sizeOf(Action)) orelse return null;
    const a = asAction(o).?;
    a.ptr = text.ptr;
    a.len = text.len;
    return o;
}

pub fn makeCapture(element: Oop) Oop {
    const o = id.alloc(capture_vt, @sizeOf(Capture)) orelse return null;
    asCapture(o).?.element = element;
    return o;
}
