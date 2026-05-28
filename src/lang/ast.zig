const std = @import("std");

pub const Oop = ?*anyopaque;

/// AST node — arena-allocated, not id objects.
pub const Node = struct {
    tag: Tag,
    data: Data,

    pub const Tag = enum {
        integer_lit,
        string_lit,
        symbol_lit,
        char_lit,
        array_lit,
        variable,
        assign,
        send,
        cascade,
        block,
        return_expr,
        sequence,
        type_def,
        method_def,
        global_assign,
        exec_block,
        import_decl,
    };

    pub const Data = union {
        integer_lit: IntegerLit,
        string_lit: StringLit,
        symbol_lit: StringLit,
        char_lit: u8,
        array_lit: ArrayLit,
        variable: Variable,
        assign: Assign,
        send: Send,
        cascade: Cascade,
        block: Block,
        return_expr: ReturnExpr,
        sequence: Sequence,
        type_def: TypeDef,
        method_def: MethodDef,
        global_assign: GlobalAssign,
        exec_block: ExecBlock,
        import_decl: ImportDecl,
    };

    pub const IntegerLit = struct {
        value: i64,
    };

    pub const StringLit = struct {
        value: []const u8,
    };

    pub const ArrayLit = struct {
        elements: []*Node,
    };

    pub const Variable = struct {
        name: []const u8,
    };

    pub const Assign = struct {
        name: []const u8,
        value: *Node,
    };

    pub const Send = struct {
        receiver: *Node,
        selector: []const u8,
        args: []const *Node,
    };

    pub const Cascade = struct {
        receiver: *Node,
        messages: []const CascadeMsg,
    };

    pub const CascadeMsg = struct {
        selector: []const u8,
        args: []const *Node,
    };

    pub const Block = struct {
        params: []const []const u8,
        temps: []const []const u8,
        body: []*Node,
    };

    pub const ReturnExpr = struct {
        value: *Node,
    };

    pub const Sequence = struct {
        statements: []*Node,
    };

    pub const TypeDef = struct {
        name: []const u8,
        parent: []const u8,
        slots: []const []const u8,
    };

    pub const MethodDef = struct {
        type_name: []const u8,
        selector: []const u8,
        params: []const []const u8,
        body: []*Node,
    };

    pub const GlobalAssign = struct {
        name: []const u8,
        value: *Node,
    };

    pub const ExecBlock = struct {
        body: []*Node,
    };

    pub const ImportDecl = struct {
        module_name: []const u8,
    };
};

/// Create nodes using an arena allocator.
pub fn makeNode(arena: std.mem.Allocator, tag: Node.Tag, data: Node.Data) !*Node {
    const node = try arena.create(Node);
    node.* = .{ .tag = tag, .data = data };
    return node;
}

pub fn makeIntegerLit(arena: std.mem.Allocator, value: i64) !*Node {
    return makeNode(arena, .integer_lit, .{ .integer_lit = .{ .value = value } });
}

pub fn makeSend(arena: std.mem.Allocator, receiver: *Node, selector: []const u8, args: []const *Node) !*Node {
    const args_copy = try arena.dupe(*Node, args);
    return makeNode(arena, .send, .{ .send = .{
        .receiver = receiver,
        .selector = selector,
        .args = args_copy,
    } });
}

pub fn makeVariable(arena: std.mem.Allocator, name: []const u8) !*Node {
    return makeNode(arena, .variable, .{ .variable = .{ .name = name } });
}

pub fn makeAssign(arena: std.mem.Allocator, name: []const u8, value: *Node) !*Node {
    return makeNode(arena, .assign, .{ .assign = .{ .name = name, .value = value } });
}

pub fn makeSequence(arena: std.mem.Allocator, stmts: []*Node) !*Node {
    return makeNode(arena, .sequence, .{ .sequence = .{ .statements = stmts } });
}

pub fn makeStringLit(arena: std.mem.Allocator, value: []const u8) !*Node {
    return makeNode(arena, .string_lit, .{ .string_lit = .{ .value = value } });
}

pub fn makeBlock(arena: std.mem.Allocator, params: []const []const u8, temps: []const []const u8, body: []*Node) !*Node {
    return makeNode(arena, .block, .{ .block = .{
        .params = params,
        .temps = temps,
        .body = body,
    } });
}

pub fn makeReturnExpr(arena: std.mem.Allocator, value: *Node) !*Node {
    return makeNode(arena, .return_expr, .{ .return_expr = .{ .value = value } });
}

pub fn makeTypeDef(arena: std.mem.Allocator, name: []const u8, parent: []const u8, slots: []const []const u8) !*Node {
    return makeNode(arena, .type_def, .{ .type_def = .{
        .name = name,
        .parent = parent,
        .slots = slots,
    } });
}

pub fn makeMethodDef(arena: std.mem.Allocator, type_name: []const u8, selector: []const u8, params: []const []const u8, body: []*Node) !*Node {
    return makeNode(arena, .method_def, .{ .method_def = .{
        .type_name = type_name,
        .selector = selector,
        .params = params,
        .body = body,
    } });
}
