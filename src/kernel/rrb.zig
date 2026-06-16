//! Relaxed Radix Balanced Tree (RRB) — persistent indexed sequence.
//!
//! Bagwell & Rompf 2011. Used by Clojure's PersistentVector (strict variant),
//! Scala's Vector, Immer.js. 32-way branching, O(log₃₂ N) lookup/update.
//!
//! Phase 1 spike scope:
//! - Strict radix tree (no tail optimization).
//! - All leaves at the same depth; non-rightmost branches are full.
//! - `nth` / `set` / `push` / `pop` are radix-descent fast paths.
//! - `concat` and `split_at` are correct-but-naive: O(N) via repeated push.
//!   Relaxed-node support (and asymptotically faster concat/split) is a
//!   future optimization, gated by actual use in source-rope / image-log
//!   workloads.
//!
//! Persistent: every operation returns a new root via path-copy. Original
//! vectors remain valid.

const std = @import("std");

const BRANCH: usize = 32;
const LOG_BRANCH: u6 = 5;
const MASK: usize = BRANCH - 1;

pub fn Rrb(comptime V: type) type {
    return struct {
        const Self = @This();

        allocator: std.mem.Allocator,
        root: ?*const Node,
        /// `shift` is the bit-offset for indexing at the root level.
        /// shift == 0 means root is a leaf (or empty).
        /// shift == 5 means root is an inner node whose children are leaves.
        /// shift == 10 means root has children-of-children, etc.
        shift: u6,
        count_: usize,

        const Node = struct {
            is_leaf: bool,
            values: []V, // used iff is_leaf
            children: []*const Node, // used iff !is_leaf
        };

        pub fn empty(allocator: std.mem.Allocator) Self {
            return .{
                .allocator = allocator,
                .root = null,
                .shift = 0,
                .count_ = 0,
            };
        }

        pub fn count(self: Self) usize {
            return self.count_;
        }

        // -- bulk build -----------------------------------------------------
        //
        // Build a tree directly from a slice, bottom-up, allocating each node
        // exactly once. Unlike repeated `push` (which path-copies and leaks
        // O(N·log N) superseded intermediates), this is O(N) allocations with
        // zero garbage — so a `fromSlice`-built tree is fully reclaimed by
        // `deinitTree`. Produces the same strict-radix shape `push` would
        // (leaves chunked by BRANCH, all at one depth, non-rightmost full), so
        // `nth`/`set`/`push`/`pop` behave identically. (Phase 15.P.)
        pub fn fromSlice(allocator: std.mem.Allocator, values: []const V) !Self {
            if (values.len == 0) return empty(allocator);

            // Leaf level: chunk the values into leaves of up to BRANCH.
            var level = std.ArrayList(*const Node){};
            defer level.deinit(allocator);
            var i: usize = 0;
            while (i < values.len) {
                const end = @min(i + BRANCH, values.len);
                const vals = try allocator.alloc(V, end - i);
                @memcpy(vals, values[i..end]);
                const node = try allocator.create(Node);
                node.* = .{ .is_leaf = true, .values = vals, .children = &.{} };
                try level.append(allocator, node);
                i = end;
            }

            // Inner levels: chunk child nodes by BRANCH until a single root.
            var shift: u6 = 0;
            while (level.items.len > 1) {
                var parents = std.ArrayList(*const Node){};
                var j: usize = 0;
                while (j < level.items.len) {
                    const end = @min(j + BRANCH, level.items.len);
                    const kids = try allocator.alloc(*const Node, end - j);
                    @memcpy(kids, level.items[j..end]);
                    const node = try allocator.create(Node);
                    node.* = .{ .is_leaf = false, .values = &.{}, .children = kids };
                    try parents.append(allocator, node);
                    j = end;
                }
                level.deinit(allocator);
                level = parents;
                shift += LOG_BRANCH;
            }

            return .{
                .allocator = allocator,
                .root = level.items[0],
                .shift = shift,
                .count_ = values.len,
            };
        }

        // -- nth ------------------------------------------------------------

        pub fn nth(self: Self, idx: usize) V {
            std.debug.assert(idx < self.count_);
            const root = self.root.?;
            return nodeNth(root, idx, self.shift);
        }

        fn nodeNth(node: *const Node, idx: usize, shift: u6) V {
            if (node.is_leaf) {
                return node.values[idx];
            }
            const child_idx = (idx >> shift) & MASK;
            const remaining = idx & ((@as(usize, 1) << shift) - 1);
            return nodeNth(node.children[child_idx], remaining, shift - LOG_BRANCH);
        }

        // -- set ------------------------------------------------------------

        pub fn set(self: Self, idx: usize, value: V) !Self {
            std.debug.assert(idx < self.count_);
            const new_root = try nodeSet(self.allocator, self.root.?, idx, value, self.shift);
            return .{
                .allocator = self.allocator,
                .root = new_root,
                .shift = self.shift,
                .count_ = self.count_,
            };
        }

        fn nodeSet(
            allocator: std.mem.Allocator,
            node: *const Node,
            idx: usize,
            value: V,
            shift: u6,
        ) !*const Node {
            if (node.is_leaf) {
                const new_values = try allocator.alloc(V, node.values.len);
                @memcpy(new_values, node.values);
                new_values[idx] = value;
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .is_leaf = true,
                    .values = new_values,
                    .children = &.{},
                };
                return new_node;
            }
            const child_idx = (idx >> shift) & MASK;
            const remaining = idx & ((@as(usize, 1) << shift) - 1);
            const new_child = try nodeSet(allocator, node.children[child_idx], remaining, value, shift - LOG_BRANCH);
            const new_children = try allocator.alloc(*const Node, node.children.len);
            @memcpy(new_children, node.children);
            new_children[child_idx] = new_child;
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .is_leaf = false,
                .values = &.{},
                .children = new_children,
            };
            return new_node;
        }

        // -- push -----------------------------------------------------------

        pub fn push(self: Self, value: V) !Self {
            // Empty vector: create a single-leaf root.
            if (self.root == null) {
                const new_root = try makeLeafSingle(self.allocator, value);
                return .{
                    .allocator = self.allocator,
                    .root = new_root,
                    .shift = 0,
                    .count_ = 1,
                };
            }

            // Capacity at the current shift: BRANCH^(shift/LOG_BRANCH + 1).
            const root_capacity: usize = @as(usize, 1) << (self.shift + LOG_BRANCH);
            if (self.count_ < root_capacity) {
                // Room in the existing tree.
                const new_root = try nodePush(self.allocator, self.root.?, value, self.shift, self.count_);
                return .{
                    .allocator = self.allocator,
                    .root = new_root,
                    .shift = self.shift,
                    .count_ = self.count_ + 1,
                };
            }

            // Need to grow root: wrap current root in a new inner node, then push into the new right path.
            const right_path = try newPath(self.allocator, value, self.shift);
            const new_children = try self.allocator.alloc(*const Node, 2);
            new_children[0] = self.root.?;
            new_children[1] = right_path;
            const new_root = try self.allocator.create(Node);
            new_root.* = .{
                .is_leaf = false,
                .values = &.{},
                .children = new_children,
            };
            return .{
                .allocator = self.allocator,
                .root = new_root,
                .shift = self.shift + LOG_BRANCH,
                .count_ = self.count_ + 1,
            };
        }

        /// Push one value into a node assuming there is room (count < capacity at this level).
        /// `count` is the current logical count *under this node*.
        fn nodePush(
            allocator: std.mem.Allocator,
            node: *const Node,
            value: V,
            shift: u6,
            cur_count: usize,
        ) anyerror!*const Node {
            if (node.is_leaf) {
                std.debug.assert(node.values.len < BRANCH);
                const new_values = try allocator.alloc(V, node.values.len + 1);
                @memcpy(new_values[0..node.values.len], node.values);
                new_values[node.values.len] = value;
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .is_leaf = true,
                    .values = new_values,
                    .children = &.{},
                };
                return new_node;
            }
            // Internal node. Decide whether to descend into the rightmost child or to spawn a new child.
            const child_idx = (cur_count >> shift) & MASK;
            const child_shift = shift - LOG_BRANCH;
            const child_capacity: usize = @as(usize, 1) << shift;

            if (child_idx < node.children.len) {
                // Descend.
                const remaining = cur_count & ((@as(usize, 1) << shift) - 1);
                _ = child_capacity;
                const new_child = try nodePush(allocator, node.children[child_idx], value, child_shift, remaining);
                const new_children = try allocator.alloc(*const Node, node.children.len);
                @memcpy(new_children, node.children);
                new_children[child_idx] = new_child;
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .is_leaf = false,
                    .values = &.{},
                    .children = new_children,
                };
                return new_node;
            }
            // Spawn a new rightmost child path.
            const new_child = try newPath(allocator, value, child_shift);
            const new_children = try allocator.alloc(*const Node, node.children.len + 1);
            @memcpy(new_children[0..node.children.len], node.children);
            new_children[node.children.len] = new_child;
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .is_leaf = false,
                .values = &.{},
                .children = new_children,
            };
            return new_node;
        }

        /// Build a fresh spine of inner nodes at `shift`, terminating in a leaf with a single value.
        fn newPath(allocator: std.mem.Allocator, value: V, shift: u6) anyerror!*const Node {
            if (shift == 0) {
                return makeLeafSingle(allocator, value);
            }
            const child = try newPath(allocator, value, shift - LOG_BRANCH);
            const children = try allocator.alloc(*const Node, 1);
            children[0] = child;
            const node = try allocator.create(Node);
            node.* = .{
                .is_leaf = false,
                .values = &.{},
                .children = children,
            };
            return node;
        }

        fn makeLeafSingle(allocator: std.mem.Allocator, value: V) !*const Node {
            const values = try allocator.alloc(V, 1);
            values[0] = value;
            const node = try allocator.create(Node);
            node.* = .{
                .is_leaf = true,
                .values = values,
                .children = &.{},
            };
            return node;
        }

        // -- deep free ------------------------------------------------------
        //
        // Frees every node reachable from the root plus the per-node `values` /
        // `children` slices. DANGER: this is a *deep* free with no refcounting.
        // It is safe ONLY when this tree shares no nodes with any other live
        // RRB — i.e. a freshly built vector that was never handed to `set` /
        // `push` / `rrbWith` to produce a still-live structurally-shared
        // sibling. A compiler constant pool qualifies (built once via `mkRrb`,
        // referenced only by its `:constant-pool` fact, never shared). Leaves
        // the handle empty so a second call is a no-op. (Phase 15.P.)
        pub fn deinitTree(self: *Self) void {
            if (self.root) |r| freeNode(self.allocator, r);
            self.root = null;
            self.count_ = 0;
        }

        fn freeNode(allocator: std.mem.Allocator, node: *const Node) void {
            if (node.is_leaf) {
                allocator.free(node.values);
            } else {
                for (node.children) |c| freeNode(allocator, c);
                allocator.free(node.children);
            }
            allocator.destroy(node);
        }

        // -- pop ------------------------------------------------------------

        pub fn pop(self: Self) !Self {
            std.debug.assert(self.count_ > 0);

            if (self.count_ == 1) {
                return .{
                    .allocator = self.allocator,
                    .root = null,
                    .shift = 0,
                    .count_ = 0,
                };
            }

            const new_root_opt = try nodePop(self.allocator, self.root.?, self.shift, self.count_ - 1);
            // If root is internal and has collapsed to a single child, lower the shift.
            var root = new_root_opt.?;
            var shift = self.shift;
            while (shift > 0 and !root.is_leaf and root.children.len == 1) {
                root = root.children[0];
                shift -= LOG_BRANCH;
            }
            return .{
                .allocator = self.allocator,
                .root = root,
                .shift = shift,
                .count_ = self.count_ - 1,
            };
        }

        /// Returns new node after popping. `last_idx` is the index of the value being removed
        /// (i.e. count-1 before pop). Returns null only when the node disappears entirely;
        /// for the root caller, this should never happen given the count_==1 short-circuit above.
        fn nodePop(
            allocator: std.mem.Allocator,
            node: *const Node,
            shift: u6,
            last_idx: usize,
        ) anyerror!?*const Node {
            if (node.is_leaf) {
                std.debug.assert(node.values.len > 0);
                if (node.values.len == 1) return null;
                const new_values = try allocator.alloc(V, node.values.len - 1);
                @memcpy(new_values, node.values[0 .. node.values.len - 1]);
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .is_leaf = true,
                    .values = new_values,
                    .children = &.{},
                };
                return new_node;
            }

            const child_idx = (last_idx >> shift) & MASK;
            const remaining = last_idx & ((@as(usize, 1) << shift) - 1);
            const child = node.children[child_idx];
            const popped_child = try nodePop(allocator, child, shift - LOG_BRANCH, remaining);

            if (popped_child) |c| {
                const new_children = try allocator.alloc(*const Node, node.children.len);
                @memcpy(new_children, node.children);
                new_children[child_idx] = c;
                const new_node = try allocator.create(Node);
                new_node.* = .{
                    .is_leaf = false,
                    .values = &.{},
                    .children = new_children,
                };
                return new_node;
            }

            // Child fully disappeared. Drop it.
            if (node.children.len == 1) {
                return null;
            }
            const new_children = try allocator.alloc(*const Node, node.children.len - 1);
            @memcpy(new_children, node.children[0 .. node.children.len - 1]);
            const new_node = try allocator.create(Node);
            new_node.* = .{
                .is_leaf = false,
                .values = &.{},
                .children = new_children,
            };
            return new_node;
        }

        // -- concat (naive) -------------------------------------------------
        //
        // TODO Phase 8/10+ — replace with relaxed-radix concat from Bagwell & Rompf 2011.
        // O(N) currently; needed for source ropes and image-log random splice.
        pub fn concat(self: Self, other: Self) !Self {
            var result = self;
            var it = other.iter();
            while (it.next()) |v| {
                result = try result.push(v);
            }
            return result;
        }

        // -- split (naive) --------------------------------------------------
        //
        // TODO Phase 8/10+ — replace with relaxed-radix split.
        pub fn splitAt(self: Self, idx: usize) !struct { Self, Self } {
            std.debug.assert(idx <= self.count_);
            var left = Self.empty(self.allocator);
            var right = Self.empty(self.allocator);
            var i: usize = 0;
            while (i < idx) : (i += 1) left = try left.push(self.nth(i));
            while (i < self.count_) : (i += 1) right = try right.push(self.nth(i));
            return .{ left, right };
        }

        // -- iterator -------------------------------------------------------

        pub const Iterator = struct {
            vec: *const Self,
            idx: usize,

            pub fn next(self: *Iterator) ?V {
                if (self.idx >= self.vec.count_) return null;
                const v = self.vec.nth(self.idx);
                self.idx += 1;
                return v;
            }
        };

        pub fn iter(self: *const Self) Iterator {
            return .{ .vec = self, .idx = 0 };
        }
    };
}
