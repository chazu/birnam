//! Fact types for the relational fact store.
//!
//! Six-column fact tuple (VISION.md §Fact):
//!   [ e  a  v  tx  op  m ]
//!     |  |  |  |   |   |
//!     |  |  |  |   |   source module EID
//!     |  |  |  |   assert | retract
//!     |  |  |  monotonic u64
//!     |  |  Ref (tagged value — NEVER NIL, per Date discipline)
//!     |  attribute EID (also an entity; schema is in db)
//!     entity EID
//!
//! Two cell kinds back the indexes:
//!
//! - `ValueCell` keys terminate at an (entity, attribute) location; the cell
//!   carries the current value plus history. Used by EAVT, AEVT, MEAT —
//!   indexes whose key paths do not include `v`.
//!
//! - `PresenceCell` keys terminate at an (entity, attribute, value) location;
//!   the cell only needs to remember whether the triple is currently present
//!   plus its history. The value is already known from the key path. Used
//!   by AVET, VAET.

const std = @import("std");
const ref_mod = @import("ref.zig");
const rrb = @import("rrb.zig");

pub const Ref = ref_mod.Ref;

pub const Op = enum(u8) {
    assert = 1,
    retract = 2,
};

/// History entry for a `ValueCell`. Carries the asserted value because
/// EAVT/AEVT/MEAT keys don't include `v`.
pub const ValueHist = struct {
    v: Ref,
    /// Epoch-local tx (resets on compact). Per-epoch coordinate.
    tx: u64,
    op: Op,
    /// Phase 15.R — durable basis-T (never resets on compact). Exact `asOf:`
    /// walks history by this. 0 for entries written before basis tracking
    /// (e.g. `frozenInsert`, which is read-only afterward).
    basis: u64 = 0,
};

/// History entry for a `PresenceCell`. No value needed — it's in the key path.
pub const PresenceHist = struct {
    tx: u64,
    op: Op,
    /// Phase 15.R — durable basis-T (see `ValueHist.basis`).
    basis: u64 = 0,
};

pub const ValueHistVec = rrb.Rrb(ValueHist);
pub const PresenceHistVec = rrb.Rrb(PresenceHist);

/// Cell at the leaf of an (entity, attribute) index path (EAVT, AEVT, MEAT).
pub const ValueCell = struct {
    current_v: ?Ref, // null = absent (retracted or never asserted). Date: no NIL value.
    history: ValueHistVec,
};

/// Cell at the leaf of an (entity, attribute, value) index path (AVET, VAET).
pub const PresenceCell = struct {
    present: bool,
    history: PresenceHistVec,
};
