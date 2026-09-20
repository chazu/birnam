use super::*;

unsafe extern "C" fn first() {}
unsafe extern "C" fn second() {}
fn address(method: unsafe extern "C" fn()) -> *mut c_void { method as *const () as *mut c_void }

#[test]
fn all_slots_match_exact_keys_and_clear_without_owning_methods() {
    unsafe {
        let context = create();
        assert!(!context.is_null() && lookup(context, u64::MAX, 0, 0).is_null());
        for index in 0..SLOTS {
            let selector = index << 4;
            assert_eq!(slot(selector), index);
            assert!(lookup(context, 17, 19, selector).is_null());
            insert(context, 17, 19, selector, address(first));
        }
        for index in 0..SLOTS {
            let selector = index << 4;
            assert_eq!(lookup(context, 17, 19, selector), address(first));
            assert!(lookup(context, 18, 19, selector).is_null());
            assert!(lookup(context, 17, 20, selector).is_null());
            // Bit 16 leaves the six hash bits unchanged but changes the key.
            let collision = selector | (1 << 16);
            assert_eq!(slot(collision), index);
            assert!(lookup(context, 17, 19, collision).is_null());
            assert_eq!(lookup(context, 17, 19, selector), address(first));
            insert(context, u64::MAX, usize::MAX, collision, address(second));
            assert!(lookup(context, 17, 19, selector).is_null());
            assert_eq!(lookup(context, u64::MAX, usize::MAX, collision), address(second));
        }
        clear(context);
        for index in 0..SLOTS {
            assert!(lookup(context, u64::MAX, usize::MAX, (index << 4) | (1 << 16)).is_null());
        }
        assert!(lookup(context, u64::MAX, 0, 0).is_null());
        // Clear does not prohibit the exact sentinel keys in a subsequent entry.
        insert(context, u64::MAX, 0, 0, address(first));
        assert_eq!(lookup(context, u64::MAX, 0, 0), address(first));
        clear(context);clear(context);
        assert!(lookup(context, u64::MAX, 0, 0).is_null());
        destroy(context);
    }
}

#[test]
fn moved_owners_and_destruction_do_not_share_contents() {
    unsafe {
        let mut owners = vec![create(), create()];
        assert!(owners[0] != owners[1]);
        insert(owners[0], 1, 2, 3, address(first));
        insert(owners[1], 1, 2, 3, address(second));
        let moved = owners.remove(0);
        assert_eq!(lookup(moved, 1, 2, 3), address(first));
        clear(moved);destroy(moved);
        assert_eq!(lookup(owners[0], 1, 2, 3), address(second));
        destroy(owners[0]);
    }
}
