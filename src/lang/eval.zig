const std = @import("std");
const ast = @import("ast.zig");
const env_mod = @import("env.zig");
const builtins = @import("builtins.zig");
const IdApi = @import("api.zig").IdApi;

const Oop = ?*anyopaque;
const Env = env_mod.Env;

pub const EvalError = error{
    UndefinedVariable,
    TypeError,
    DoesNotUnderstand,
    OutOfMemory,
    NonLocalReturn,
};

pub const Evaluator = struct {
    api: *const IdApi,
    global: *Env,
    arena: std.mem.Allocator,
    /// Buffer for formatting result output
    result_buf: [256]u8 = undefined,
    /// Non-local return value (set by return_expr, caught by method frame)
    return_value: Oop = null,

    pub fn init(id_api: *const IdApi, global: *Env, arena: std.mem.Allocator) Evaluator {
        builtins.initTypeRegistry(arena);
        // Register "Object" as the root type
        builtins.registerType("Object", .{
            .name = "Object",
            .vt = id_api.object_vt,
            .parent_vt = null,
            .slots = &.{},
        });
        return .{
            .api = id_api,
            .global = global,
            .arena = arena,
        };
    }

    pub fn eval(self: *Evaluator, node: *ast.Node, scope: *Env) EvalError!Oop {
        return switch (node.tag) {
            .integer_lit => builtins.intToOop(node.data.integer_lit.value),
            .string_lit => builtins.makeString(node.data.string_lit.value),
            .symbol_lit => builtins.makeString(node.data.symbol_lit.value),
            .variable => self.evalVariable(node.data.variable.name, scope),
            .assign => self.evalAssign(node.data.assign, scope),
            .send => self.evalSend(node.data.send, scope),
            .sequence => self.evalSequence(node.data.sequence, scope),
            .block => self.evalBlock(node.data.block, scope),
            .return_expr => self.evalReturn(node.data.return_expr, scope),
            .type_def => self.evalTypeDef(node.data.type_def, scope),
            .method_def => self.evalMethodDef(node.data.method_def, scope),
            .char_lit => builtins.intToOop(@as(i64, @intCast(node.data.char_lit))),
            .array_lit => self.evalArrayLit(node.data.array_lit, scope),
            .cascade => self.evalCascade(node.data.cascade, scope),
            .global_assign => self.evalGlobalAssign(node.data.global_assign, scope),
            .exec_block => self.evalExecBlock(node.data.exec_block, scope),
            .import_decl,
            => error.TypeError,
        };
    }

    fn evalVariable(self: *Evaluator, name: []const u8, scope: *Env) EvalError!Oop {
        // Check scope chain first
        if (scope.lookup(name)) |v| return v;

        // Check if it's a slot access (self is bound and name is a slot of self's type)
        if (scope.self_ref) |self_obj| {
            if (!builtins.isSmallInt(self_obj)) {
                const vt = builtins.getVt(self_obj);
                if (builtins.typeNameForVt(vt)) |type_name| {
                    if (builtins.slotIndex(type_name, name)) |idx| {
                        return builtins.getSlot(self_obj, idx);
                    }
                }
            }
        }

        _ = self;
        return error.UndefinedVariable;
    }

    fn evalAssign(self: *Evaluator, data: ast.Node.Assign, scope: *Env) EvalError!Oop {
        const value = try self.eval(data.value, scope);

        // Check if assigning to a slot of self's type
        if (scope.self_ref) |self_obj| {
            if (!builtins.isSmallInt(self_obj)) {
                const vt = builtins.getVt(self_obj);
                if (builtins.typeNameForVt(vt)) |type_name| {
                    if (builtins.slotIndex(type_name, data.name)) |idx| {
                        builtins.setSlot(self_obj, idx, value);
                        return value;
                    }
                }
            }
        }

        if (!scope.set(data.name, value)) {
            scope.define(data.name, value);
        }
        return value;
    }

    fn evalSend(self: *Evaluator, data: ast.Node.Send, scope: *Env) EvalError!Oop {
        const receiver = try self.eval(data.receiver, scope);

        // Evaluate arguments
        var args: [3]Oop = .{ null, null, null };
        const n_args = @min(data.args.len, 3);
        for (0..n_args) |i| {
            args[i] = try self.eval(data.args[i], scope);
        }

        return self.dispatchSend(receiver, data.selector, args[0..n_args]);
    }

    /// Common send dispatch — used by evalSend, performSend (cascade), etc.
    fn dispatchSend(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) EvalError!Oop {
        const n_args = args.len;

        // -- Universal methods (apply to ALL objects including nil) --

        if (n_args == 0) {
            if (std.mem.eql(u8, selector, "yourself")) return receiver;
            if (std.mem.eql(u8, selector, "isNil")) {
                return if (receiver == null) builtins.getTrue() else builtins.getFalse();
            }
            if (std.mem.eql(u8, selector, "class")) {
                if (receiver == null) return builtins.getNilVt();
                if (builtins.isSmallInt(receiver)) return builtins.getSmallIntVt();
                return builtins.getVt(receiver);
            }
            if (std.mem.eql(u8, selector, "printString")) {
                return self.universalPrintString(receiver);
            }
        }

        if (n_args == 1) {
            if (std.mem.eql(u8, selector, "==")) {
                return if (receiver == args[0]) builtins.getTrue() else builtins.getFalse();
            }
            if (std.mem.eql(u8, selector, "~~")) {
                return if (receiver != args[0]) builtins.getTrue() else builtins.getFalse();
            }
        }

        // -- Nil handler (after universal methods) --
        if (receiver == null) {
            return self.handleDoesNotUnderstand(receiver, selector, args);
        }

        // SmallInt fast path
        if (builtins.isSmallInt(receiver)) {
            return self.sendToSmallInt(receiver, selector, args) orelse
                error.DoesNotUnderstand;
        }

        // Block fast path — handle value, value:, value:value:, whileTrue:, whileFalse:
        if (builtins.isBlock(receiver)) {
            return try self.sendToBlock(receiver, selector, args);
        }

        // Boolean fast path — handle ifTrue:, ifFalse:, ifTrue:ifFalse:, not, and:, or:
        if (builtins.isTrueObj(receiver) or builtins.isFalseObj(receiver)) {
            return try self.sendToBoolean(receiver, selector, args);
        }

        // Array fast path
        if (builtins.isArray(receiver)) {
            return try self.sendToArray(receiver, selector, args);
        }

        // "new" on a type name — check if receiver is a vtable we know about
        if (receiver != null and std.mem.eql(u8, selector, "new") and n_args == 0) {
            if (builtins.typeNameForVt(receiver)) |_| {
                const info = self.lookupTypeByVt(receiver) orelse return error.DoesNotUnderstand;
                return builtins.allocInstance(info.vt, info.slots.len);
            }
        }

        // General id send dispatch
        const sel_z = self.arena.allocSentinel(u8, selector.len, 0) catch return error.OutOfMemory;
        @memcpy(sel_z, selector);
        const sel = self.api.intern(sel_z.ptr);
        if (sel == null) return self.handleDoesNotUnderstand(receiver, selector, args);

        const result: Oop = switch (n_args) {
            0 => self.api.send0(receiver, sel),
            1 => self.api.send1(receiver, sel, args[0]),
            2 => self.api.send2(receiver, sel, args[0], args[1]),
            3 => self.api.send3(receiver, sel, args[0], args[1], args[2]),
            else => null,
        };
        if (result != null) return result;

        return self.handleDoesNotUnderstand(receiver, selector, args);
    }

    fn sendToSmallInt(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) ?Oop {
        const a = builtins.oopToInt(receiver);

        if (args.len == 1 and builtins.isSmallInt(args[0])) {
            const b = builtins.oopToInt(args[0]);

            if (std.mem.eql(u8, selector, "+")) return builtins.intToOop(a + b);
            if (std.mem.eql(u8, selector, "-")) return builtins.intToOop(a - b);
            if (std.mem.eql(u8, selector, "*")) return builtins.intToOop(a * b);
            if (std.mem.eql(u8, selector, "//")) {
                if (b == 0) return builtins.intToOop(0);
                return builtins.intToOop(@divTrunc(a, b));
            }
            if (std.mem.eql(u8, selector, "\\\\")) {
                if (b == 0) return builtins.intToOop(0);
                return builtins.intToOop(@rem(a, b));
            }
            if (std.mem.eql(u8, selector, "<")) return builtins.intToOop(if (a < b) @as(i64, 1) else 0);
            if (std.mem.eql(u8, selector, ">")) return builtins.intToOop(if (a > b) @as(i64, 1) else 0);
            if (std.mem.eql(u8, selector, "=")) return builtins.intToOop(if (a == b) @as(i64, 1) else 0);
            if (std.mem.eql(u8, selector, "~=")) return builtins.intToOop(if (a != b) @as(i64, 1) else 0);
        }

        // timesRepeat: aBlock
        if (args.len == 1 and std.mem.eql(u8, selector, "timesRepeat:")) {
            const bd = builtins.asBlockData(args[0]) orelse return null;
            var i: i64 = 1;
            while (i <= a) : (i += 1) {
                _ = self.evalBlockBody(bd, &.{builtins.intToOop(i)}) catch return null;
            }
            return receiver;
        }

        if (args.len == 0) {
            if (std.mem.eql(u8, selector, "println")) {
                const w = std.fs.File.stdout().deprecatedWriter();
                w.print("{d}\n", .{a}) catch {};
                return receiver;
            }
            if (std.mem.eql(u8, selector, "printString")) {
                var buf: [32]u8 = undefined;
                const formatted = std.fmt.bufPrint(&buf, "{d}", .{a}) catch return null;
                return builtins.makeString(formatted);
            }
        }

        return null;
    }

    fn sendToBlock(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) EvalError!Oop {
        const bd = builtins.asBlockData(receiver) orelse return error.TypeError;

        // value, value:, value:value:, value:value:value:
        if (std.mem.eql(u8, selector, "value") and args.len == 0) {
            return self.evalBlockBody(bd, &.{});
        }
        if (std.mem.eql(u8, selector, "value:") and args.len == 1) {
            return self.evalBlockBody(bd, args[0..1]);
        }
        if (std.mem.eql(u8, selector, "value:value:") and args.len == 2) {
            return self.evalBlockBody(bd, args[0..2]);
        }
        if (std.mem.eql(u8, selector, "value:value:value:") and args.len == 3) {
            return self.evalBlockBody(bd, args[0..3]);
        }

        // whileTrue: aBlock
        if (std.mem.eql(u8, selector, "whileTrue:") and args.len == 1) {
            const body_bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            while (true) {
                const cond = try self.evalBlockBody(bd, &.{});
                if (!isTruthy(cond)) break;
                _ = try self.evalBlockBody(body_bd, &.{});
            }
            return builtins.getNil();
        }

        // whileFalse: aBlock
        if (std.mem.eql(u8, selector, "whileFalse:") and args.len == 1) {
            const body_bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            while (true) {
                const cond = try self.evalBlockBody(bd, &.{});
                if (isTruthy(cond)) break;
                _ = try self.evalBlockBody(body_bd, &.{});
            }
            return builtins.getNil();
        }

        return error.DoesNotUnderstand;
    }

    fn sendToBoolean(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) EvalError!Oop {
        const is_true = builtins.isTrueObj(receiver);

        if (std.mem.eql(u8, selector, "ifTrue:") and args.len == 1) {
            if (is_true) {
                const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
                return self.evalBlockBody(bd, &.{});
            }
            return builtins.getNil();
        }

        if (std.mem.eql(u8, selector, "ifFalse:") and args.len == 1) {
            if (!is_true) {
                const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
                return self.evalBlockBody(bd, &.{});
            }
            return builtins.getNil();
        }

        if (std.mem.eql(u8, selector, "ifTrue:ifFalse:") and args.len == 2) {
            const which = if (is_true) args[0] else args[1];
            const bd = builtins.asBlockData(which) orelse return error.TypeError;
            return self.evalBlockBody(bd, &.{});
        }

        if (std.mem.eql(u8, selector, "ifFalse:ifTrue:") and args.len == 2) {
            const which = if (!is_true) args[0] else args[1];
            const bd = builtins.asBlockData(which) orelse return error.TypeError;
            return self.evalBlockBody(bd, &.{});
        }

        if (std.mem.eql(u8, selector, "not") and args.len == 0) {
            return if (is_true) builtins.getFalse() else builtins.getTrue();
        }

        if (std.mem.eql(u8, selector, "and:") and args.len == 1) {
            if (!is_true) return builtins.getFalse();
            const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            return self.evalBlockBody(bd, &.{});
        }

        if (std.mem.eql(u8, selector, "or:") and args.len == 1) {
            if (is_true) return builtins.getTrue();
            const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            return self.evalBlockBody(bd, &.{});
        }

        return error.DoesNotUnderstand;
    }

    fn evalBlock(self: *Evaluator, data: ast.Node.Block, scope: *Env) EvalError!Oop {
        const bd = self.arena.create(builtins.BlockData) catch return error.OutOfMemory;
        bd.* = .{
            .params = data.params,
            .body = data.body,
            .captured_env = scope,
        };
        return builtins.makeBlock(bd);
    }

    /// Evaluate a block's body with given arguments.
    pub fn evalBlockBody(self: *Evaluator, bd: *builtins.BlockData, args: []const Oop) EvalError!Oop {
        var scope = Env.init(self.arena, bd.captured_env);

        const n_params = @min(bd.params.len, args.len);
        for (0..n_params) |i| {
            scope.define(bd.params[i], args[i]);
        }

        var result: Oop = builtins.getNil();
        for (bd.body) |stmt| {
            result = try self.eval(stmt, &scope);
        }
        return result;
    }

    fn evalReturn(self: *Evaluator, data: ast.Node.ReturnExpr, scope: *Env) EvalError!Oop {
        const value = try self.eval(data.value, scope);
        self.return_value = value;
        return error.NonLocalReturn;
    }

    fn evalSequence(self: *Evaluator, data: ast.Node.Sequence, scope: *Env) EvalError!Oop {
        var result: Oop = null;
        for (data.statements) |stmt| {
            result = try self.eval(stmt, scope);
        }
        return result;
    }

    // -- Phase 6: Polish -------------------------------------------------------

    /// Universal printString — works on all types.
    fn universalPrintString(self: *Evaluator, receiver: Oop) EvalError!Oop {
        if (receiver == null) return builtins.makeString("nil");
        if (builtins.isSmallInt(receiver)) {
            const val = builtins.oopToInt(receiver);
            const formatted = std.fmt.bufPrint(&self.result_buf, "{d}", .{val}) catch return error.OutOfMemory;
            return builtins.makeString(formatted);
        }
        if (builtins.isString(receiver)) return receiver;
        if (builtins.isBlock(receiver)) return builtins.makeString("a Block");
        if (receiver == builtins.getTrue()) return builtins.makeString("true");
        if (receiver == builtins.getFalse()) return builtins.makeString("false");
        if (builtins.isArray(receiver)) {
            // Reuse formatResult for arrays
            const formatted = self.formatResult(receiver);
            return builtins.makeString(formatted);
        }
        // Type vtable
        if (builtins.typeNameForVt(receiver)) |name| {
            return builtins.makeString(name);
        }
        // Type instance
        const vt = builtins.getVt(receiver);
        if (builtins.typeNameForVt(vt)) |name| {
            const formatted = std.fmt.bufPrint(&self.result_buf, "a {s}", .{name}) catch return error.OutOfMemory;
            return builtins.makeString(formatted);
        }
        return builtins.makeString("an Object");
    }

    /// doesNotUnderstand: hook — try sending doesNotUnderstand: to the receiver.
    fn handleDoesNotUnderstand(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) EvalError!Oop {
        _ = args;

        // Don't recurse on doesNotUnderstand: itself
        if (std.mem.eql(u8, selector, "doesNotUnderstand:")) return error.DoesNotUnderstand;

        // Only try on non-null, non-SmallInt objects that might have the method
        if (receiver != null and !builtins.isSmallInt(receiver)) {
            const dnu_sel_z = self.arena.allocSentinel(u8, "doesNotUnderstand:".len, 0) catch return error.DoesNotUnderstand;
            @memcpy(dnu_sel_z, "doesNotUnderstand:");
            const dnu_sel = self.api.intern(dnu_sel_z.ptr);
            if (dnu_sel != null) {
                // Pass the selector as a string object
                const sel_str = builtins.makeString(selector);
                const result = self.api.send1(receiver, dnu_sel, sel_str);
                if (result != null) return result;
            }
        }

        return error.DoesNotUnderstand;
    }

    // -- Phase 5: Advanced Features --------------------------------------------

    fn evalArrayLit(self: *Evaluator, data: ast.Node.ArrayLit, scope: *Env) EvalError!Oop {
        var elements: std.ArrayList(Oop) = .{};
        for (data.elements) |elem| {
            const val = try self.eval(elem, scope);
            elements.append(self.arena, val) catch return error.OutOfMemory;
        }
        return builtins.makeArray(elements.items);
    }

    fn evalCascade(self: *Evaluator, data: ast.Node.Cascade, scope: *Env) EvalError!Oop {
        const receiver = try self.eval(data.receiver, scope);
        for (data.messages) |msg| {
            _ = try self.performSend(receiver, msg.selector, msg.args, scope);
        }
        return receiver; // cascade returns the receiver
    }

    /// Helper: perform a send with unevaluated args (already have receiver)
    fn performSend(self: *Evaluator, receiver: Oop, selector: []const u8, arg_nodes: []const *ast.Node, scope: *Env) EvalError!Oop {
        var args: [3]Oop = .{ null, null, null };
        const n_args = @min(arg_nodes.len, 3);
        for (0..n_args) |i| {
            args[i] = try self.eval(arg_nodes[i], scope);
        }

        return self.dispatchSend(receiver, selector, args[0..n_args]);
    }

    fn evalGlobalAssign(self: *Evaluator, data: ast.Node.GlobalAssign, scope: *Env) EvalError!Oop {
        const value = try self.eval(data.value, scope);
        self.global.define(data.name, value);
        return value;
    }

    fn evalExecBlock(self: *Evaluator, data: ast.Node.ExecBlock, scope: *Env) EvalError!Oop {
        var result: Oop = builtins.getNil();
        for (data.body) |stmt| {
            result = try self.eval(stmt, scope);
        }
        return result;
    }

    fn sendToArray(self: *Evaluator, receiver: Oop, selector: []const u8, args: []const Oop) EvalError!Oop {
        if (std.mem.eql(u8, selector, "size") and args.len == 0) {
            return builtins.intToOop(@as(i64, @intCast(builtins.arraySize(receiver))));
        }
        if (std.mem.eql(u8, selector, "at:") and args.len == 1) {
            if (!builtins.isSmallInt(args[0])) return error.TypeError;
            const idx = builtins.oopToInt(args[0]);
            if (idx < 0) return error.TypeError;
            return builtins.arrayAt(receiver, @intCast(idx));
        }
        if (std.mem.eql(u8, selector, "at:put:") and args.len == 2) {
            if (!builtins.isSmallInt(args[0])) return error.TypeError;
            const idx = builtins.oopToInt(args[0]);
            if (idx < 0) return error.TypeError;
            builtins.arrayAtPut(receiver, @intCast(idx), args[1]);
            return receiver;
        }
        if (std.mem.eql(u8, selector, "do:") and args.len == 1) {
            const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            const arr = builtins.asArrayObj(receiver) orelse return error.TypeError;
            const elems = arr.elements orelse return builtins.getNil();
            for (0..arr.len) |i| {
                _ = try self.evalBlockBody(bd, &.{elems[i]});
            }
            return receiver;
        }
        if (std.mem.eql(u8, selector, "collect:") and args.len == 1) {
            const bd = builtins.asBlockData(args[0]) orelse return error.TypeError;
            const arr = builtins.asArrayObj(receiver) orelse return error.TypeError;
            const elems = arr.elements orelse return builtins.makeArray(&.{});
            var results: std.ArrayList(Oop) = .{};
            for (0..arr.len) |i| {
                const val = try self.evalBlockBody(bd, &.{elems[i]});
                results.append(self.arena, val) catch return error.OutOfMemory;
            }
            return builtins.makeArray(results.items);
        }
        if (std.mem.eql(u8, selector, "inject:into:") and args.len == 2) {
            var acc = args[0];
            const bd = builtins.asBlockData(args[1]) orelse return error.TypeError;
            const arr = builtins.asArrayObj(receiver) orelse return error.TypeError;
            const elems = arr.elements orelse return acc;
            for (0..arr.len) |i| {
                acc = try self.evalBlockBody(bd, &.{ acc, elems[i] });
            }
            return acc;
        }
        return error.DoesNotUnderstand;
    }

    // -- Phase 4: Type Definitions and Methods --------------------------------

    fn evalTypeDef(self: *Evaluator, data: ast.Node.TypeDef, scope: *Env) EvalError!Oop {
        _ = scope;

        // Look up parent type
        const parent_info = builtins.lookupType(data.parent) orelse return error.UndefinedVariable;

        // Create new vtable delegating to parent
        const vt = self.api.vtable_delegated(parent_info.vt) orelse return error.OutOfMemory;

        // Register the type
        const info = builtins.TypeInfo{
            .name = data.name,
            .vt = vt,
            .parent_vt = parent_info.vt,
            .slots = data.slots,
        };
        builtins.registerType(data.name, info);

        // Bind the type name in global env to its vtable
        self.global.define(data.name, vt);

        return vt;
    }

    fn evalMethodDef(self: *Evaluator, data: ast.Node.MethodDef, scope: *Env) EvalError!Oop {
        _ = scope;

        // Look up the type this method is being defined on
        const type_info = builtins.lookupType(data.type_name) orelse return error.UndefinedVariable;

        // Create MethodData
        const md = self.arena.create(builtins.MethodData) catch return error.OutOfMemory;
        md.* = .{
            .selector = data.selector,
            .params = data.params,
            .body = data.body,
            .type_name = data.type_name,
        };

        // Store MethodData as an id object so we can pass it as closure data
        const md_obj = self.api.alloc(self.api.object_vt, @sizeOf(builtins.MethodDataObj)) orelse return error.OutOfMemory;
        const md_ext: *builtins.MethodDataObj = @ptrCast(@alignCast(md_obj));
        md_ext.data = @ptrCast(md);

        // Pick trampoline based on arity
        const n_params = data.params.len;
        const trampoline: ?*const anyopaque = switch (n_params) {
            0 => @ptrCast(&trampoline0),
            1 => @ptrCast(&trampoline1),
            2 => @ptrCast(&trampoline2),
            3 => @ptrCast(&trampoline3),
            else => return error.TypeError,
        };

        // Intern selector and install method on the vtable
        const sel_z = self.arena.allocSentinel(u8, data.selector.len, 0) catch return error.OutOfMemory;
        @memcpy(sel_z, data.selector);
        const sel = self.api.intern(sel_z.ptr);
        if (sel == null) return error.OutOfMemory;

        _ = self.api.vtable_method_at_put_with(type_info.vt, sel, trampoline, md_obj);

        return builtins.getNil();
    }

    /// Evaluate a language-defined method body with self, params, and slot access.
    pub fn evalMethodBody(self: *Evaluator, md: *builtins.MethodData, receiver: Oop, args: []const Oop) Oop {
        // Create method scope
        var scope = Env.init(self.arena, self.global);
        scope.self_ref = receiver;
        scope.is_method_frame = true;

        // Bind 'self'
        scope.define("self", receiver);

        // Bind parameters
        const n = @min(md.params.len, args.len);
        for (0..n) |i| {
            scope.define(md.params[i], args[i]);
        }

        // Evaluate body, catching NonLocalReturn
        var result: Oop = builtins.getNil();
        for (md.body) |stmt| {
            result = self.eval(stmt, &scope) catch |err| {
                if (err == error.NonLocalReturn) {
                    return self.return_value;
                }
                return builtins.getNil();
            };
        }
        return result;
    }

    fn lookupTypeByVt(self: *Evaluator, vt: Oop) ?builtins.TypeInfo {
        _ = self;
        // Find the TypeInfo whose vtable matches
        return builtins.lookupTypeByVt(vt);
    }

    /// Format an Oop as a printable string. Returns slice into result_buf.
    pub fn formatResult(self: *Evaluator, oop: Oop) []const u8 {
        if (oop == null) return "nil";
        if (builtins.isSmallInt(oop)) {
            const val = builtins.oopToInt(oop);
            return std.fmt.bufPrint(&self.result_buf, "{d}", .{val}) catch "?";
        }
        if (builtins.isString(oop)) {
            if (builtins.stringSlice(oop)) |s| {
                if (s.len + 2 <= self.result_buf.len) {
                    self.result_buf[0] = '\'';
                    @memcpy(self.result_buf[1 .. 1 + s.len], s);
                    self.result_buf[1 + s.len] = '\'';
                    return self.result_buf[0 .. s.len + 2];
                }
                return "'...'";
            }
            return "''";
        }
        if (builtins.isBlock(oop)) return "a Block";
        if (builtins.isArray(oop)) {
            const arr = builtins.asArrayObj(oop) orelse return "#()";
            var pos: usize = 0;
            self.result_buf[pos] = '#';
            pos += 1;
            self.result_buf[pos] = '(';
            pos += 1;
            const elems = arr.elements orelse {
                self.result_buf[pos] = ')';
                return self.result_buf[0 .. pos + 1];
            };
            for (0..arr.len) |i| {
                if (i > 0) {
                    if (pos >= self.result_buf.len - 1) break;
                    self.result_buf[pos] = ' ';
                    pos += 1;
                }
                // Format each element inline
                if (builtins.isSmallInt(elems[i])) {
                    const val = builtins.oopToInt(elems[i]);
                    const formatted = std.fmt.bufPrint(self.result_buf[pos..], "{d}", .{val}) catch break;
                    pos += formatted.len;
                } else if (elems[i] == null) {
                    const s = "nil";
                    if (pos + s.len <= self.result_buf.len) {
                        @memcpy(self.result_buf[pos .. pos + s.len], s);
                        pos += s.len;
                    }
                } else {
                    const s = "...";
                    if (pos + s.len <= self.result_buf.len) {
                        @memcpy(self.result_buf[pos .. pos + s.len], s);
                        pos += s.len;
                    }
                    break;
                }
            }
            if (pos < self.result_buf.len) {
                self.result_buf[pos] = ')';
                pos += 1;
            }
            return self.result_buf[0..pos];
        }
        if (oop == builtins.getTrue()) return "true";
        if (oop == builtins.getFalse()) return "false";
        // Check if it's a known type vtable
        if (builtins.typeNameForVt(oop)) |name| {
            return std.fmt.bufPrint(&self.result_buf, "{s}", .{name}) catch "?";
        }
        // Check if it's an instance of a known type
        if (!builtins.isSmallInt(oop) and oop != null) {
            const vt = builtins.getVt(oop);
            if (builtins.typeNameForVt(vt)) |name| {
                return std.fmt.bufPrint(&self.result_buf, "a {s}", .{name}) catch "?";
            }
        }
        return std.fmt.bufPrint(&self.result_buf, "<Object@{x}>", .{@intFromPtr(oop.?)}) catch "?";
    }
};

