//! Hash Array Mapped Trie (HAMT) — persistent map.
//!
//! Design notes:
//! - 32-way branching (5 bits of hash per level).
//! - CHAMP-shaped (Steindorfer & Vinju 2015): split bitmap into a value-slot
//!   bitmap (`data_map`) and a subnode-slot bitmap (`node_map`). The two
//!   bitmaps are disjoint — a slot is either an inline (K,V) pair or a
//!   pointer to a child subnode, never both.
//! - 64-bit hash gives ~12 levels of branching before exhaustion. Beyond
//!   that, collision nodes hold a linear bucket of entries with identical
//!   full hashes.
//! - Persistent: every mutation produces a new root via path-copy. Old
//!   roots remain valid. No mutation of existing nodes ever.
//! - Allocator passed in. Phase 1 spike: leak everything. Pruning + GC
//!   are downstream phases.
//!
//! Public API:
//!   const M = Hamt(K, V, Ctx);
//!   var m = M.empty(allocator, ctx);
//!   m = try m.assoc(k, v);
//!   if (m.get(k)) |v| ...
//!   m = try m.dissoc(k);
//!   var it = m.iter(); while (it.next()) |entry| ...
//!
//! `Ctx` provides `hash(K) u64` and `eql(K, K) bool`.

const std = @import("std");

