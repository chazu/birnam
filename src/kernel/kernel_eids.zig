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
pub const NIL_VT: u64 = 7;

/// Bootstrap module — owns the `m` column of every fact asserted during
/// kernel bootstrap. User-loaded modules get user-range EIDs.
pub const M_BOOTSTRAP: u64 = 8;

/// RRB-Tree wrapper vtable. Kernel reserves the EID and pre-materializes
/// the struct in bootstrap; dylibs install methods (count, at:) on it
/// so collect-aggregate results behave like normal sendable objects.
pub const RRB_VT: u64 = 9;

// 10..15 reserved for future kernel vtables / well-known entities

// -- Kernel selectors / attributes (16..63) -----------------------------------

pub const SEL_LOOKUP: u64 = 16;
pub const SEL_METHOD_AT_PUT_WITH: u64 = 17;
pub const SEL_DELEGATED: u64 = 18;

/// `:vtable` attribute. Value is the entity's vtable EID. Used by bind's
/// cold-path dispatch on EID-tagged receivers. Distinct from `SEL_VTABLE`
/// (the message selector "vtable" — see below) so method installs on the
/// `:vtable` selector don't collide with `:vtable` facts.
pub const ATTR_VTABLE: u64 = 19;
pub const SEL_INIT: u64 = 20;
pub const SEL_ALLOCATE: u64 = 21;

/// `:delegate` attribute on a vtable entity. Value is the parent vtable EID.
pub const ATTR_DELEGATE: u64 = 22;
/// `:name` attribute, conventional human-readable label.
pub const ATTR_NAME: u64 = 23;
/// `:module` — appears in the `m` column of every fact (see VISION.md §Fact).
pub const ATTR_MODULE: u64 = 24;

/// `vtable` message selector — returns the receiver's vtable when sent.
/// EID kept distinct from ATTR_VTABLE so methods on this selector don't
/// collide with the attribute fact.
pub const SEL_VTABLE: u64 = 25;

/// Saved-query infrastructure (Phase 7.3). A saved query is an entity with:
///   [saved_eid :saved-query-name <interned-name-EID-Ref>]
///   [saved_eid :saved-query-spec <pointer-Ref-to-*Query>]
pub const ATTR_SAVED_QUERY_NAME: u64 = 26;
pub const ATTR_SAVED_QUERY_SPEC: u64 = 27;

/// Rule storage walk-back (Phase 7.3.7). Rules are fact-entities:
///   [rule_eid :rule-name <interned-name-EID-Ref>]
///   [rule_eid :rule-params <SmallInt n_params>]
///   [rule_eid :rule-max-var <SmallInt max_var>]
///   [rule_eid :rule-body <pointer Ref to []const Clause>]
pub const ATTR_RULE_NAME: u64 = 28;
pub const ATTR_RULE_PARAMS: u64 = 29;
pub const ATTR_RULE_MAX_VAR: u64 = 30;
pub const ATTR_RULE_BODY: u64 = 31;

/// Phase 7.4: predicates + functions as fact-shaped entities.
///   [pred_eid :predicate-name <interned-name-EID-Ref>]
///   [pred_eid :predicate-impl <pointer-Ref to native NativePredBox, OR EID-Ref to closure>]
/// Same shape for functions with ATTR_FN_*. Native pointer Refs are
/// distinguished from closure Refs by tag at resolve time.
pub const ATTR_PRED_NAME: u64 = 32;
pub const ATTR_PRED_IMPL: u64 = 33;
pub const ATTR_FN_NAME: u64 = 34;
pub const ATTR_FN_IMPL: u64 = 35;

// 36..63 reserved for future kernel selectors / attributes