/// Truthiness: nil and false are falsy, everything else (including SmallInt 0) is truthy.
/// Exception: SmallInt 0 is falsy to support C-style comparisons returning 0/1.
fn isTruthy(oop: Oop) bool {
    if (oop == null) return false; // nil
    if (oop == builtins.getFalse()) return false;
    if (builtins.isSmallInt(oop)) return builtins.oopToInt(oop) != 0;
    return true;
}

// -- Trampolines for language-defined methods --------------------------------

// These are C-callable functions that id's send dispatch invokes.
// They extract MethodData from closure.data, then call back into the evaluator.

// Global evaluator pointer — set during module init, used by trampolines.
var global_evaluator: ?*Evaluator = null;

pub fn setGlobalEvaluator(e: *Evaluator) void {
    global_evaluator = e;
}

const Send = extern struct {
    selector: Oop,
    n_args: c_int,
    receiver: Oop,
    state: Oop,
    closure: Oop, // *Closure as Oop
};

const Closure = extern struct {
    method: ?*const anyopaque,
    data: Oop,
};

fn getMethodData(s: *Send) ?*builtins.MethodData {
    if (s.closure == null) return null;
    const closure: *Closure = @ptrCast(@alignCast(s.closure.?));
    if (closure.data == null) return null;
    const md_obj: *builtins.MethodDataObj = @ptrCast(@alignCast(closure.data.?));
    if (md_obj.data == null) return null;
    return @ptrCast(@alignCast(md_obj.data.?));
}

fn trampoline0(s: *Send) callconv(.c) Oop {
    const e = global_evaluator orelse return null;
    const md = getMethodData(s) orelse return null;
    return e.evalMethodBody(md, s.receiver, &.{});
}

fn trampoline1(s: *Send, a0: Oop) callconv(.c) Oop {
    const e = global_evaluator orelse return null;
    const md = getMethodData(s) orelse return null;
    return e.evalMethodBody(md, s.receiver, &.{a0});
}

fn trampoline2(s: *Send, a0: Oop, a1: Oop) callconv(.c) Oop {
    const e = global_evaluator orelse return null;
    const md = getMethodData(s) orelse return null;
    return e.evalMethodBody(md, s.receiver, &.{ a0, a1 });
}

fn trampoline3(s: *Send, a0: Oop, a1: Oop, a2: Oop) callconv(.c) Oop {
    const e = global_evaluator orelse return null;
    const md = getMethodData(s) orelse return null;
    return e.evalMethodBody(md, s.receiver, &.{ a0, a1, a2 });
}