pub fn Hamt(comptime K: type, comptime V: type, comptime Ctx: type) type {
    return struct {
        const Self = @This();
        const MAX_DEPTH: u6 = 13; // 13 levels of 5 bits + 1 level of partial bits

        allocator: std.mem.Allocator,
        ctx: Ctx,
        root: ?*const Node,
        count_: usize,

        pub const KV = struct {
            key: K,
            value: V,
            hash: u64,
        };

        const Node = struct {
            data_map: u32,
            node_map: u32,
            kvs: []KV,
            subnodes: []*const Node,
            /// If true, this is a collision bucket. `data_map` and `node_map`
            /// are unused; `kvs` holds all colliding entries.
            collision: bool,
        };

        pub fn empty(allocator: std.mem.Allocator, ctx: Ctx) Self {
            return .{
                .allocator = allocator,
                .ctx = ctx,
                .root = null,
                .count_ = 0,
            };
        }

        pub fn count(self: Self) usize {
            return self.count_;
        }

        pub fn get(self: Self, key: K) ?V {
            const root = self.root orelse return null;
            const h = self.ctx.hash(key);
            return nodeGet(self.ctx, root, key, h, 0);
        }

        fn nodeGet(ctx: Ctx, node: *const Node, key: K, h: u64, depth: u6) ?V {
            if (node.collision) {
                for (node.kvs) |kv| {
                    if (ctx.eql(kv.key, key)) return kv.value;
                }
                return null;
            }
            const bit: u5 = @intCast((h >> (@as(u6, depth) * 5)) & 0x1F);
            const mask: u32 = @as(u32, 1) << bit;
            if (node.data_map & mask != 0) {
                const idx = @popCount(node.data_map & (mask - 1));
                const kv = node.kvs[idx];
                if (ctx.eql(kv.key, key)) return kv.value;
                return null;
            }
            if (node.node_map & mask != 0) {
                const idx = @popCount(node.node_map & (mask - 1));
                return nodeGet(ctx, node.subnodes[idx], key, h, depth + 1);
            }
            return null;
        }

        pub fn assoc(self: Self, key: K, value: V) !Self {
            const h = self.ctx.hash(key);
            const root = self.root orelse blk: {
                const new_root = try makeLeaf(self.allocator, key, value, h);
                break :blk new_root;
            };
            // For empty root, makeLeaf already returns the node we want.
            if (self.root == null) {
                return .{
                    .allocator = self.allocator,
                    .ctx = self.ctx,
                    .root = root,
                    .count_ = 1,
                };
            }

            const result = try nodeAssoc(self.allocator, self.ctx, root, key, value, h, 0);
            return .{
                .allocator = self.allocator,
                .ctx = self.ctx,
                .root = result.node,
                .count_ = self.count_ + @as(usize, if (result.added) 1 else 0),
            };
        }

        const AssocResult = struct {
            node: *const Node,
            added: bool, // true if a new key was added, false if existing key was updated
        };

        fn nodeAssoc(
            allocator: std.mem.Allocator,
            ctx: Ctx,
            node: *const Node,
            key: K,
            value: V,
            h: u64,
            depth: u6,
        ) !AssocResult {
            if (node.collision) {
                // Linear scan for existing key
                for (node.kvs, 0..) |kv, i| {
                    if (ctx.eql(kv.key, key)) {
                        // Update
                        const new_kvs = try allocator.alloc(KV, node.kvs.len);
                        @memcpy(new_kvs, node.kvs);
                        new_kvs[i] = .{ .key = key, .value = value, .hash = h };
                        return .{ .node = try makeCollision(allocator, new_kvs), .added = false };
                    }
                }
                // Append
                const new_kvs = try allocator.alloc(KV, node.kvs.len + 1);
                @memcpy(new_kvs[0..node.kvs.len], node.kvs);
                new_kvs[node.kvs.len] = .{ .key = key, .value = value, .hash = h };
                return .{ .node = try makeCollision(allocator, new_kvs), .added = true };
            }

            const bit: u5 = @intCast((h >> (@as(u6, depth) * 5)) & 0x1F);
            const mask: u32 = @as(u32, 1) << bit;

            if (node.data_map & mask != 0) {
                // Existing data entry at this slot
                const idx = @popCount(node.data_map & (mask - 1));
                const existing = node.kvs[idx];
                if (ctx.eql(existing.key, key)) {
                    // Update value in place (copy node, replace kv)
                    const new_kvs = try allocator.alloc(KV, node.kvs.len);
                    @memcpy(new_kvs, node.kvs);
                    new_kvs[idx] = .{ .key = key, .value = value, .hash = h };
                    const new_node = try allocator.create(Node);
                    new_node.* = .{
                        .data_map = node.data_map,
                        .node_map = node.node_map,
                        .kvs = new_kvs,
                        .subnodes = try dupSubnodes(allocator, node.subnodes),
                        .collision = false,
                    };
                    return .{ .node = new_node, .added = false };
                }
                // Hash collision (at this prefix). Spawn subnode holding both.
                const subnode = try mergeTwo(allocator, ctx, existing, .{ .key = key, .value = value, .hash = h }, depth + 1);
                // Build new node: drop data entry, add subnode entry
                return .{ .node = try replaceDataWithSubnode(allocator, node, idx, bit, subnode), .added = true };
            }

            if (node.node_map & mask != 0) {
                // Recurse into subnode
                const idx = @popCount(node.node_map & (mask - 1));
                const inner = try nodeAssoc(allocator, ctx, node.subnodes[idx], key, value, h, depth + 1);
                // Replace subnode pointer in copy of this node
                const new_subnodes = try allocator.alloc(*const Node, node.subnodes.len);
                @memcpy(new_subnodes, node.subnodes);
                new_subnodes[idx] = inner.node;
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .data_map = node.data_map,
                    .node_map = node.node_map,
                    .kvs = try dupKvs(allocator, node.kvs),
                    .subnodes = new_subnodes,
                    .collision = false,
                };
                return .{ .node = new_node, .added = inner.added };
            }

            // Insert new data entry
            return .{ .node = try insertData(allocator, node, bit, .{ .key = key, .value = value, .hash = h }), .added = true };
        }

        pub fn dissoc(self: Self, key: K) !Self {
            const root = self.root orelse return self;
            const h = self.ctx.hash(key);
            const res = try nodeDissoc(self.allocator, self.ctx, root, key, h, 0);
            if (!res.removed) return self;
            const new_root: ?*const Node = if (res.node == null) null else res.node;
            return .{
                .allocator = self.allocator,
                .ctx = self.ctx,
                .root = new_root,
                .count_ = self.count_ - 1,
            };
        }

        const DissocResult = struct {
            node: ?*const Node,
            removed: bool,
        };

        fn nodeDissoc(
            allocator: std.mem.Allocator,
            ctx: Ctx,
            node: *const Node,
            key: K,
            h: u64,
            depth: u6,
        ) !DissocResult {
            if (node.collision) {
                for (node.kvs, 0..) |kv, i| {
                    if (ctx.eql(kv.key, key)) {
                        if (node.kvs.len == 1) {
                            return .{ .node = null, .removed = true };
                        }
                        if (node.kvs.len == 2) {
                            // Collapse collision bucket into a leaf.
                            const surviving = node.kvs[1 - i];
                            return .{ .node = try makeLeaf(allocator, surviving.key, surviving.value, surviving.hash), .removed = true };
                        }
                        const new_kvs = try allocator.alloc(KV, node.kvs.len - 1);
                        @memcpy(new_kvs[0..i], node.kvs[0..i]);
                        @memcpy(new_kvs[i..], node.kvs[i + 1 ..]);
                        return .{ .node = try makeCollision(allocator, new_kvs), .removed = true };
                    }
                }
                return .{ .node = node, .removed = false };
            }

            const bit: u5 = @intCast((h >> (@as(u6, depth) * 5)) & 0x1F);
            const mask: u32 = @as(u32, 1) << bit;

            if (node.data_map & mask != 0) {
                const idx = @popCount(node.data_map & (mask - 1));
                const existing = node.kvs[idx];
                if (!ctx.eql(existing.key, key)) return .{ .node = node, .removed = false };
                // Remove this data entry
                if (node.kvs.len == 1 and node.subnodes.len == 0) {
                    return .{ .node = null, .removed = true };
                }
                const new_kvs = try allocator.alloc(KV, node.kvs.len - 1);
                @memcpy(new_kvs[0..idx], node.kvs[0..idx]);
                @memcpy(new_kvs[idx..], node.kvs[idx + 1 ..]);
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .data_map = node.data_map & ~mask,
                    .node_map = node.node_map,
                    .kvs = new_kvs,
                    .subnodes = try dupSubnodes(allocator, node.subnodes),
                    .collision = false,
                };
                return .{ .node = new_node, .removed = true };
            }

            if (node.node_map & mask != 0) {
                const idx = @popCount(node.node_map & (mask - 1));
                const inner = try nodeDissoc(allocator, ctx, node.subnodes[idx], key, h, depth + 1);
                if (!inner.removed) return .{ .node = node, .removed = false };

                // Inner may have collapsed to a single entry. Detect and inline.
                if (inner.node) |child| {
                    if (!child.collision and child.subnodes.len == 0 and child.kvs.len == 1) {
                        // Inline the lone KV back into this node, drop subnode slot.
                        return .{ .node = try inlineSubnodeAsData(allocator, node, idx, bit, child.kvs[0]), .removed = true };
                    }
                    // Replace subnode pointer
                    const new_subnodes = try allocator.alloc(*const Node, node.subnodes.len);
                    @memcpy(new_subnodes, node.subnodes);
                    new_subnodes[idx] = child;
                    const new_node = try allocator.create(Node);
                    new_node.* = .{
                        .data_map = node.data_map,
                        .node_map = node.node_map,
                        .kvs = try dupKvs(allocator, node.kvs),
                        .subnodes = new_subnodes,
                        .collision = false,
                    };
                    return .{ .node = new_node, .removed = true };
                } else {
                    // Subnode disappeared entirely. Drop it from this node.
                    if (node.kvs.len == 0 and node.subnodes.len == 1) {
                        return .{ .node = null, .removed = true };
                    }
                    const new_subnodes = try allocator.alloc(*const Node, node.subnodes.len - 1);
                    @memcpy(new_subnodes[0..idx], node.subnodes[0..idx]);
                    @memcpy(new_subnodes[idx..], node.subnodes[idx + 1 ..]);
                    const new_node = try allocator.create(Node);
                    new_node.* = .{
                        .data_map = node.data_map,
                        .node_map = node.node_map & ~mask,
                        .kvs = try dupKvs(allocator, node.kvs),
                        .subnodes = new_subnodes,
                        .collision = false,
                    };
                    return .{ .node = new_node, .removed = true };
                }
            }

            return .{ .node = node, .removed = false };
        }

        // -- Iterator -----------------------------------------------------------

        pub const Iterator = struct {
            stack: [MAX_DEPTH + 1]Frame,
            depth: i32,

            const Frame = struct {
                node: *const Node,
                kv_idx: usize,
                sub_idx: usize,
            };

            pub fn next(self: *Iterator) ?KV {
                while (self.depth >= 0) {
                    const d: usize = @intCast(self.depth);
                    const f = &self.stack[d];
                    if (f.kv_idx < f.node.kvs.len) {
                        const kv = f.node.kvs[f.kv_idx];
                        f.kv_idx += 1;
                        return kv;
                    }
                    if (!f.node.collision and f.sub_idx < f.node.subnodes.len) {
                        const child = f.node.subnodes[f.sub_idx];
                        f.sub_idx += 1;
                        self.depth += 1;
                        self.stack[d + 1] = .{ .node = child, .kv_idx = 0, .sub_idx = 0 };
                        continue;
                    }
                    self.depth -= 1;
                }
                return null;
            }
        };

        pub fn iter(self: Self) Iterator {
            var it = Iterator{
                .stack = undefined,
                .depth = -1,
            };
            if (self.root) |r| {
                it.stack[0] = .{ .node = r, .kv_idx = 0, .sub_idx = 0 };
                it.depth = 0;
            }
            return it;
        }

        // -- Internal helpers ---------------------------------------------------

        fn dupKvs(allocator: std.mem.Allocator, src: []KV) ![]KV {
            const out = try allocator.alloc(KV, src.len);
            @memcpy(out, src);
            return out;
        }

        fn dupSubnodes(allocator: std.mem.Allocator, src: []*const Node) ![]*const Node {
            const out = try allocator.alloc(*const Node, src.len);
            @memcpy(out, src);
            return out;
        }

        fn makeLeaf(allocator: std.mem.Allocator, key: K, value: V, h: u64) !*const Node {
            const kvs = try allocator.alloc(KV, 1);
            kvs[0] = .{ .key = key, .value = value, .hash = h };
            const subnodes = try allocator.alloc(*const Node, 0);
            const bit: u5 = @intCast(h & 0x1F);
            const node = try allocator.create(Node);
            node.* = .{
                .data_map = @as(u32, 1) << bit,
                .node_map = 0,
                .kvs = kvs,
                .subnodes = subnodes,
                .collision = false,
            };
            return node;
        }

        fn makeCollision(allocator: std.mem.Allocator, kvs: []KV) !*const Node {
            const subnodes = try allocator.alloc(*const Node, 0);
            const node = try allocator.create(Node);
            node.* = .{
                .data_map = 0,
                .node_map = 0,
                .kvs = kvs,
                .subnodes = subnodes,
                .collision = true,
            };
            return node;
        }

        fn mergeTwo(
            allocator: std.mem.Allocator,
            ctx: Ctx,
            a: KV,
            b: KV,
            depth: u6,
        ) anyerror!*const Node {
            _ = ctx;
            if (depth >= MAX_DEPTH) {
                // Same hash. Collision bucket.
                const kvs = try allocator.alloc(KV, 2);
                kvs[0] = a;
                kvs[1] = b;
                return makeCollision(allocator, kvs);
            }
            const shift = @as(u6, depth) * 5;
            const a_bit: u5 = @intCast((a.hash >> shift) & 0x1F);
            const b_bit: u5 = @intCast((b.hash >> shift) & 0x1F);
            if (a_bit == b_bit) {
                // Still colliding at this level. Recurse.
                const inner = try mergeTwo(allocator, undefined, a, b, depth + 1);
                const subnodes = try allocator.alloc(*const Node, 1);
                subnodes[0] = inner;
                const kvs = try allocator.alloc(KV, 0);
                const node = try allocator.create(Node);
                node.* = .{
                    .data_map = 0,
                    .node_map = @as(u32, 1) << a_bit,
                    .kvs = kvs,
                    .subnodes = subnodes,
                    .collision = false,
                };
                return node;
            }
            // Bits differ at this level. Two data entries.
            const kvs = try allocator.alloc(KV, 2);
            const subnodes = try allocator.alloc(*const Node, 0);
            if (a_bit < b_bit) {
                kvs[0] = a;
                kvs[1] = b;
            } else {
                kvs[0] = b;
                kvs[1] = a;
            }
            const node = try allocator.create(Node);
            node.* = .{
                .data_map = (@as(u32, 1) << a_bit) | (@as(u32, 1) << b_bit),
                .node_map = 0,
                .kvs = kvs,
                .subnodes = subnodes,
                .collision = false,
            };
            return node;
        }

        fn insertData(
            allocator: std.mem.Allocator,
            node: *const Node,
            bit: u5,
            new_kv: KV,
        ) !*const Node {
            const mask: u32 = @as(u32, 1) << bit;
            const idx = @popCount(node.data_map & (mask - 1));
            const new_kvs = try allocator.alloc(KV, node.kvs.len + 1);
            @memcpy(new_kvs[0..idx], node.kvs[0..idx]);
            new_kvs[idx] = new_kv;
            @memcpy(new_kvs[idx + 1 ..], node.kvs[idx..]);
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .data_map = node.data_map | mask,
                .node_map = node.node_map,
                .kvs = new_kvs,
                .subnodes = try dupSubnodes(allocator, node.subnodes),
                .collision = false,
            };
            return new_node;
        }

        fn replaceDataWithSubnode(
            allocator: std.mem.Allocator,
            node: *const Node,
            data_idx: u32,
            bit: u5,
            subnode: *const Node,
        ) !*const Node {
            const mask: u32 = @as(u32, 1) << bit;
            // Remove data entry
            const new_kvs = try allocator.alloc(KV, node.kvs.len - 1);
            @memcpy(new_kvs[0..data_idx], node.kvs[0..data_idx]);
            @memcpy(new_kvs[data_idx..], node.kvs[data_idx + 1 ..]);
            // Insert subnode at its position
            const new_node_map = node.node_map | mask;
            const sub_idx = @popCount(node.node_map & (mask - 1));
            const new_subnodes = try allocator.alloc(*const Node, node.subnodes.len + 1);
            @memcpy(new_subnodes[0..sub_idx], node.subnodes[0..sub_idx]);
            new_subnodes[sub_idx] = subnode;
            @memcpy(new_subnodes[sub_idx + 1 ..], node.subnodes[sub_idx..]);
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .data_map = node.data_map & ~mask,
                .node_map = new_node_map,
                .kvs = new_kvs,
                .subnodes = new_subnodes,
                .collision = false,
            };
            return new_node;
        }

        fn inlineSubnodeAsData(
            allocator: std.mem.Allocator,
            node: *const Node,
            sub_idx: u32,
            bit: u5,
            inlined_kv: KV,
        ) !*const Node {
            const mask: u32 = @as(u32, 1) << bit;
            // Remove subnode
            const new_subnodes = try allocator.alloc(*const Node, node.subnodes.len - 1);
            @memcpy(new_subnodes[0..sub_idx], node.subnodes[0..sub_idx]);
            @memcpy(new_subnodes[sub_idx..], node.subnodes[sub_idx + 1 ..]);
            // Insert data entry
            const new_data_map = node.data_map | mask;
            const data_idx = @popCount(node.data_map & (mask - 1));
            const new_kvs = try allocator.alloc(KV, node.kvs.len + 1);
            @memcpy(new_kvs[0..data_idx], node.kvs[0..data_idx]);
            new_kvs[data_idx] = inlined_kv;
            @memcpy(new_kvs[data_idx + 1 ..], node.kvs[data_idx..]);
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .data_map = new_data_map,
                .node_map = node.node_map & ~mask,
                .kvs = new_kvs,
                .subnodes = new_subnodes,
                .collision = false,
            };
            return new_node;
        }
    };
}

// -- Convenience: default contexts for common key types ------------------------

pub const U64Ctx = struct {
    pub fn hash(_: U64Ctx, k: u64) u64 {
        // Wyhash on the 8 bytes of the u64
        return std.hash.Wyhash.hash(0, std.mem.asBytes(&k));
    }
    pub fn eql(_: U64Ctx, a: u64, b: u64) bool {
        return a == b;
    }
};

pub const StrCtx = struct {
    pub fn hash(_: StrCtx, k: []const u8) u64 {
        return std.hash.Wyhash.hash(0, k);
    }
    pub fn eql(_: StrCtx, a: []const u8, b: []const u8) bool {
        return std.mem.eql(u8, a, b);
    }
};
