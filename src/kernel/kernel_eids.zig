//! Well-known kernel EIDs.
//!
//! Locked in VISION.md §Open Questions: kernel EIDs occupy 0..63;
//! user EIDs allocate from 64+. Renumbering this block is a
//! breaking image-format change.
//!
//! Layout convention:
//!   0       NIL (the nothing-entity)
//!   1..15   kernel vtables
//!   16..63  kernel selectors and well-known attributes
//!   64+     user-allocated

pub const USER_EID_START: u64 = 64;

// -- Entities -----------------------------------------------------------------

pub const NIL: u64 = 0;

// -- Kernel vtables (1..15) ---------------------------------------------------

pub const VTABLE_VT: u64 = 1;
pub const OBJECT_VT: u64 = 2;
pub const SELECTOR_VT: u64 = 3;
pub const CLOSURE_VT: u64 = 4;
pub const VECTOR_VT: u64 = 5;
pub const ASSOC_VT: u64 = 6;
// 7..15 reserved for future kernel vtables

// -- Kernel selectors / attributes (16..63) -----------------------------------

pub const SEL_LOOKUP: u64 = 16;
pub const SEL_METHOD_AT_PUT_WITH: u64 = 17;
pub const SEL_DELEGATED: u64 = 18;
pub const SEL_VTABLE: u64 = 19;
pub const SEL_INIT: u64 = 20;
pub const SEL_ALLOCATE: u64 = 21;

/// `:delegate` attribute on a vtable entity. Value is the parent vtable EID.
pub const ATTR_DELEGATE: u64 = 22;
/// `:name` attribute, conventional human-readable label.
pub const ATTR_NAME: u64 = 23;
/// `:module` — appears in the `m` column of every fact (see VISION.md §Fact).
pub const ATTR_MODULE: u64 = 24;

// 25..63 reserved for future kernel selectors / attributes
